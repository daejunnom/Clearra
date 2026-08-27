use clearra_forward_search::{ForwardSearchQuery, ForwardSearchReport};
use clearra_host_contract::AppCommandKind;
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppContext,
    app_request::{AppOutputPolicy, AppRequest},
    app_response::AppResponse,
    commands::forward_search_app_command::{forward_search_response, ForwardResponseKind},
    product_capability_contract::ValidatedProductCapabilityContract,
    AppCommand,
};

pub enum DistributedForwardPreparation {
    Ready(AppResponse),
    Search(PreparedDistributedForwardSearch),
}

pub struct PreparedDistributedForwardSearch {
    context: AppContext,
    query: ForwardSearchQuery,
    response_kind: ForwardResponseKind,
    workers: usize,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    validation_report: DiagnosticReport,
    product_capability_contract: Option<ValidatedProductCapabilityContract>,
}

impl AppContext {
    pub fn prepare_distributed_forward_search(
        &self,
        request: AppRequest,
    ) -> DistributedForwardPreparation {
        let workers = usize::from(request.resource_budget().workers()).max(1);
        let (command, output_policy, _, _, _, product_capability_contract) =
            match request.into_execution_parts() {
                Ok(execution_parts) => execution_parts,
                Err(rejection) => {
                    return DistributedForwardPreparation::Ready(
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
            return DistributedForwardPreparation::Ready(
                self.finalize_response_with_product_capability(
                    response,
                    command_kind,
                    &output_policy,
                    product_capability_contract,
                ),
            );
        }
        let (query, response_kind) = match command {
            AppCommand::Damage(command) => (command.into_query(), ForwardResponseKind::Damage),
            AppCommand::SpinFinder(command) => (command.into_query(), ForwardResponseKind::Spin),
            AppCommand::Ren(command) => (command.into_query(), ForwardResponseKind::Ren),
            _ => {
                return DistributedForwardPreparation::Ready(
                    self.finalize_response_with_product_capability(
                        AppResponse::validation_failed(DiagnosticReport::new()),
                        command_kind,
                        &output_policy,
                        product_capability_contract,
                    ),
                );
            }
        };
        DistributedForwardPreparation::Search(PreparedDistributedForwardSearch {
            context: self.clone(),
            query,
            response_kind,
            workers,
            command_kind,
            output_policy,
            validation_report,
            product_capability_contract,
        })
    }
}

impl PreparedDistributedForwardSearch {
    pub const fn query(&self) -> &ForwardSearchQuery {
        &self.query
    }

    pub const fn workers(&self) -> usize {
        self.workers
    }

    pub fn complete(self, report: ForwardSearchReport) -> AppResponse {
        let response = forward_search_response(report, self.response_kind);
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
