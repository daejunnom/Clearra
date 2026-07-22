use clearra_problem::{BuildProbabilityQuery, ProblemCompiler};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::execution_error_response::core_execution_error_response,
    render::AppRenderModel,
};

#[derive(Clone, Debug, PartialEq)]
pub struct BuildProbabilityAppCommand {
    query: BuildProbabilityQuery,
}

impl BuildProbabilityAppCommand {
    pub fn new(query: BuildProbabilityQuery) -> Self {
        Self { query }
    }

    pub fn query(&self) -> &BuildProbabilityQuery {
        &self.query
    }
}

impl RunnableAppCommand for BuildProbabilityAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        if let Some(reason) = invalid_query_reason(&self.query) {
            return AppResponse::failed(
                AppStatus::ValidationFailed,
                AppError::new(AppErrorCode::InvalidInput, reason),
            );
        }
        let problem = match ProblemCompiler::compile_scenario_pc(self.query.core_query()) {
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
            .execute_build_probability_with_control(
                &problem,
                self.query.field(),
                self.query.aggregation(),
                context.execution_control(),
            ) {
            Ok(result) => AppResponse::success(AppRenderModel::BuildProbability(result)),
            Err(error) => core_execution_error_response(error),
        }
    }
}

pub(crate) fn invalid_query_reason(query: &BuildProbabilityQuery) -> Option<&'static str> {
    let field = query.field();
    if field.width() != 10 || field.height() == 0 || field.height() > 24 {
        return Some("build probability requires a 10-wide field between 1 and 24 rows");
    }
    if field.target().intersects(field.base()) {
        return Some("build target cells overlap the existing field");
    }
    if field.target().count_ones() % 4 != 0 {
        return Some("build target cell count must be divisible by four");
    }
    for row in 0..field.height() {
        let row_mask = clearra_core_domain::board::standard_pc_board::Board256Mask::row(
            10,
            u16::from(field.height()),
            u16::from(row),
        )
        .ok()?;
        if field.base().intersects(row_mask) && field.base().union(row_mask) == field.base() {
            return Some("existing field contains a completed row and must be normalized first");
        }
    }
    if query.core_query().exact_pieces() != Some(query.target_piece_count()) {
        return Some("build supply piece count does not match the target area");
    }
    None
}
