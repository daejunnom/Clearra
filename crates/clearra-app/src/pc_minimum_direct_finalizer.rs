//! Native scheduling of the existing source-bound minimum-cover preparation.
//! The request thread has already handed this owner to a compute worker; only
//! admitted workers solve AtMost tasks, while the owner checks memory receipts.
use super::*;
use crate::{
    portfolio_alternative_store::CoveragePortfolioAlternativeSetPreparation,
    product_capability_result::PcMinimumCoverProductPreparationAdvance,
};
use clearra_coverage::cover::ExactMinimumCoverError;

/// Convert an inner preparation peak into the enclosing owner's whole peak.
/// The preparation owns the same immutable source, so subtract its retained
/// bytes instead of charging those source Arcs once again for each shard.
pub(crate) fn checked_parallel_preparation_outer_bytes(
    outer_inline: usize,
    outer_heap: Option<u128>,
    inner: &CoveragePortfolioAlternativeSetPreparation,
) -> Option<u128> {
    (outer_inline as u128)
        .checked_add(outer_heap?)?
        .checked_sub(core::mem::size_of_val(inner) as u128)?
        .checked_sub(inner.checked_retained_capacity_bytes()?)
}

impl AppContext {
    pub(super) fn finalize_pc_minimum_response(
        &self,
        response: AppResponse,
        command: clearra_host_contract::AppCommandKind,
        output: &AppOutputPolicy,
        resource_budget: &ResourceBudget,
        contract: ValidatedProductCapabilityContract,
        control: &ExecutionControl,
    ) -> AppResponse {
        let response = response.with_contract_context(command);
        let complete = || -> Result<ProductCapabilityResult, String> {
            let mut preparation =
                ProductCapabilityResult::prepare_pc_minimum_cover(contract, &response)
                    .map_err(|error| error.to_string())?;
            let request_mib = match (
                resource_budget.memory_mib(),
                resource_budget.max_memory_mib().map(u64::from),
            ) {
                (Some(first), Some(second)) => Some(first.min(second)),
                (first, second) => first.or(second),
            };
            let request_maximum = request_mib.map(|mib| u128::from(mib) * 1024 * 1024);
            let host_maximum = self
                .services
                .core_executor()
                .product_retention_budget()
                .map(|budget| u128::from(budget.maximum_bytes()));
            let maximum = match (request_maximum, host_maximum) {
                (Some(request), Some(host)) => Some(request.min(host)),
                (request, host) => request.or(host),
            };
            let external = response
                .checked_pc_minimals_retained_capacity_bytes()
                .and_then(|bytes| bytes.checked_add(core::mem::size_of_val(&response) as u128))
                .and_then(|bytes| bytes.checked_add(output.checked_retained_capacity_bytes()?))
                .and_then(|bytes| bytes.checked_add(self.checked_retained_capacity_bytes()?))
                .and_then(|bytes| bytes.checked_add(core::mem::size_of_val(control) as u128))
                .and_then(|bytes| {
                    bytes.checked_add(control.compute_budget().shared_allocation_retained_bytes())
                })
                .ok_or("pc_minimum_native_external_memory_projection_overflow")?;
            let mut guard = |peak: u128| {
                let whole = external
                    .checked_add(peak)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
                if maximum.is_some_and(|maximum| whole > maximum) {
                    Err(ExactMinimumCoverError::MemoryGuardRejected)
                } else {
                    Ok(())
                }
            };
            loop {
                if control.is_cancelled() {
                    return Err("pc_minimum_native_cancelled".to_owned());
                }
                let outer = checked_parallel_preparation_outer_bytes(
                    core::mem::size_of_val(&preparation),
                    preparation.checked_retained_capacity_bytes(),
                    preparation.parallel_work(),
                )
                .ok_or("pc_minimum_native_memory_projection_overflow")?;
                preparation
                    .parallel_work_mut()
                    .drive_native_parallel_tasks(
                        control,
                        &mut |peak| {
                            guard(
                                outer
                                    .checked_add(peak)
                                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                            )
                        },
                        &mut || control.is_cancelled(),
                    )
                    .map_err(|error| error.as_str().to_owned())?;
                match preparation
                    .advance_with_memory_guard(1_024, &mut guard, &mut || control.is_cancelled())
                    .map_err(|error| error.to_string())?
                {
                    PcMinimumCoverProductPreparationAdvance::Pending { work_steps } => {
                        control.report_progress("pc-minimals-finalize", work_steps, None);
                    }
                    PcMinimumCoverProductPreparationAdvance::Cancelled { .. } => {
                        return Err("pc_minimum_native_cancelled".to_owned());
                    }
                    PcMinimumCoverProductPreparationAdvance::Completed(product) => {
                        return Ok(product);
                    }
                }
            }
        };
        match complete() {
            Ok(product) => self.finalize_response_with_prevalidated_product_capability(
                response, command, output, product,
            ),
            Err(error) => self.finalize_response(
                AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ExecutionFailed, error),
                ),
                command,
                output,
            ),
        }
    }
}
