//! Native/direct facade over the SAME bounded replay manifest used by hosts.
//! No eager full-height trace family or separate CLI/Discord reducer is built.
use super::*;
use clearra_core_executor::CoreExecutionError;

impl AppContext {
    pub(super) fn finalize_full_height_replay_response(
        &self,
        response: AppResponse,
        command: clearra_host_contract::AppCommandKind,
        output: &AppOutputPolicy,
        contract: ValidatedProductCapabilityContract,
        control: &ExecutionControl,
    ) -> AppResponse {
        let response = response.with_contract_context(command);
        let build = || -> Result<ProductCapabilityResult, CoreExecutionError> {
            if control.is_cancelled() {
                return Err(CoreExecutionError::Cancelled);
            }
            let (query, _) =
                contract
                    .pc_path_binding()
                    .ok_or(CoreExecutionError::RuntimeUnavailable {
                        component: "pc.path query missing",
                    })?;
            let problem = query
                .compile_expected()
                .map_err(|component| CoreExecutionError::RuntimeUnavailable { component })?;
            let reserve = crate::cooperative_execution::checked_pc_replay_external_reserve(
                self, &response, output, &contract, &problem,
            )
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: "complete_replay_app_owner_projection_overflow",
            })?;
            let core = response
                .render_model()
                .and_then(crate::render::AppRenderModel::core_result)
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: "pc.path core source missing",
                })?;
            let mut preparation = self
                .services()
                .core_executor()
                .prepare_pc_replay_page_source(&problem, core, reserve)?;
            while !preparation
                .advance(8192, control)
                .map_err(replay_core_error)?
            {}
            let source = preparation.complete().map_err(replay_core_error)?;
            // Only a completed manifest, whose constructor verified the Core
            // source proof, enters the public typed result validator.
            ProductCapabilityResult::validate_with_pc_replay_source(contract, &response, source)
                .map_err(|e| CoreExecutionError::Pc(e.to_string()))
        };
        match build() {
            Ok(product) => self.finalize_response_with_prevalidated_product_capability(
                response, command, output, product,
            ),
            Err(error) => self.finalize_response(
                crate::commands::core_execution_error_response(error),
                command,
                output,
            ),
        }
    }
}

fn replay_core_error(error: crate::pc_replay_page_error::PcReplayPageError) -> CoreExecutionError {
    if error.code() == "complete_replay_cancelled" {
        CoreExecutionError::Cancelled
    } else {
        CoreExecutionError::Pc(error.to_string())
    }
}
