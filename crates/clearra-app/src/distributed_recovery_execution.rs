use clearra_forward_search::{RecoveryBuildPopulation, RecoveryBuildQuery};
use clearra_host_contract::AppCommandKind;
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppContext,
    app_request::{AppOutputPolicy, AppRequest},
    app_response::AppResponse,
    commands::recovery_build_app_command::recovery_build_response,
    product_capability_contract::ValidatedProductCapabilityContract,
    AppCommand,
};

// Preparation is a one-shot ownership transfer and its public variants are part
// of the distributed host contract, so retain their established inline shape.
#[allow(clippy::large_enum_variant)]
pub enum DistributedRecoveryBuildPreparation {
    Ready(AppResponse),
    Search(PreparedDistributedRecoveryBuildSearch),
}

pub struct PreparedDistributedRecoveryBuildSearch {
    context: AppContext,
    query: RecoveryBuildQuery,
    workers: usize,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    validation_report: DiagnosticReport,
    product_capability_contract: Option<ValidatedProductCapabilityContract>,
}

impl AppContext {
    pub fn prepare_distributed_recovery_build(
        &self,
        request: AppRequest,
    ) -> DistributedRecoveryBuildPreparation {
        let workers = usize::from(request.resource_budget().workers()).max(1);
        let (command, output_policy, _, _, _, product_capability_contract) =
            match request.into_execution_parts() {
                Ok(execution_parts) => execution_parts,
                Err(rejection) => {
                    return DistributedRecoveryBuildPreparation::Ready(
                        self.finalize_execution_parts_rejection(rejection),
                    )
                }
            };
        let command_kind = command.kind();
        let validation_report = command.validate();
        if validation_report.has_errors() {
            let response = command
                .validation_failed_response(validation_report.clone())
                .unwrap_or_else(|| AppResponse::validation_failed(validation_report));
            return DistributedRecoveryBuildPreparation::Ready(
                self.finalize_response_with_product_capability(
                    response,
                    command_kind,
                    &output_policy,
                    product_capability_contract,
                ),
            );
        }
        let query = match command {
            AppCommand::RecoveryBuild(command) => command.query().clone(),
            _ => {
                return DistributedRecoveryBuildPreparation::Ready(
                    self.finalize_response_with_product_capability(
                        AppResponse::validation_failed(DiagnosticReport::new()),
                        command_kind,
                        &output_policy,
                        product_capability_contract,
                    ),
                );
            }
        };
        DistributedRecoveryBuildPreparation::Search(PreparedDistributedRecoveryBuildSearch {
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

impl PreparedDistributedRecoveryBuildSearch {
    pub const fn query(&self) -> &RecoveryBuildQuery {
        &self.query
    }

    pub const fn workers(&self) -> usize {
        self.workers
    }

    pub fn complete(self, report: RecoveryBuildPopulation) -> AppResponse {
        let response = recovery_build_response(&self.query, report);
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
