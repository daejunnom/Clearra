use clearra_problem::{ProblemCompiler, SetupSearchQuery};
use clearra_validation::validators::setup_query_validator::validate_setup_search_query;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::execution_error_response::core_execution_error_response,
    render::AppRenderModel,
};

#[derive(Clone, Debug, PartialEq)]
pub struct SetupAppCommand {
    query: SetupSearchQuery,
}

impl SetupAppCommand {
    pub fn new(query: SetupSearchQuery) -> Self {
        Self { query }
    }
}
impl SetupAppCommand {
    pub fn query(&self) -> &SetupSearchQuery {
        &self.query
    }
}

impl RunnableAppCommand for SetupAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let report = validate_setup_search_query(&self.query);
        if report.has_errors() {
            return AppResponse::validation_failed(report);
        }
        let problem = match ProblemCompiler::compile_setup(&self.query) {
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
            Ok(result) => AppResponse::success(AppRenderModel::Setup(result)),
            Err(error) => core_execution_error_response(error),
        }
    }
}
