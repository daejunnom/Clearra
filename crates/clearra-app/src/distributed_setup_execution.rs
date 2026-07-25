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
    render::AppRenderModel,
    AppCommand,
};

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
}

impl AppContext {
    pub fn prepare_distributed_setup_search(
        &self,
        request: AppRequest,
    ) -> DistributedSetupPreparation {
        let command_kind = request.command_kind();
        let workers = usize::from(request.resource_budget().workers()).max(1);
        let (command, output_policy, _, _) = request.into_parts();
        let query = match command {
            AppCommand::Setup(command) => command.into_query(),
            _ => {
                return DistributedSetupPreparation::Ready(self.finalize_response(
                    AppResponse::validation_failed(DiagnosticReport::new()),
                    command_kind,
                    &output_policy,
                ));
            }
        };
        let validation_report = validate_setup_search_query(&query);
        if validation_report.has_errors() {
            return DistributedSetupPreparation::Ready(self.finalize_response(
                AppResponse::validation_failed(validation_report),
                command_kind,
                &output_policy,
            ));
        }
        DistributedSetupPreparation::Search(PreparedDistributedSetupSearch {
            context: self.clone(),
            query,
            workers,
            command_kind,
            output_policy,
            validation_report,
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
        let response = AppResponse::success(AppRenderModel::Setup(result));
        let response = if self.validation_report.is_empty() {
            response
        } else {
            response.with_validation_diagnostics(self.validation_report)
        };
        self.context
            .finalize_response(response, self.command_kind, &self.output_policy)
    }
}
