//! Shared owner of typed pc.path finalization. Direct callers drain the same
//! exact page manifest that cooperative hosts advance with their work budget.
use super::*;
use crate::{pc_replay_page_error::PcReplayPageError, PcReplaySourceBuildSession};
use clearra_core_executor::CoreExecutionError;

/// Existing diagnostics on the two outer execution APIs. This does not select
/// a reducer, a result mode, or a different Core producer.
pub(crate) enum PcReplayFinalizerHost {
    Direct,
    Cooperative,
}

pub(crate) struct TypedPcReplayFinalizer {
    contract: ValidatedProductCapabilityContract,
    preparation: PcReplaySourceBuildSession,
}

impl TypedPcReplayFinalizer {
    pub(crate) fn prepare(
        context: &AppContext,
        response: &AppResponse,
        output: &AppOutputPolicy,
        contract: ValidatedProductCapabilityContract,
        host: PcReplayFinalizerHost,
    ) -> Result<Self, CoreExecutionError> {
        let (missing_binding, missing_source) = match host {
            PcReplayFinalizerHost::Direct => {
                ("pc.path query missing", "pc.path core source missing")
            }
            PcReplayFinalizerHost::Cooperative => (
                "complete_replay_query_binding_missing",
                "complete_replay_response_source_missing",
            ),
        };
        let (query, _) =
            contract
                .pc_path_binding()
                .ok_or(CoreExecutionError::RuntimeUnavailable {
                    component: missing_binding,
                })?;
        let problem = query
            .compile_expected()
            .map_err(|component| CoreExecutionError::RuntimeUnavailable { component })?;
        let reserve = crate::cooperative_execution::checked_pc_replay_external_reserve(
            context, response, output, &contract, &problem,
        )
        .ok_or(CoreExecutionError::RuntimeUnavailable {
            component: "complete_replay_app_owner_projection_overflow",
        })?;
        let core = response
            .render_model()
            .and_then(crate::render::AppRenderModel::core_result)
            .ok_or(CoreExecutionError::RuntimeUnavailable {
                component: missing_source,
            })?;
        let preparation = context
            .services()
            .core_executor()
            .prepare_pc_replay_page_source(&problem, core, reserve)?;
        Ok(Self {
            contract,
            preparation,
        })
    }

    pub(crate) fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<bool, PcReplayPageError> {
        self.preparation.advance(work_budget, control)
    }

    pub(crate) fn seal(self, response: &AppResponse) -> Result<ProductCapabilityResult, String> {
        let source = self
            .preparation
            .complete()
            .map_err(|error| error.to_string())?;
        // The source supplies the graph's exact witness ranks and counts.
        // The product validator still independently verifies the request and
        // result authority, so neither host can choose a narrower reducer.
        ProductCapabilityResult::validate_with_pc_replay_source(self.contract, response, source)
            .map_err(|error| error.to_string())
    }
}

impl AppContext {
    pub(super) fn finalize_pc_replay_response(
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
            let mut finalizer = TypedPcReplayFinalizer::prepare(
                self,
                &response,
                output,
                contract,
                PcReplayFinalizerHost::Direct,
            )?;
            while !finalizer
                .advance(8192, control)
                .map_err(replay_core_error)?
            {}
            finalizer.seal(&response).map_err(CoreExecutionError::Pc)
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

fn replay_core_error(error: PcReplayPageError) -> CoreExecutionError {
    if error.code() == "complete_replay_cancelled" {
        CoreExecutionError::Cancelled
    } else {
        CoreExecutionError::Pc(error.to_string())
    }
}
