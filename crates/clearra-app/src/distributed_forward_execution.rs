use clearra_forward_search::{ForwardSearchQuery, ForwardSearchReport};
use clearra_host_contract::AppCommandKind;
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppContext,
    app_request::{AppOutputPolicy, AppRequest},
    app_response::AppResponse,
    commands::forward_search_app_command::forward_search_response,
    AppCommand,
};

pub enum DistributedForwardPreparation {
    Ready(AppResponse),
    Search(PreparedDistributedForwardSearch),
}

pub struct PreparedDistributedForwardSearch {
    context: AppContext,
    query: ForwardSearchQuery,
    damage: bool,
    workers: usize,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    validation_report: DiagnosticReport,
}

impl AppContext {
    pub fn prepare_distributed_forward_search(
        &self,
        request: AppRequest,
    ) -> DistributedForwardPreparation {
        let command_kind = request.command_kind();
        let workers = usize::from(request.resource_budget().workers()).max(1);
        let (command, output_policy, _, _) = request.into_parts();
        let validation_report = command.validate();
        if validation_report.has_errors() {
            let response = command
                .validation_failed_response(validation_report.clone())
                .unwrap_or_else(|| AppResponse::validation_failed(validation_report));
            return DistributedForwardPreparation::Ready(self.finalize_response(
                response,
                command_kind,
                &output_policy,
            ));
        }
        let (query, damage) = match command {
            AppCommand::Damage(command) => (command.into_query(), true),
            AppCommand::SpinFinder(command) => (command.into_query(), false),
            _ => {
                return DistributedForwardPreparation::Ready(self.finalize_response(
                    AppResponse::validation_failed(DiagnosticReport::new()),
                    command_kind,
                    &output_policy,
                ));
            }
        };
        DistributedForwardPreparation::Search(PreparedDistributedForwardSearch {
            context: self.clone(),
            query,
            damage,
            workers,
            command_kind,
            output_policy,
            validation_report,
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
        let response = forward_search_response(report, self.damage);
        let response = if self.validation_report.is_empty() {
            response
        } else {
            response.with_validation_diagnostics(self.validation_report)
        };
        self.context
            .finalize_response(response, self.command_kind, &self.output_policy)
    }
}
