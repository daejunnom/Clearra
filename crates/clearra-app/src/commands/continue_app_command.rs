#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
use clearra_core_executor::backend::wasm_cpu_search_backend::NativePcCooperativeSession;
use clearra_core_executor::{
    service::pc_continuation_fields::PreparedFixedOpeningContinuation, CoreExecutionError,
    CoreExecutionResult,
};
use clearra_pc_graph::request::{
    PcContinuationToken, PcContinuationTokenCodec, PcExecutionPolicy, RequestedSearchBackend,
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

#[derive(Clone, Copy)]
pub(crate) enum ContinueExecutionHost {
    Native,
    LocalWasmWorker,
}

impl ContinueExecutionHost {
    fn local_worker_policy(self, policy: &PcExecutionPolicy) -> Option<PcExecutionPolicy> {
        match self {
            Self::Native => None,
            // The token carries scenario semantics, not a worker request.
            // A local WASM receiver owns one CPU worker and never a pool.
            Self::LocalWasmWorker => Some(
                policy
                    .clone()
                    .with_requested_backend(RequestedSearchBackend::Cpu)
                    .with_workers(1),
            ),
        }
    }
}

impl ContinueAppCommand {
    pub fn new(token: Option<String>) -> Self {
        Self { token }
    }

    /// Invalid input and errors during compilation stay on Continue's ordinary
    /// path, including its pre-execution cancellation behavior.
    pub(crate) fn compile_cooperative(
        &self,
        host: ContinueExecutionHost,
        allow_native_multiworker: bool,
    ) -> Option<(SearchProblem, ContinueKind)> {
        let (problem, kind) = compile_continue(self.token.as_deref()?, host).ok()?;
        if problem.backend_request().requested_backend() != RequestedSearchBackend::Cpu {
            return None;
        }
        if problem.backend_policy().workers() == 1 {
            return Some((problem, kind));
        }
        #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
        if allow_native_multiworker
            && !matches!(kind, ContinueKind::Opening)
            && NativePcCooperativeSession::supports_problem(&problem)
        {
            return Some((problem, kind));
        }
        #[cfg(not(all(feature = "parallel", not(target_family = "wasm"))))]
        let _ = allow_native_multiworker;
        None
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
        let (problem, kind) = match compile_continue(&token, ContinueExecutionHost::Native) {
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
                continue_response(kind, token, opening.as_ref(), result)
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
    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    pub(crate) fn checked_native_retained_capacity_bytes(&self) -> Option<u128> {
        // Only scenario tokens enter the native cooperative route. The
        // compact opening projection has a separate boxed memory owner.
        if matches!(self.kind, ContinueKind::Opening) || self.opening.is_some() {
            return None;
        }
        Some(self.token.as_ref()?.capacity() as u128)
    }

    pub(crate) fn response(self, result: CoreExecutionResult) -> AppResponse {
        match self.token {
            Some(token) => continue_response(self.kind, token, self.opening.as_deref(), result),
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
fn compile_continue(
    token: &str,
    host: ContinueExecutionHost,
) -> Result<(SearchProblem, ContinueKind), AppResponse> {
    let decoded = PcContinuationTokenCodec::parse(token).map_err(|error| {
        AppResponse::failed(
            AppStatus::ExecutionFailed,
            AppError::new(AppErrorCode::ContinueTokenInvalid, error.to_string()),
        )
    })?;
    let (mut query, kind) = match decoded {
        PcContinuationToken::Opening(mut query) => {
            if let Some(policy) = host.local_worker_policy(query.execution_policy()) {
                query = query.with_execution_policy(policy);
            }
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
    if let Some(policy) = host.local_worker_policy(query.execution_policy()) {
        query = query.with_execution_policy(policy);
    }
    let report = validate_pc_scenario_query(&query);
    if report.has_errors() {
        return Err(AppResponse::validation_failed(report));
    }
    // An old compact checkpoint can specify a visible frame taller than
    // the exact residual packing target. Reuse the legacy frame proof only
    // for sc2 and an explicit piece count: that compiler can admit a
    // smaller target only on exact area equality and without discarding
    // occupied cells. Keep sc3 and inferred-count checkpoints unchanged.
    let legacy_exact_checkpoint = token.starts_with("sc2:") && query.exact_pieces().is_some();
    let compiled = if matches!(kind, ContinueKind::ScenarioReplay) || legacy_exact_checkpoint {
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
    token: String,
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
    token: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_scenario_checkpoint_uses_exact_legacy_packing_frame() {
        let prefix = "sc2:w10:v2:m0x";
        let suffix = ":psstandard-tetrominoes:bgstandard-7-bag:rsrs-plus:hnone:qI:p1:x1:n0:a1:z0:gclear-to-empty:ccount-all:t1";
        // Both fields have six occupied cells and request one I. Only the
        // lower field fits in the proven one-line packing target.
        for (mask, geometry_height) in [(0x3f0, 1), (0xfc000, 2)] {
            let token = format!("{prefix}{mask:016x}{suffix}");
            for (host, requested_workers) in [
                (ContinueExecutionHost::Native, None),
                (ContinueExecutionHost::LocalWasmWorker, Some(1)),
            ] {
                let (problem, kind) =
                    compile_continue(&token, host).expect("parse compact checkpoint");
                assert!(matches!(kind, ContinueKind::ScenarioContinuation));
                assert_eq!(problem.initial_board().occupied_mask(), mask);
                assert_eq!(problem.visible_height(), 2);
                assert_eq!(problem.exact_pieces(), Some(1));
                assert_eq!(problem.compact_pc_geometry_height(), geometry_height);
                assert_eq!(
                    problem.backend_request().workers_requested(),
                    requested_workers
                );
            }
        }
    }
}
