use clearra_pc_graph::request::PcScenarioQuery;
use clearra_problem::ProblemCompiler;
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;
use clearra_validation::validators::pc_query_validator::validate_pc_scenario_query;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::{
        execution_error_response::core_execution_error_response, ScenarioAppRenderContract,
    },
    render::AppRenderModel,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioAppCommand {
    query: PcScenarioQuery,
    render_contract: Option<ScenarioAppRenderContract>,
}

impl ScenarioAppCommand {
    pub fn new(query: PcScenarioQuery) -> Self {
        Self {
            query,
            render_contract: None,
        }
    }
}
impl ScenarioAppCommand {
    pub fn with_render_contract(mut self, render_contract: ScenarioAppRenderContract) -> Self {
        self.render_contract = Some(render_contract);
        self
    }
}
impl ScenarioAppCommand {
    pub fn query(&self) -> &PcScenarioQuery {
        &self.query
    }

    pub(crate) fn into_search_parts(self) -> (PcScenarioQuery, Option<ScenarioAppRenderContract>) {
        (self.query, self.render_contract)
    }
}

impl RunnableAppCommand for ScenarioAppCommand {
    fn validation_failed_response(&self, report: DiagnosticReport) -> Option<AppResponse> {
        self.render_contract
            .as_ref()
            .and_then(|contract| contract.validation_failed_response(report))
    }

    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let report = validate_pc_scenario_query(&self.query);
        if report.has_errors() {
            return AppResponse::validation_failed(report);
        }
        let problem = match ProblemCompiler::compile_scenario_pc(&self.query) {
            Ok(problem) => problem,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
                )
            }
        };
        match context
            .services()
            .core_executor()
            .execute_with_control(&problem, context.execution_control())
        {
            Ok(result) => {
                if let Some(contract) = self.render_contract {
                    return contract.success_response(result);
                }
                AppResponse::success(AppRenderModel::Scenario(result))
            }
            Err(error) => core_execution_error_response(error),
        }
    }
}
