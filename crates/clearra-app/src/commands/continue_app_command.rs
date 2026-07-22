use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcContinuationToken, PcContinuationTokenCodec, PcScenarioQuery,
};
use clearra_problem::ProblemCompiler;
use clearra_validation::validators::pc_query_validator::{
    validate_opening_pc_search_query, validate_pc_scenario_query,
};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::{execution_error_response::core_execution_error_response, string_field},
    render::{AppMessage, AppRenderModel, AppResultKind},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContinueAppCommand {
    token: Option<String>,
}

impl ContinueAppCommand {
    pub fn new(token: Option<String>) -> Self {
        Self { token }
    }
}

impl RunnableAppCommand for ContinueAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let Some(token) = self.token else {
            return AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::ContinueTokenRequired,
                    "continue requires a token from a previous PC result",
                ),
            );
        };
        let decoded = match PcContinuationTokenCodec::parse(&token) {
            Ok(decoded) => decoded,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ContinueTokenInvalid, error.to_string()),
                )
            }
        };
        match decoded {
            PcContinuationToken::Opening(query) => {
                run_opening_continue(context, query, &token, "continued-searched", "opening")
            }
            PcContinuationToken::Scenario(query) => run_scenario_continue(
                context,
                query,
                &token,
                "scenario-continued-searched",
                "scenario",
            ),
            PcContinuationToken::ScenarioReplay(query) => run_scenario_continue(
                context,
                query,
                &token,
                "scenario-replayed-searched",
                "scenario-replay",
            ),
        }
    }
}

fn run_opening_continue(
    context: &AppExecutionContext<'_>,
    query: OpeningPcSearchQuery,
    token: &str,
    status: &str,
    continuation_kind: &str,
) -> AppResponse {
    let report = validate_opening_pc_search_query(&query);
    if report.has_errors() {
        return AppResponse::validation_failed(report);
    }
    let problem = match ProblemCompiler::compile_opening_pc(&query) {
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
        Ok(result) => AppResponse::success(AppRenderModel::Continue(AppMessage::new(
            AppResultKind::Continue,
            continue_fields(status, continuation_kind, token, result.summary_fields()),
        ))),
        Err(error) => core_execution_error_response(error),
    }
}

fn run_scenario_continue(
    context: &AppExecutionContext<'_>,
    query: PcScenarioQuery,
    token: &str,
    status: &str,
    continuation_kind: &str,
) -> AppResponse {
    let report = validate_pc_scenario_query(&query);
    if report.has_errors() {
        return AppResponse::validation_failed(report);
    }
    let problem = match ProblemCompiler::compile_scenario_pc(&query) {
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
        Ok(result) => AppResponse::success(AppRenderModel::Continue(AppMessage::new(
            AppResultKind::Continue,
            continue_fields(status, continuation_kind, token, result.summary_fields()),
        ))),
        Err(error) => core_execution_error_response(error),
    }
}

fn continue_fields(
    status: &str,
    continuation_kind: &str,
    token: &str,
    result_fields: Vec<(String, String)>,
) -> Vec<clearra_output::model::RenderField> {
    let mut fields = vec![
        string_field("status", status),
        string_field("continuation_kind", continuation_kind),
        string_field("input_continuation_token", token),
        clearra_output::model::RenderField::new("interactive_prompt", false),
    ];
    fields.extend(
        result_fields
            .into_iter()
            .filter(|(key, _)| key != "status")
            .map(|(key, value)| string_field(key, value)),
    );
    fields
}
