use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{CoreExecutionError, CoreExecutionResult};
use clearra_host_contract::AppCommandKind;
use clearra_problem::{BuildProbabilityAggregation, BuildProbabilityField, SearchProblem};
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppContext,
    app_request::{AppOutputPolicy, AppRequest},
    app_response::AppResponse,
    commands::core_execution_error_response,
    cooperative_execution::{
        compile_search_command, response_from_search, CooperativeSearchResponseKind,
    },
};

pub enum DistributedSearchPreparation {
    Ready(AppResponse),
    Search(PreparedDistributedSearch),
}

pub struct PreparedDistributedSearch {
    context: AppContext,
    problem: SearchProblem,
    response_kind: CooperativeSearchResponseKind,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    validation_report: DiagnosticReport,
}

impl AppContext {
    pub fn prepare_distributed_search(&self, request: AppRequest) -> DistributedSearchPreparation {
        let command_kind = request.command_kind();
        let (command, output_policy, _, _) = request.into_parts();
        let validation_report = command.validate();
        if validation_report.has_errors() {
            let response = command
                .validation_failed_response(validation_report.clone())
                .unwrap_or_else(|| AppResponse::validation_failed(validation_report));
            return DistributedSearchPreparation::Ready(self.finalize_response(
                response,
                command_kind,
                &output_policy,
            ));
        }

        let (problem, response_kind) = match compile_search_command(command) {
            Ok(compiled) => compiled,
            Err(response) => {
                return DistributedSearchPreparation::Ready(self.finalize_response(
                    response,
                    command_kind,
                    &output_policy,
                ));
            }
        };
        DistributedSearchPreparation::Search(PreparedDistributedSearch {
            context: self.clone(),
            problem,
            response_kind,
            command_kind,
            output_policy,
            validation_report,
        })
    }
}

impl PreparedDistributedSearch {
    pub fn problem(&self) -> &SearchProblem {
        &self.problem
    }

    pub fn build_probability_request(
        &self,
    ) -> Option<(BuildProbabilityField, BuildProbabilityAggregation)> {
        match self.response_kind {
            CooperativeSearchResponseKind::BuildProbability { field, aggregation } => {
                Some((field, aggregation))
            }
            _ => None,
        }
    }

    pub fn complete(self, result: CoreExecutionResult, control: &ExecutionControl) -> AppResponse {
        let response = match self
            .context
            .services()
            .core_executor()
            .postprocess_search_result(result, control)
        {
            Ok(result) => response_from_search(self.response_kind, result),
            Err(error) => core_execution_error_response(error),
        };
        let response = if self.validation_report.is_empty() {
            response
        } else {
            response.with_validation_diagnostics(self.validation_report)
        };
        self.context
            .finalize_response(response, self.command_kind, &self.output_policy)
    }

    pub fn fail(self, error: CoreExecutionError) -> AppResponse {
        self.context.finalize_response(
            core_execution_error_response(error),
            self.command_kind,
            &self.output_policy,
        )
    }
}
