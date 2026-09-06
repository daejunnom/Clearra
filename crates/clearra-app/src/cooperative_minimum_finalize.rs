// SRP rationale: one change reason is routing typed minimum-product work to
// the shared exact continuation without moving proof authority into a host.

use super::*;
use crate::{
    build_probability_product_result::{
        BuildScoreMinimumPreparation, BuildScoreMinimumPreparationAdvance,
    },
    build_solution_probability_result::build_v2_facade::{
        BuildCoverV2Preparation, BuildCoverV2PreparationAdvance, BuildCoveragePortfolioV2,
    },
    pc_score_minimum_cover_result::{
        PcScorePortfolioExecutionPreparation, PcScorePortfolioPreparationAdvance,
        ValidatedPcScorePortfolioExecutionEvidence,
    },
    portfolio_alternative_store::CoveragePortfolioAlternativeSetPreparation,
};
use clearra_coverage::cover::ExactMinimumCoverError;

// The enclosing finalizer is already boxed and memory-counted as one owner;
// preserving inline variants avoids unadmitted per-product heap boxes.
#[allow(clippy::large_enum_variant)]
pub(super) enum CooperativeMinimumPreparation {
    Pc(PcMinimumCoverProductPreparation),
    PcScore {
        preparation: PcScorePortfolioExecutionPreparation,
        contract: Option<ValidatedProductCapabilityContract>,
    },
    BuildCover(BuildCoverV2Preparation),
    BuildScore(BuildScoreMinimumPreparation),
}

#[allow(clippy::large_enum_variant)]
pub(super) enum CooperativeMinimumCompletion {
    Pc(ProductCapabilityResult),
    PcScore(
        ValidatedPcScorePortfolioExecutionEvidence,
        ValidatedProductCapabilityContract,
    ),
    BuildCover(BuildCoveragePortfolioV2),
    BuildScore(
        clearra_host_contract::ProductResultPayload,
        crate::ProductPageSourceOwner,
    ),
}

#[allow(clippy::large_enum_variant)]
pub(super) enum CooperativeMinimumAdvance {
    Pending { work_steps: u64 },
    Completed(CooperativeMinimumCompletion),
    Cancelled { work_steps: u64 },
}

impl CooperativeMinimumPreparation {
    pub(super) fn parallel_source_dimensions(&self) -> Option<(usize, usize)> {
        self.parallel_work().parallel_source_dimensions()
    }
    pub(super) fn enable_parallel(&mut self, partitions: usize) -> Result<(), &'static str> {
        self.parallel_work_mut()
            .enable_parallel(partitions)
            .map_err(|_| "minimum parallel preparation rejected")
    }
    pub(super) fn parallel_query_satisfied(&self) -> bool {
        self.parallel_work().parallel_query_satisfied()
    }
    pub(super) fn parallel_query(&self) -> Option<&clearra_coverage::cover::ExactAtMostQuery> {
        self.parallel_work().parallel_query()
    }
    pub(super) fn take_parallel_task(
        &mut self,
    ) -> Option<clearra_coverage::cover::ExactAtMostTask> {
        self.parallel_work_mut().take_parallel_task()
    }
    pub(super) fn accept_parallel_receipt(
        &mut self,
        receipt: clearra_coverage::cover::ExactAtMostReceipt,
    ) -> Result<(), &'static str> {
        self.parallel_work_mut()
            .accept_parallel_receipt(receipt)
            .map_err(|_| "minimum parallel receipt rejected")
    }
    pub(super) fn prepare_parallel_idle_assist(
        &mut self,
        maximum_children: usize,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<bool, &'static str> {
        self.parallel_work_mut()
            .prepare_parallel_idle_assist(maximum_children, guard)
            .map_err(|_| "minimum parallel assistance rejected")
    }
    pub(super) fn parallel_task_is_redundant(
        &self,
        identity: clearra_coverage::cover::ExactAtMostQueryIdentity,
        partition_id: u64,
    ) -> Result<bool, &'static str> {
        self.parallel_work()
            .parallel_task_is_redundant(identity, partition_id)
            .map_err(|_| "minimum parallel task identity rejected")
    }

    pub(super) fn parallel_work(&self) -> &CoveragePortfolioAlternativeSetPreparation {
        match self {
            Self::Pc(work) => work.parallel_work(),
            Self::PcScore { preparation, .. } => preparation.parallel_work(),
            Self::BuildCover(work) => work.parallel_work(),
            Self::BuildScore(work) => work.parallel_work(),
        }
    }

    pub(super) fn parallel_work_mut(&mut self) -> &mut CoveragePortfolioAlternativeSetPreparation {
        match self {
            Self::Pc(work) => work.parallel_work_mut(),
            Self::PcScore { preparation, .. } => preparation.parallel_work_mut(),
            Self::BuildCover(work) => work.parallel_work_mut(),
            Self::BuildScore(work) => work.parallel_work_mut(),
        }
    }

    pub(super) fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::Pc(work) => work.checked_retained_capacity_bytes(),
            Self::PcScore {
                preparation,
                contract,
            } => preparation
                .checked_retained_capacity_bytes()?
                .checked_add(match contract {
                    Some(contract) => {
                        contract.checked_score_minimum_cover_retained_capacity_bytes()?
                    }
                    None => 0,
                }),
            Self::BuildCover(work) => work.checked_retained_capacity_bytes(),
            Self::BuildScore(work) => work.checked_retained_capacity_bytes(),
        }
    }

    pub(super) fn checked_response_heap(&self, response: &AppResponse) -> Option<u128> {
        match self {
            Self::BuildCover(_) | Self::BuildScore(_) => response.checked_retained_capacity_bytes(),
            Self::Pc(_) | Self::PcScore { .. } => {
                response.checked_pc_minimals_retained_capacity_bytes()
            }
        }
    }

    pub(super) const fn progress_stage(&self) -> &'static str {
        match self {
            Self::Pc(_) => "pc-minimals-finalize",
            Self::PcScore { .. } => "pc-score-minimals-finalize",
            Self::BuildCover(_) => "build-minimals-finalize",
            Self::BuildScore(_) => "build-score-minimals-finalize",
        }
    }

    pub(super) fn advance_with_memory_guard(
        &mut self,
        work: u64,
        guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
        cancelled: &mut impl FnMut() -> bool,
    ) -> Result<CooperativeMinimumAdvance, &'static str> {
        let enum_size = core::mem::size_of::<Self>() as u128;
        match self {
            Self::Pc(preparation) => {
                let extra = enum_size - core::mem::size_of_val(preparation) as u128;
                match preparation
                    .advance_with_memory_guard(
                        work,
                        &mut |peak| {
                            guard(
                                extra
                                    .checked_add(peak)
                                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                            )
                        },
                        cancelled,
                    )
                    .map_err(|_| "pc_minimum_product_preparation_failed")?
                {
                    PcMinimumCoverProductPreparationAdvance::Pending { work_steps } => {
                        Ok(CooperativeMinimumAdvance::Pending { work_steps })
                    }
                    PcMinimumCoverProductPreparationAdvance::Cancelled { work_steps } => {
                        Ok(CooperativeMinimumAdvance::Cancelled { work_steps })
                    }
                    PcMinimumCoverProductPreparationAdvance::Completed(result) => {
                        Ok(CooperativeMinimumAdvance::Completed(
                            CooperativeMinimumCompletion::Pc(result),
                        ))
                    }
                }
            }
            Self::PcScore {
                preparation,
                contract,
            } => {
                let contract_bytes = match contract {
                    Some(contract) => {
                        contract.checked_score_minimum_cover_retained_capacity_bytes()
                    }
                    None => Some(0),
                }
                .ok_or("pc_score_minimum_contract_memory_overflow")?;
                let extra = (enum_size - core::mem::size_of_val(preparation) as u128)
                    .checked_add(contract_bytes)
                    .ok_or("pc_score_minimum_contract_memory_overflow")?;
                match preparation
                    .advance_with_memory_guard(
                        work,
                        &mut |peak| {
                            guard(
                                extra
                                    .checked_add(peak)
                                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                            )
                        },
                        cancelled,
                    )
                    .map_err(|error| error.as_str())?
                {
                    PcScorePortfolioPreparationAdvance::Pending { work_steps } => {
                        Ok(CooperativeMinimumAdvance::Pending { work_steps })
                    }
                    PcScorePortfolioPreparationAdvance::Cancelled { work_steps } => {
                        *contract = None;
                        Ok(CooperativeMinimumAdvance::Cancelled { work_steps })
                    }
                    PcScorePortfolioPreparationAdvance::Completed(evidence) => {
                        Ok(CooperativeMinimumAdvance::Completed(
                            CooperativeMinimumCompletion::PcScore(
                                evidence,
                                contract.take().ok_or("pc_score_minimum_contract_missing")?,
                            ),
                        ))
                    }
                }
            }
            Self::BuildCover(preparation) => {
                let extra = enum_size - core::mem::size_of_val(preparation) as u128;
                match preparation
                    .advance_with_memory_guard(
                        work,
                        &mut |peak| {
                            guard(
                                extra
                                    .checked_add(peak)
                                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                            )
                        },
                        cancelled,
                    )
                    .map_err(|_| "build_minimum_product_preparation_failed")?
                {
                    BuildCoverV2PreparationAdvance::Pending { work_steps } => {
                        Ok(CooperativeMinimumAdvance::Pending { work_steps })
                    }
                    BuildCoverV2PreparationAdvance::Cancelled { work_steps } => {
                        Ok(CooperativeMinimumAdvance::Cancelled { work_steps })
                    }
                    BuildCoverV2PreparationAdvance::Completed(result) => {
                        Ok(CooperativeMinimumAdvance::Completed(
                            CooperativeMinimumCompletion::BuildCover(result),
                        ))
                    }
                }
            }
            Self::BuildScore(preparation) => {
                let extra = enum_size - core::mem::size_of_val(preparation) as u128;
                match preparation.advance_with_memory_guard(
                    work,
                    &mut |peak| {
                        guard(
                            extra
                                .checked_add(peak)
                                .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
                        )
                    },
                    cancelled,
                )? {
                    BuildScoreMinimumPreparationAdvance::Pending { work_steps } => {
                        Ok(CooperativeMinimumAdvance::Pending { work_steps })
                    }
                    BuildScoreMinimumPreparationAdvance::Cancelled { work_steps } => {
                        Ok(CooperativeMinimumAdvance::Cancelled { work_steps })
                    }
                    BuildScoreMinimumPreparationAdvance::Completed((payload, owner)) => {
                        Ok(CooperativeMinimumAdvance::Completed(
                            CooperativeMinimumCompletion::BuildScore(payload, owner),
                        ))
                    }
                }
            }
        }
    }
}

impl CooperativeAppExecution {
    /// Keep source validation and preparation under the still-live producer
    /// lease, then drop that lease before exact tasks acquire their own slice.
    pub(super) fn prepare_build_minimum_continuation(
        &mut self,
        mut postprocess: CooperativePostprocessExecution,
        result: Result<clearra_core_executor::CoreExecutionResult, CoreExecutionError>,
        derivation: Option<PcScoreDerivation>,
        control: &ExecutionControl,
    ) -> CooperativeAppAdvance {
        let outcome = (|| {
            let result = result?;
            if control.is_cancelled() {
                return Err(CoreExecutionError::Cancelled);
            }
            if postprocess.product_capability_contract.is_some() {
                return Err(CoreExecutionError::RuntimeUnavailable {
                    component: "build_minimum_unexpected_product_contract",
                });
            }
            let projection_error = || CoreExecutionError::RuntimeUnavailable {
                component: "build_minimum_source_memory_projection_overflow",
            };
            let kind_heap = match &postprocess.response_kind {
                CooperativeSearchResponseKind::BuildCover {
                    request,
                    expected_problem,
                } => request
                    .query()
                    .checked_retained_capacity_bytes()
                    .and_then(|bytes| {
                        bytes.checked_add(
                            expected_problem.checked_build_probability_pointee_retained_bytes()?,
                        )
                    })
                    .and_then(|bytes| {
                        bytes.checked_add((2 * core::mem::size_of::<usize>()) as u128)
                    }),
                CooperativeSearchResponseKind::BuildProbability {
                    result_command: Some(command),
                    finesse,
                    ..
                } => command
                    .query()
                    .checked_retained_capacity_bytes()
                    .and_then(|bytes| bytes.checked_add(finesse.checked_retained_capacity_bytes()?))
                    .and_then(|bytes| {
                        bytes
                            .checked_add(core::mem::size_of::<BuildProbabilityAppCommand>() as u128)
                    }),
                _ => None,
            }
            .ok_or_else(projection_error)?;
            let external = self
                .context()
                .checked_retained_capacity_bytes()
                .and_then(|bytes| {
                    bytes.checked_add(
                        (core::mem::size_of::<Self>()
                            + core::mem::size_of::<CooperativePostprocessExecution>()
                            + core::mem::size_of::<CooperativeMinimumFinalizeExecution>())
                            as u128,
                    )
                })
                .and_then(|bytes| {
                    bytes.checked_add(
                        postprocess
                            .output_policy
                            .checked_retained_capacity_bytes()?,
                    )
                })
                .and_then(|bytes| {
                    bytes.checked_add(
                        postprocess
                            .validation_report
                            .checked_retained_capacity_bytes()?,
                    )
                })
                .and_then(|bytes| bytes.checked_add(kind_heap))
                .and_then(|bytes| {
                    bytes.checked_add(match &derivation {
                        Some(derivation) => derivation.checked_retained_capacity_bytes()?,
                        None => 0,
                    })
                })
                .ok_or_else(projection_error)?;
            let request_limit =
                checked_cooperative_request_memory_limit_bytes(postprocess.resource_budget)?;
            let mut original_guard_error = None;
            let mut guard = |preparation_peak: u128| {
                let checked = (|| {
                    let future = external
                        .checked_add(preparation_peak)
                        .ok_or_else(projection_error)?;
                    let session = postprocess.build_probability_session.as_ref().ok_or(
                        CoreExecutionError::RuntimeUnavailable {
                            component: "build_minimum_source_memory_authority_missing",
                        },
                    )?;
                    session
                        .validate_public_result_memory_with_future(&result, future)
                        .map_err(WasmCpuSearchError::into_core_execution_error)?;
                    if let Some(limit) = request_limit {
                        let whole = result
                            .checked_resource_retained_bytes()
                            .and_then(|bytes| bytes.checked_add(future))
                            .ok_or_else(projection_error)?;
                        validate_finite_cooperative_memory_requirement(
                            whole,
                            limit,
                            "build_minimum_source_memory_limit_exceeded",
                        )?;
                    }
                    Ok(())
                })();
                if let Err(error) = checked {
                    original_guard_error = Some(error);
                    return Err(ExactMinimumCoverError::MemoryGuardRejected);
                }
                Ok(())
            };
            // Move the request out without cloning its owned queues/profile.
            // The placeholder cannot escape this one-shot continuation.
            let kind = core::mem::replace(
                &mut postprocess.response_kind,
                CooperativeSearchResponseKind::Damage,
            );
            let (response, preparation) = match kind {
                CooperativeSearchResponseKind::BuildCover {
                    request,
                    expected_problem,
                } => {
                    let prepared =
                        request.prepare_from_result(&result, &expected_problem, &mut guard);
                    let preparation = prepared.map_err(|_| {
                        original_guard_error.take().unwrap_or(
                            CoreExecutionError::RuntimeUnavailable {
                                component: "build_minimum_source_rejected",
                            },
                        )
                    })?;
                    drop(expected_problem);
                    (
                        AppResponse::success(AppRenderModel::BuildProbability(result)),
                        CooperativeMinimumPreparation::BuildCover(preparation),
                    )
                }
                CooperativeSearchResponseKind::BuildProbability {
                    result_command: Some(command),
                    ..
                } if command.result_mode()
                    == BuildProbabilityResultMode::HighestScoreMinimumSet =>
                {
                    let derivation =
                        derivation
                            .as_ref()
                            .ok_or(CoreExecutionError::RuntimeUnavailable {
                                component: "build_score_minimum_derivation_missing",
                            })?;
                    let prepared = crate::build_probability_product_result::prepare_build_highest_score_minimum_payload(
                        command.query(), &result, derivation, command.product_retention_budget(), &mut guard,
                    );
                    let preparation = prepared.map_err(|component| {
                        original_guard_error
                            .take()
                            .unwrap_or(CoreExecutionError::RuntimeUnavailable { component })
                    })?;
                    (
                        command.response_from_prepared_score_minimum(result),
                        CooperativeMinimumPreparation::BuildScore(preparation),
                    )
                }
                _ => {
                    return Err(CoreExecutionError::RuntimeUnavailable {
                        component: "build_minimum_continuation_kind_mismatch",
                    })
                }
            };
            if control.is_cancelled() {
                return Err(CoreExecutionError::Cancelled);
            }
            if response.status() != AppStatus::Success {
                return Ok((response, None));
            }
            Ok((response, Some(preparation)))
        })();
        drop(postprocess.build_probability_session.take());
        match outcome {
            Ok((response, Some(preparation))) => {
                control.report_progress(preparation.progress_stage(), 0, None);
                self.state = CooperativeExecutionState::MinimumFinalize(Box::new(
                    CooperativeMinimumFinalizeExecution {
                        response: response
                            .with_contract_context(postprocess.command_kind)
                            .with_validation_diagnostics(postprocess.validation_report),
                        command_kind: postprocess.command_kind,
                        output_policy: postprocess.output_policy,
                        resource_budget: postprocess.resource_budget,
                        preparation,
                        completed_work_steps: 0,
                    },
                ));
                CooperativeAppAdvance::Progress
            }
            Err(CoreExecutionError::Cancelled) => CooperativeAppAdvance::Cancelled,
            other => {
                let response = match other {
                    Ok((response, None)) => response,
                    Err(error) => core_execution_error_response(error),
                    Ok((_, Some(_))) => unreachable!("handled prepared continuation"),
                };
                CooperativeAppAdvance::Completed(
                    self.context().finalize_response_with_product_capability(
                        response,
                        postprocess.command_kind,
                        &postprocess.output_policy,
                        None,
                    ),
                )
            }
        }
    }
}
