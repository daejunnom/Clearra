use clearra_core_executor::service::pc_continuation_fields::attach_exact_fixed_no_hold_2l_opening_continuation;
use clearra_core_executor::CoreExecutionResult;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcContinuationToken, PcContinuationTokenCodec, PcScenarioQuery,
    RequestedSearchBackend,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
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

    /// A routable local replay; every other token or error stays on Continue's
    /// ordinary path, including its pre-execution cancellation behavior.
    pub(crate) fn compile_cooperative_sr3(&self) -> Option<SearchProblem> {
        let token = self
            .token
            .as_deref()
            .filter(|token| token.starts_with("sr3:"))?;
        let PcContinuationToken::ScenarioReplay(query) =
            PcContinuationTokenCodec::parse(token).ok()?
        else {
            return None;
        };
        let problem = compile_scenario_restart(&query, ScenarioRestartKind::Replay).ok()?;
        (problem.visible_height() > 6
            && problem.backend_request().requested_backend() == RequestedSearchBackend::Cpu
            && problem.backend_policy().workers() == 1)
            .then_some(problem)
    }

    pub(crate) fn cooperative_sr3_response(self, result: CoreExecutionResult) -> AppResponse {
        match self.token {
            Some(token) => scenario_restart_response(ScenarioRestartKind::Replay, &token, result),
            None => missing_continue_token_response(),
        }
    }
}

impl RunnableAppCommand for ContinueAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let Some(token) = self.token else {
            return missing_continue_token_response();
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
            PcContinuationToken::Scenario(query) => {
                run_scenario_continue(context, query, &token, ScenarioRestartKind::Continuation)
            }
            PcContinuationToken::ScenarioReplay(query) => {
                run_scenario_continue(context, query, &token, ScenarioRestartKind::Replay)
            }
        }
    }
}

fn missing_continue_token_response() -> AppResponse {
    AppResponse::failed(
        AppStatus::ExecutionFailed,
        AppError::new(
            AppErrorCode::ContinueTokenRequired,
            "continue requires a token from a previous PC result",
        ),
    )
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
        Ok(result) => {
            let result = if context
                .services()
                .core_executor()
                .supports_cooperative_wasm_search()
            {
                attach_exact_fixed_no_hold_2l_opening_continuation(&problem, result)
            } else {
                result
            };
            AppResponse::success(AppRenderModel::Continue(AppMessage::new(
                AppResultKind::Continue,
                continue_fields(status, continuation_kind, token, result.summary_fields()),
            )))
        }
        Err(error) => core_execution_error_response(error),
    }
}

#[derive(Clone, Copy)]
enum ScenarioRestartKind {
    Continuation,
    Replay,
}

fn run_scenario_continue(
    context: &AppExecutionContext<'_>,
    query: PcScenarioQuery,
    token: &str,
    restart: ScenarioRestartKind,
) -> AppResponse {
    let problem = match compile_scenario_restart(&query, restart) {
        Ok(problem) => problem,
        Err(response) => return response,
    };
    match context
        .services()
        .core_executor()
        .execute_with_control(&problem, context.execution_control())
    {
        Ok(result) => scenario_restart_response(restart, token, result),
        Err(error) => core_execution_error_response(error),
    }
}

// Preserve the original typed validation/compile failure on both App drivers.
#[allow(clippy::result_large_err)]
fn compile_scenario_restart(
    query: &PcScenarioQuery,
    restart: ScenarioRestartKind,
) -> Result<SearchProblem, AppResponse> {
    let report = validate_pc_scenario_query(query);
    if report.has_errors() {
        return Err(AppResponse::validation_failed(report));
    }
    let compiled = match restart {
        ScenarioRestartKind::Continuation => ProblemCompiler::compile_scenario_pc(query),
        ScenarioRestartKind::Replay => ProblemCompiler::compile_legacy_scenario_pc(query),
    };
    compiled.map_err(|error| {
        AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
        )
    })
}

fn scenario_restart_response(
    restart: ScenarioRestartKind,
    token: &str,
    result: CoreExecutionResult,
) -> AppResponse {
    let (status, continuation_kind) = match restart {
        ScenarioRestartKind::Continuation => ("scenario-continued-searched", "scenario"),
        ScenarioRestartKind::Replay => ("scenario-replayed-searched", "scenario-replay"),
    };
    AppResponse::success(AppRenderModel::Continue(AppMessage::new(
        AppResultKind::Continue,
        continue_fields(status, continuation_kind, token, result.summary_fields()),
    )))
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
