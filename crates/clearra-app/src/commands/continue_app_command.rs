use clearra_core_executor::{
    service::pc_continuation_fields::PreparedFixedOpeningContinuation, CoreExecutionError,
    CoreExecutionResult,
};
use clearra_pc_graph::request::{
    PcContinuationToken, PcContinuationTokenCodec, RequestedSearchBackend,
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

    /// Invalid input and errors during compilation stay on Continue's ordinary
    /// path, including its pre-execution cancellation behavior.
    pub(crate) fn compile_cooperative(&self) -> Option<(SearchProblem, ContinueKind)> {
        let (problem, kind) = compile_continue(self.token.as_deref()?).ok()?;
        (problem.backend_request().requested_backend() == RequestedSearchBackend::Cpu
            && problem.backend_policy().workers() == 1)
            .then_some((problem, kind))
    }

    pub(crate) fn into_cooperative_result(
        self,
        kind: ContinueKind,
        problem: &SearchProblem,
    ) -> CooperativeContinueResult {
        CooperativeContinueResult {
            token: self.token,
            kind,
            opening: kind.prepare_opening(problem).map(Box::new),
        }
    }
}

impl RunnableAppCommand for ContinueAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let Some(token) = self.token else {
            return missing_continue_token_response();
        };
        let (problem, kind) = match compile_continue(&token) {
            Ok(prepared) => prepared,
            Err(response) => return response,
        };
        let executor = context.services().core_executor();
        let control = context.execution_control();
        match executor.execute_with_control(&problem, control) {
            Ok(_) if control.is_cancelled() => {
                core_execution_error_response(CoreExecutionError::Cancelled)
            }
            Ok(result) => {
                let opening = if executor.supports_cooperative_wasm_search() {
                    kind.prepare_opening(&problem)
                } else {
                    None
                };
                continue_response(kind, &token, opening.as_ref(), result)
            }
            Err(error) => core_execution_error_response(error),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ContinueKind {
    Opening,
    ScenarioContinuation,
    ScenarioReplay,
}

impl ContinueKind {
    fn prepare_opening(&self, problem: &SearchProblem) -> Option<PreparedFixedOpeningContinuation> {
        match self {
            Self::Opening => PreparedFixedOpeningContinuation::prepare(problem),
            Self::ScenarioContinuation | Self::ScenarioReplay => None,
        }
    }
}

pub(crate) struct CooperativeContinueResult {
    token: Option<String>,
    kind: ContinueKind,
    opening: Option<Box<PreparedFixedOpeningContinuation>>,
}

impl CooperativeContinueResult {
    pub(crate) fn response(self, result: CoreExecutionResult) -> AppResponse {
        match self.token {
            Some(token) => continue_response(self.kind, &token, self.opening.as_deref(), result),
            None => missing_continue_token_response(),
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

// An opening or checkpoint starts at the token's accepted residual state.
// A replay reconstructs the original input, including its legacy extent.
#[allow(clippy::result_large_err)]
fn compile_continue(token: &str) -> Result<(SearchProblem, ContinueKind), AppResponse> {
    let decoded = PcContinuationTokenCodec::parse(token).map_err(|error| {
        AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ContinueTokenInvalid, error.to_string()),
        )
    })?;
    let (query, kind) = match decoded {
        PcContinuationToken::Opening(query) => {
            let report = validate_opening_pc_search_query(&query);
            if report.has_errors() {
                return Err(AppResponse::validation_failed(report));
            }
            return ProblemCompiler::compile_opening_pc(&query)
                .map(|problem| (problem, ContinueKind::Opening))
                .map_err(continue_compile_error);
        }
        PcContinuationToken::Scenario(query) => (query, ContinueKind::ScenarioContinuation),
        PcContinuationToken::ScenarioReplay(query) => (query, ContinueKind::ScenarioReplay),
    };
    let report = validate_pc_scenario_query(&query);
    if report.has_errors() {
        return Err(AppResponse::validation_failed(report));
    }
    let compiled = if matches!(kind, ContinueKind::ScenarioReplay) {
        ProblemCompiler::compile_legacy_scenario_pc(&query)
    } else {
        ProblemCompiler::compile_scenario_pc(&query)
    };
    compiled
        .map(|problem| (problem, kind))
        .map_err(continue_compile_error)
}

fn continue_compile_error(error: impl std::fmt::Debug) -> AppResponse {
    AppResponse::failed(
        AppStatus::ExecutionFailed,
        AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
    )
}

fn continue_response(
    kind: ContinueKind,
    token: &str,
    opening: Option<&PreparedFixedOpeningContinuation>,
    result: CoreExecutionResult,
) -> AppResponse {
    let result = match opening {
        Some(prepared) => prepared.attach(result),
        None => result,
    };
    let (status, continuation_kind) = match kind {
        ContinueKind::Opening => ("continued-searched", "opening"),
        ContinueKind::ScenarioContinuation => ("scenario-continued-searched", "scenario"),
        ContinueKind::ScenarioReplay => ("scenario-replayed-searched", "scenario-replay"),
    };
    AppResponse::success(AppRenderModel::Continue(AppMessage::new(
        AppResultKind::Continue,
        continue_fields(
            status,
            continuation_kind,
            token,
            result.into_summary_fields(),
        ),
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
