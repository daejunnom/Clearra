use clearra_pc_graph::request::OpeningPcSearchQuery;
use clearra_problem::ProblemCompiler;
use clearra_validation::validators::pc_query_validator::validate_opening_pc_search_query;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::execution_error_response::core_execution_error_response,
    render::AppRenderModel,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcAppCommand {
    query: OpeningPcSearchQuery,
}

impl PcAppCommand {
    pub fn new(query: OpeningPcSearchQuery) -> Self {
        Self { query }
    }
}
impl PcAppCommand {
    pub fn query(&self) -> &OpeningPcSearchQuery {
        &self.query
    }
}

impl RunnableAppCommand for PcAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let report = validate_opening_pc_search_query(&self.query);
        if report.has_errors() {
            return AppResponse::validation_failed(report);
        }
        let problem = match ProblemCompiler::compile_opening_pc(&self.query) {
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
            Ok(result) => AppResponse::success(AppRenderModel::Pc(result)),
            Err(error) => core_execution_error_response(error),
        }
    }
}
