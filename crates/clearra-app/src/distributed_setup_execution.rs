use clearra_core_executor::CoreExecutionResult;
use clearra_host_contract::AppCommandKind;
use clearra_problem::SetupSearchQuery;
use clearra_validation::{
    diagnostic::diagnostic_report::DiagnosticReport,
    validators::setup_query_validator::validate_setup_search_query,
};

use crate::{
    app_context::AppContext,
    app_request::{AppOutputPolicy, AppRequest},
    app_response::AppResponse,
    commands::setup_app_command::setup_success_response,
    product_capability_contract::ValidatedProductCapabilityContract,
    AppCommand,
};

// Preparation is a one-shot ownership transfer and its public variants are part
// of the distributed host contract, so retain their established inline shape.
#[allow(clippy::large_enum_variant)]
pub enum DistributedSetupPreparation {
    Ready(AppResponse),
    Search(PreparedDistributedSetupSearch),
}

pub struct PreparedDistributedSetupSearch {
    context: AppContext,
    query: SetupSearchQuery,
    workers: usize,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    validation_report: DiagnosticReport,
    product_capability_contract: Option<ValidatedProductCapabilityContract>,
}

impl AppContext {
    pub fn prepare_distributed_setup_search(
        &self,
        request: AppRequest,
    ) -> DistributedSetupPreparation {
        let workers = usize::from(request.resource_budget().workers()).max(1);
        let (command, output_policy, _, _, _, product_capability_contract) =
            match request.into_execution_parts() {
                Ok(execution_parts) => execution_parts,
                Err(rejection) => {
                    return DistributedSetupPreparation::Ready(
                        self.finalize_execution_parts_rejection(rejection),
                    )
                }
            };
        let command_kind = command.kind();
        let query = match command {
            AppCommand::Setup(command) => command.into_query(),
            _ => {
                return DistributedSetupPreparation::Ready(
                    self.finalize_response_with_product_capability(
                        AppResponse::validation_failed(DiagnosticReport::new()),
                        command_kind,
                        &output_policy,
                        product_capability_contract,
                    ),
                );
            }
        };
        let validation_report = validate_setup_search_query(&query);
        if validation_report.has_errors() {
            return DistributedSetupPreparation::Ready(
                self.finalize_response_with_product_capability(
                    AppResponse::validation_failed(validation_report),
                    command_kind,
                    &output_policy,
                    product_capability_contract,
                ),
            );
        }
        DistributedSetupPreparation::Search(PreparedDistributedSetupSearch {
            context: self.clone(),
            query,
            workers,
            command_kind,
            output_policy,
            validation_report,
            product_capability_contract,
        })
    }
}

impl PreparedDistributedSetupSearch {
    pub const fn query(&self) -> &SetupSearchQuery {
        &self.query
    }

    pub const fn workers(&self) -> usize {
        self.workers
    }

    pub fn complete(self, result: CoreExecutionResult) -> AppResponse {
        let response = setup_success_response(&self.query, result);
        let response = if self.validation_report.is_empty() {
            response
        } else {
            response.with_validation_diagnostics(self.validation_report)
        };
        self.context.finalize_response_with_product_capability(
            response,
            self.command_kind,
            &self.output_policy,
            self.product_capability_contract,
        )
    }
}

#[cfg(test)]
mod tests {
    use clearra_problem::SetupCandidatePriority;

    use super::*;
    use crate::{commands::SetupAppCommand, render::AppRenderModel, setup_ranked_fixture};

    #[test]
    fn distributed_completion_preserves_the_ranked_family_snapshot() {
        let context = AppContext::default();
        let query = setup_ranked_fixture::query(SetupCandidatePriority::BuildProbabilityFirst);
        let request = AppRequest::new(AppCommand::Setup(SetupAppCommand::new(query.clone())));
        let DistributedSetupPreparation::Search(prepared) =
            context.prepare_distributed_setup_search(request)
        else {
            panic!("valid Setup fixture must prepare distributed execution")
        };
        let response = prepared.complete(setup_ranked_fixture::core_result(&query));
        let snapshot = response
            .render_model()
            .and_then(AppRenderModel::setup_ranked_family_snapshot)
            .expect("distributed Setup ranked-family snapshot");
        assert_eq!(snapshot.capability_id(), "setup.build");
        assert_eq!(snapshot.result_schema(), "setup-build-ranking.v2");
        assert_eq!(snapshot.candidate_count(), 1);
    }
}
