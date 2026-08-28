// SRP rationale: this module has one change reason: the WASM CPU backend's exact search execution contract.
use std::sync::Arc;

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    objective::objective_kind::ObjectiveKind,
    resource::{ExecutionAvailabilityReason, ExecutionAvailabilityState, ResourceReport},
};
use clearra_problem::{SearchOutputPolicy, SearchProblem};

use crate::{resource::WasmCpuTerminalResourceAuthority, CoreExecutionError, CoreExecutionResult};

#[cfg(feature = "webgpu-search")]
use super::wasm_cpu::WasmWebGpuSearchSession;
use super::wasm_cpu::{ExactSearchAdvance, WasmExactSearchError, WasmExactSearchSession};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WasmCpuSearchError {
    Unsupported { reason: &'static str },
    InvalidProblem { reason: &'static str },
    ResourceAdmission { resource_report: ResourceReport },
    WorkerPoolUnavailable,
    Cancelled,
}

impl WasmCpuSearchError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Unsupported { reason } | Self::InvalidProblem { reason } => reason,
            Self::ResourceAdmission { resource_report } => {
                resource_admission_reason(resource_report)
            }
            Self::WorkerPoolUnavailable => "wasm_cpu_worker_pool_unavailable",
            Self::Cancelled => "wasm_cpu_search_cancelled",
        }
    }

    pub fn into_core_execution_error(self) -> CoreExecutionError {
        match self {
            Self::ResourceAdmission { resource_report } => {
                CoreExecutionError::resource_incomplete("execution-admission", 0, resource_report)
            }
            Self::Unsupported { reason } => {
                CoreExecutionError::RuntimeUnavailable { component: reason }
            }
            Self::WorkerPoolUnavailable => CoreExecutionError::RuntimeUnavailable {
                component: "wasm_cpu_worker_pool_unavailable",
            },
            Self::InvalidProblem { reason } => CoreExecutionError::Pc(reason.to_owned()),
            Self::Cancelled => CoreExecutionError::Cancelled,
        }
    }
}

const fn resource_admission_reason(report: ResourceReport) -> &'static str {
    match (
        report.execution_availability().state(),
        report.execution_availability().reason(),
    ) {
        (
            ExecutionAvailabilityState::Unavailable,
            Some(ExecutionAvailabilityReason::PatternCountAddressSpaceExceeded),
        ) => "pattern_count_address_space_unavailable",
        (
            ExecutionAvailabilityState::Unavailable,
            Some(ExecutionAvailabilityReason::DensePatternRepresentationUnavailable),
        ) => "dense_pattern_representation_unavailable",
        (
            ExecutionAvailabilityState::Deferred,
            Some(ExecutionAvailabilityReason::SharedResourceContention),
        ) => "shared_execution_resource_deferred",
        (
            ExecutionAvailabilityState::Exhausted,
            Some(ExecutionAvailabilityReason::MemoryBudgetExceeded),
        ) => "shared_execution_memory_exhausted",
        (
            ExecutionAvailabilityState::Exhausted,
            Some(ExecutionAvailabilityReason::ComputeBudgetExceeded),
        ) => "shared_execution_compute_exhausted",
        _ => "shared_execution_resource_unavailable",
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct WasmCpuSearchBackend;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WasmProductSearchBackend {
    Cpu,
    WebGpu,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WasmCpuSearchAdvance {
    Pending,
    Completed(CoreExecutionResult),
    Cancelled,
}

pub struct WasmCpuSearchSession {
    inner: WasmSearchSessionInner,
}

/// Read-only terminal validator borrowed from a completed direct WASM search.
///
/// The borrow keeps the underlying search session, including its shared
/// execution-resource lease, alive for the whole terminal callback.  No
/// search mutation is exposed through this authority; callers may only prove
/// that the still-live session, the returned Core result, and a checked future
/// post-processing allocation fit under the admitted memory cap. Validation
/// succeeds only for a session created under
/// [`WasmCpuTerminalResourceAuthority`]; compatibility sessions fail closed.
pub struct WasmCpuSearchTerminalAuthority<'a> {
    session: &'a WasmCpuSearchSession,
}

impl WasmCpuSearchTerminalAuthority<'_> {
    pub fn validate_public_result_memory(
        &self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmCpuSearchError> {
        self.session.validate_public_result_memory(result)
    }

    pub fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        checked_future_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        self.session
            .validate_public_result_memory_with_future(result, checked_future_bytes)
    }
}

enum WasmSearchSessionInner {
    Cpu(WasmExactSearchSession),
    #[cfg(feature = "webgpu-search")]
    WebGpu(WasmWebGpuSearchSession),
}

impl WasmCpuSearchSession {
    pub fn new(problem: &SearchProblem) -> Result<Self, WasmCpuSearchError> {
        if let Some(unavailable_reason) = explicit_gpu_unavailable_reason(problem) {
            if !problem.backend_policy().allow_backend_fallback() {
                return Err(WasmCpuSearchError::Unsupported {
                    reason: "webgpu_backend_unavailable",
                });
            }
            let mut cpu = WasmExactSearchSession::new(problem).map_err(map_error)?;
            cpu.mark_cpu_fallback(
                unavailable_reason,
                "unavailable",
                "capability-query",
                false,
                false,
            );
            return Ok(Self {
                inner: WasmSearchSessionInner::Cpu(cpu),
            });
        }
        #[cfg(feature = "webgpu-search")]
        if should_use_webgpu(problem) {
            return Ok(Self {
                inner: WasmSearchSessionInner::WebGpu(
                    WasmWebGpuSearchSession::new(problem).map_err(map_error)?,
                ),
            });
        }
        Ok(Self {
            inner: WasmSearchSessionInner::Cpu(
                WasmExactSearchSession::new(problem).map_err(map_error)?,
            ),
        })
    }

    /// Compatibility shared-input constructor that avoids a SearchProblem
    /// clone but does not establish terminal public-memory authority. Use
    /// `new_shared_under_authority` for an authoritative typed score session.
    pub fn new_shared(problem: Arc<SearchProblem>) -> Result<Self, WasmCpuSearchError> {
        validate_shared_terminal_problem(problem.as_ref(), false)?;
        Ok(Self {
            inner: WasmSearchSessionInner::Cpu(
                WasmExactSearchSession::new_shared(problem).map_err(map_error)?,
            ),
        })
    }

    /// Starts a typed score session as a compute-only child of the full
    /// request-level authority acquired before query compilation. The external
    /// bound must conservatively cover every caller-owned value retained with
    /// the session, including the shared SearchProblem pointee.
    pub fn new_shared_under_authority(
        problem: Arc<SearchProblem>,
        checked_external_retained_upper_bound_bytes: u128,
        authority: &WasmCpuTerminalResourceAuthority,
    ) -> Result<Self, WasmCpuSearchError> {
        validate_shared_terminal_problem(problem.as_ref(), true)?;
        Ok(Self {
            inner: WasmSearchSessionInner::Cpu(
                WasmExactSearchSession::new_shared_under_authority(
                    problem,
                    checked_external_retained_upper_bound_bytes,
                    authority,
                )
                .map_err(map_error)?,
            ),
        })
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<WasmCpuSearchAdvance, WasmCpuSearchError> {
        let advance = match &mut self.inner {
            WasmSearchSessionInner::Cpu(session) => session.advance(work_budget, control),
            #[cfg(feature = "webgpu-search")]
            WasmSearchSessionInner::WebGpu(session) => session.advance(work_budget, control),
        }
        .map_err(map_error)?;
        match advance {
            ExactSearchAdvance::Pending => Ok(WasmCpuSearchAdvance::Pending),
            ExactSearchAdvance::Completed(result) => Ok(WasmCpuSearchAdvance::Completed(result)),
            ExactSearchAdvance::Cancelled => Ok(WasmCpuSearchAdvance::Cancelled),
        }
    }

    /// Validates a completed public result while this session still owns its
    /// child execution admission. Cooperative callers retain both this session
    /// and its request-level [`WasmCpuTerminalResourceAuthority`] through
    /// post-processing. Compatibility sessions without that parent and an
    /// external retained bound fail closed.
    pub fn validate_public_result_memory(
        &self,
        result: &CoreExecutionResult,
    ) -> Result<(), WasmCpuSearchError> {
        self.validate_public_result_memory_with_future(result, 0)
    }

    pub fn validate_public_result_memory_with_future(
        &self,
        result: &CoreExecutionResult,
        checked_future_bytes: u128,
    ) -> Result<(), WasmCpuSearchError> {
        match &self.inner {
            WasmSearchSessionInner::Cpu(session) => {
                session.validate_public_result_memory_with_future(result, checked_future_bytes)
            }
            #[cfg(feature = "webgpu-search")]
            WasmSearchSessionInner::WebGpu(session) => {
                session.validate_public_result_memory_with_future(result, checked_future_bytes)
            }
        }
        .map_err(map_error)
    }

    #[cfg(test)]
    fn admitted_memory_cap_bytes(&self) -> u128 {
        match &self.inner {
            WasmSearchSessionInner::Cpu(session) => session.admitted_memory_cap_bytes(),
            #[cfg(feature = "webgpu-search")]
            WasmSearchSessionInner::WebGpu(_) => {
                unreachable!("test memory-cap probe requires the explicitly selected CPU backend")
            }
        }
    }

    #[cfg(test)]
    fn shares_problem_arc(&self, problem: &Arc<SearchProblem>) -> bool {
        match &self.inner {
            WasmSearchSessionInner::Cpu(session) => session.shares_problem_arc(problem),
            #[cfg(feature = "webgpu-search")]
            WasmSearchSessionInner::WebGpu(_) => false,
        }
    }

    #[cfg(test)]
    fn checked_terminal_retained_bytes(&self, result: &CoreExecutionResult) -> Option<u128> {
        match &self.inner {
            WasmSearchSessionInner::Cpu(session) => session.checked_terminal_retained_bytes(result),
            #[cfg(feature = "webgpu-search")]
            WasmSearchSessionInner::WebGpu(_) => None,
        }
    }

    #[cfg(feature = "parallel")]
    fn execute_parallel_if_worthwhile(
        &mut self,
        worker_count: usize,
        control: &ExecutionControl,
    ) -> Result<Option<CoreExecutionResult>, WasmCpuSearchError> {
        match &mut self.inner {
            WasmSearchSessionInner::Cpu(session) => session
                .execute_parallel_if_worthwhile(worker_count, control)
                .map_err(map_error),
            #[cfg(feature = "webgpu-search")]
            WasmSearchSessionInner::WebGpu(_) => Ok(None),
        }
    }
}

impl WasmCpuSearchBackend {
    pub fn selected_product_backend(problem: &SearchProblem) -> WasmProductSearchBackend {
        if cfg!(feature = "webgpu-search") && should_use_webgpu(problem) {
            WasmProductSearchBackend::WebGpu
        } else {
            WasmProductSearchBackend::Cpu
        }
    }

    pub fn distributed_execution_is_worthwhile(problem: &SearchProblem) -> bool {
        !problem
            .queue_observation_policy()
            .requires_observation_policy()
            && required_piece_count(problem) >= 7
    }

    pub fn execute_with_control(
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, WasmCpuSearchError> {
        Self::execute_with_control_and_terminal(problem, control, |result, _| result)
    }

    pub fn execute_with_control_and_terminal<R>(
        problem: &SearchProblem,
        control: &ExecutionControl,
        terminal: impl FnOnce(
            Result<CoreExecutionResult, WasmCpuSearchError>,
            Option<WasmCpuSearchTerminalAuthority<'_>>,
        ) -> R,
    ) -> R {
        let worker_count = runtime_worker_count(problem.backend_policy().workers());
        #[cfg(not(feature = "parallel"))]
        let gpu_selected = cfg!(feature = "webgpu-search") && should_use_webgpu(problem);
        #[cfg(not(feature = "parallel"))]
        if worker_count > 1 && !gpu_selected && Self::distributed_execution_is_worthwhile(problem) {
            return terminal(Err(WasmCpuSearchError::WorkerPoolUnavailable), None);
        }
        let mut session = match WasmCpuSearchSession::new(problem) {
            Ok(session) => session,
            Err(error) => return terminal(Err(error), None),
        };
        #[cfg(feature = "parallel")]
        if worker_count > 1 {
            match session.execute_parallel_if_worthwhile(worker_count, control) {
                Ok(Some(result)) => {
                    return terminal(
                        Ok(result),
                        Some(WasmCpuSearchTerminalAuthority { session: &session }),
                    )
                }
                Ok(None) => {}
                Err(error) => {
                    return terminal(
                        Err(error),
                        Some(WasmCpuSearchTerminalAuthority { session: &session }),
                    )
                }
            }
        }
        let result = loop {
            match session.advance(4096, control) {
                Err(error) => break Err(error),
                Ok(WasmCpuSearchAdvance::Pending) => {}
                Ok(WasmCpuSearchAdvance::Completed(result)) => break Ok(result),
                Ok(WasmCpuSearchAdvance::Cancelled) => break Err(WasmCpuSearchError::Cancelled),
            }
        };
        terminal(
            result,
            Some(WasmCpuSearchTerminalAuthority { session: &session }),
        )
    }

    /// Compatibility direct shared-input execution. Its terminal validator
    /// fails closed because no pre-compilation parent lease or conservative
    /// external retained bound was supplied.
    pub fn execute_shared_with_control_and_terminal<R>(
        problem: Arc<SearchProblem>,
        control: &ExecutionControl,
        terminal: impl FnOnce(
            Result<CoreExecutionResult, WasmCpuSearchError>,
            Option<WasmCpuSearchTerminalAuthority<'_>>,
        ) -> R,
    ) -> R {
        let mut session = match WasmCpuSearchSession::new_shared(problem) {
            Ok(session) => session,
            Err(error) => return terminal(Err(error), None),
        };
        let result = loop {
            match session.advance(4096, control) {
                Err(error) => break Err(error),
                Ok(WasmCpuSearchAdvance::Pending) => {}
                Ok(WasmCpuSearchAdvance::Completed(result)) => break Ok(result),
                Ok(WasmCpuSearchAdvance::Cancelled) => break Err(WasmCpuSearchError::Cancelled),
            }
        };
        terminal(
            result,
            Some(WasmCpuSearchTerminalAuthority { session: &session }),
        )
    }

    /// Direct typed score execution under a parent acquired before request
    /// compilation. The borrow guarantees that the public terminal callback
    /// runs before the caller can release that parent owner.
    pub fn execute_shared_under_authority_with_control_and_terminal<R>(
        problem: Arc<SearchProblem>,
        checked_external_retained_upper_bound_bytes: u128,
        authority: &WasmCpuTerminalResourceAuthority,
        control: &ExecutionControl,
        terminal: impl FnOnce(
            Result<CoreExecutionResult, WasmCpuSearchError>,
            Option<WasmCpuSearchTerminalAuthority<'_>>,
        ) -> R,
    ) -> R {
        let mut session = match WasmCpuSearchSession::new_shared_under_authority(
            problem,
            checked_external_retained_upper_bound_bytes,
            authority,
        ) {
            Ok(session) => session,
            Err(error) => return terminal(Err(error), None),
        };
        let result = loop {
            match session.advance(4096, control) {
                Err(error) => break Err(error),
                Ok(WasmCpuSearchAdvance::Pending) => {}
                Ok(WasmCpuSearchAdvance::Completed(result)) => break Ok(result),
                Ok(WasmCpuSearchAdvance::Cancelled) => break Err(WasmCpuSearchError::Cancelled),
            }
        };
        terminal(
            result,
            Some(WasmCpuSearchTerminalAuthority { session: &session }),
        )
    }
}

fn validate_shared_terminal_problem(
    problem: &SearchProblem,
    allow_typed_tiling: bool,
) -> Result<(), WasmCpuSearchError> {
    use clearra_pc_graph::request::{RequestedSearchBackend, WorkerPolicy};

    let typed_tiling = allow_typed_tiling
        && problem.output_policy() == SearchOutputPolicy::TilingOnly
        && problem.objective().kind() == ObjectiveKind::Tiling;
    if !problem.objective().score().requested() && !typed_tiling {
        return Err(WasmCpuSearchError::InvalidProblem {
            reason: "shared_terminal_memory_authority_requires_score",
        });
    }
    match problem.backend_policy().requested_backend() {
        RequestedSearchBackend::Cpu => {}
        RequestedSearchBackend::Auto if !should_use_webgpu(problem) => {}
        RequestedSearchBackend::Auto
        | RequestedSearchBackend::Gpu
        | RequestedSearchBackend::Hybrid => {
            return Err(WasmCpuSearchError::Unsupported {
                reason: "shared_terminal_memory_authority_requires_cpu_backend",
            });
        }
    }
    if matches!(
        problem.backend_policy().worker_policy(),
        WorkerPolicy::Fixed(workers) if workers > 1
    ) {
        return Err(WasmCpuSearchError::Unsupported {
            reason: "shared_terminal_memory_authority_requires_single_worker",
        });
    }
    Ok(())
}

fn should_use_webgpu(problem: &SearchProblem) -> bool {
    if !problem.backend_policy().runtime_webgpu_available() {
        return false;
    }
    let requested = problem.backend_policy().requested_backend();
    select_webgpu_for_workload(requested, required_piece_count(problem))
}

fn required_piece_count(problem: &SearchProblem) -> usize {
    let width = usize::from(problem.initial_board().width());
    let height = usize::from(problem.visible_height());
    let board_cells = width.saturating_mul(height);
    let required_cells =
        board_cells.saturating_sub(problem.initial_board().occupied_mask().count_ones() as usize);
    required_cells / 4
}

fn explicit_gpu_unavailable_reason(problem: &SearchProblem) -> Option<&'static str> {
    use clearra_pc_graph::request::RequestedSearchBackend;

    if problem.backend_policy().requested_backend() != RequestedSearchBackend::Gpu {
        return None;
    }
    if !problem.backend_policy().runtime_webgpu_available() {
        return Some("gpu_device_not_found");
    }
    if !cfg!(feature = "webgpu-search") {
        return Some("gpu_kernel_unavailable");
    }
    None
}

const fn select_webgpu_for_workload(
    requested: clearra_pc_graph::request::RequestedSearchBackend,
    required_piece_count: usize,
) -> bool {
    use clearra_pc_graph::request::RequestedSearchBackend;

    match requested {
        RequestedSearchBackend::Cpu => false,
        RequestedSearchBackend::Gpu | RequestedSearchBackend::Hybrid => true,
        RequestedSearchBackend::Auto => required_piece_count >= 7,
    }
}

fn runtime_worker_count(requested: usize) -> usize {
    if cfg!(target_family = "wasm") {
        1
    } else {
        requested.max(1)
    }
}

fn map_error(error: WasmExactSearchError) -> WasmCpuSearchError {
    match error {
        WasmExactSearchError::InvalidProblem(reason) => {
            WasmCpuSearchError::InvalidProblem { reason }
        }
        WasmExactSearchError::ResourceAdmission(resource_report) => {
            WasmCpuSearchError::ResourceAdmission { resource_report }
        }
        WasmExactSearchError::Cancelled => WasmCpuSearchError::Cancelled,
    }
}

#[cfg(test)]
mod coverage_summary_tests {
    use std::sync::Arc;

    use clearra_core_domain::{
        execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
        piece::piece_kind::PieceKind,
    };
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PieceWindow, RequestedSearchBackend,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{
        score_resource_test_guard, WasmCpuSearchBackend, WasmCpuSearchError, WasmCpuSearchSession,
        WasmCpuTerminalResourceAuthority,
    };

    fn one_piece_problem(
        backend: RequestedSearchBackend,
        allow_fallback: bool,
        runtime_webgpu_available: bool,
    ) -> clearra_problem::SearchProblem {
        let execution_policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(backend)
            .with_allow_backend_fallback(allow_fallback)
            .with_runtime_webgpu_available(runtime_webgpu_available)
            .with_workers(1);
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_retained_trace_limit(0)
        .with_execution_policy(execution_policy);
        ProblemCompiler::compile_scenario_percent(&query).expect("problem")
    }

    fn one_piece_exact_problem_with_policy(
        score_requested: bool,
        backend: RequestedSearchBackend,
        workers: Option<usize>,
        runtime_webgpu_available: bool,
    ) -> clearra_problem::SearchProblem {
        let execution_policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(backend)
            .with_allow_backend_fallback(false)
            .with_runtime_webgpu_available(runtime_webgpu_available)
            .with_max_memory_mib(Some(64));
        let execution_policy = match workers {
            Some(workers) => execution_policy.with_workers(workers),
            None => execution_policy,
        };
        let objective = if score_requested {
            ObjectivePolicy::all().with_score_summary()
        } else {
            ObjectivePolicy::all()
        };
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(1, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_retained_trace_limit(0)
        .with_objective(objective)
        .with_execution_policy(execution_policy);
        ProblemCompiler::compile_scenario_pc(&query).expect("one-piece exact problem")
    }

    fn one_piece_exact_problem(score_requested: bool) -> clearra_problem::SearchProblem {
        one_piece_exact_problem_with_policy(
            score_requested,
            RequestedSearchBackend::Cpu,
            Some(1),
            false,
        )
    }

    fn ten_piece_auto_gpu_problem() -> clearra_problem::SearchProblem {
        let execution_policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Auto)
            .with_allow_backend_fallback(false)
            .with_runtime_webgpu_available(true)
            .with_workers(1)
            .with_max_memory_mib(Some(64));
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 10])),
            PieceWindow::new(10),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(10))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_retained_trace_limit(0)
        .with_objective(ObjectivePolicy::all().with_score_summary())
        .with_execution_policy(execution_policy);
        ProblemCompiler::compile_scenario_pc(&query).expect("ten-piece auto GPU problem")
    }

    fn one_piece_default_auto_score_problem() -> clearra_problem::SearchProblem {
        let execution_policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Auto)
            .with_allow_backend_fallback(false)
            .with_runtime_webgpu_available(false);
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(1, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_retained_trace_limit(0)
        .with_objective(ObjectivePolicy::all().with_score_summary())
        .with_execution_policy(execution_policy);
        ProblemCompiler::compile_scenario_pc(&query).expect("default Auto score problem")
    }

    #[test]
    fn score_exact_search_uses_full_configured_cap_while_generic_keeps_projection() {
        let _resource_guard = score_resource_test_guard();
        let generic_cap = {
            let session = WasmCpuSearchSession::new(&one_piece_exact_problem(false))
                .expect("generic exact session");
            session.admitted_memory_cap_bytes()
        };
        let score_cap = {
            let session = WasmCpuSearchSession::new(&one_piece_exact_problem(true))
                .expect("score exact session");
            session.admitted_memory_cap_bytes()
        };

        assert_eq!(score_cap, 64_u128 * 1024 * 1024);
        assert!(generic_cap < score_cap);
    }

    #[test]
    fn successful_direct_score_terminal_has_live_read_only_memory_authority() {
        let _resource_guard = score_resource_test_guard();
        let resource_authority = WasmCpuTerminalResourceAuthority::try_acquire_full_capacity()
            .expect("request-level terminal resource authority");
        let problem = Arc::new(one_piece_exact_problem(true));
        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let checked_external_retained_upper_bound_bytes = 4_096;

        let result = WasmCpuSearchBackend::execute_shared_under_authority_with_control_and_terminal(
            problem,
            checked_external_retained_upper_bound_bytes,
            &resource_authority,
            &control,
            |result, authority| {
                let result = result.expect("score search completes");
                let authority = authority.expect("successful terminal callback has authority");
                authority
                    .validate_public_result_memory(&result)
                    .expect("live session and public result fit the score lease");
                assert!(matches!(
                    authority.validate_public_result_memory_with_future(&result, u128::MAX),
                    Err(WasmCpuSearchError::ResourceAdmission { .. })
                ));
                result
            },
        );

        assert!(result.solution_found());
    }

    #[test]
    fn completed_cooperative_score_session_retains_the_same_memory_authority() {
        let _resource_guard = score_resource_test_guard();
        let resource_authority = WasmCpuTerminalResourceAuthority::try_acquire_full_capacity()
            .expect("request-level terminal resource authority");
        let problem = Arc::new(one_piece_exact_problem(true));
        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let checked_external_retained_upper_bound_bytes = 4_096;
        let mut session = WasmCpuSearchSession::new_shared_under_authority(
            Arc::clone(&problem),
            checked_external_retained_upper_bound_bytes,
            &resource_authority,
        )
        .expect("parent-authorized shared score session");
        assert!(session.shares_problem_arc(&problem));
        let result = loop {
            match session.advance(64, &control).expect("cooperative advance") {
                super::WasmCpuSearchAdvance::Pending => {}
                super::WasmCpuSearchAdvance::Completed(result) => break result,
                super::WasmCpuSearchAdvance::Cancelled => panic!("score search cancelled"),
            }
        };

        session
            .validate_public_result_memory(&result)
            .expect("completed session retains admission lease");
        let retained = session
            .checked_terminal_retained_bytes(&result)
            .expect("shared session has exact terminal accounting");
        assert!(retained >= checked_external_retained_upper_bound_bytes);
        let cap = session.admitted_memory_cap_bytes();
        let exact_cap_future = cap
            .checked_sub(retained)
            .expect("completed score result fits admitted cap");
        session
            .validate_public_result_memory_with_future(&result, exact_cap_future)
            .expect("the exact admitted cap boundary is accepted");
        assert!(matches!(
            session.validate_public_result_memory_with_future(
                &result,
                exact_cap_future
                    .checked_add(1)
                    .expect("finite cap boundary")
            ),
            Err(WasmCpuSearchError::ResourceAdmission { .. })
        ));
        assert!(matches!(
            session.validate_public_result_memory_with_future(&result, u128::MAX),
            Err(WasmCpuSearchError::ResourceAdmission { .. })
        ));

        drop(session);
        assert!(matches!(
            WasmCpuSearchSession::new_shared_under_authority(
                Arc::clone(&problem),
                cap.checked_add(1).expect("finite configured cap"),
                &resource_authority,
            ),
            Err(WasmCpuSearchError::ResourceAdmission { .. })
        ));
    }

    #[test]
    fn full_capacity_parent_is_exclusive_and_releases_after_drop() {
        let _resource_guard = score_resource_test_guard();
        let authority = WasmCpuTerminalResourceAuthority::try_acquire_full_capacity()
            .expect("first full-capacity authority");
        let report = match WasmCpuTerminalResourceAuthority::try_acquire_full_capacity() {
            Ok(_) => panic!("a second full-capacity authority must not overlap"),
            Err(report) => report,
        };
        assert_ne!(
            report.execution_availability().state(),
            clearra_core_domain::resource::ExecutionAvailabilityState::Available
        );
        drop(authority);
        let reacquired = WasmCpuTerminalResourceAuthority::try_acquire_full_capacity()
            .expect("dropping the parent releases full capacity");
        let problem = Arc::new(one_piece_default_auto_score_problem());
        let session = WasmCpuSearchSession::new_shared_under_authority(problem, 4_096, &reacquired)
            .expect("default Auto score is normalized to the serial CPU child");
        assert_eq!(
            session.admitted_memory_cap_bytes(),
            reacquired.memory_capacity_bytes(),
            "an unbounded request inherits the physical host surface without an arbitrary cap"
        );
        drop(session);
        drop(reacquired);
    }

    #[test]
    fn clone_owned_compatibility_score_executes_but_terminal_validation_fails_closed() {
        let _resource_guard = score_resource_test_guard();
        let problem = one_piece_exact_problem(true);
        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let mut session = WasmCpuSearchSession::new(&problem).expect("compatibility score session");
        let result = loop {
            match session
                .advance(64, &control)
                .expect("compatibility advance")
            {
                super::WasmCpuSearchAdvance::Pending => {}
                super::WasmCpuSearchAdvance::Completed(result) => break result,
                super::WasmCpuSearchAdvance::Cancelled => panic!("score search cancelled"),
            }
        };

        assert!(result.solution_found());
        assert!(matches!(
            session.validate_public_result_memory(&result),
            Err(WasmCpuSearchError::ResourceAdmission { .. })
        ));

        drop(session);
        let problem = Arc::new(problem);
        let mut unparented = WasmCpuSearchSession::new_shared(problem)
            .expect("unparented shared compatibility session");
        let result = loop {
            match unparented
                .advance(64, &control)
                .expect("unparented shared advance")
            {
                super::WasmCpuSearchAdvance::Pending => {}
                super::WasmCpuSearchAdvance::Completed(result) => break result,
                super::WasmCpuSearchAdvance::Cancelled => panic!("score search cancelled"),
            }
        };
        assert!(matches!(
            unparented.validate_public_result_memory(&result),
            Err(WasmCpuSearchError::ResourceAdmission { .. })
        ));
    }

    #[test]
    fn shared_score_accepts_auto_only_when_it_resolves_to_serial_cpu() {
        let _resource_guard = score_resource_test_guard();
        let generic = Arc::new(one_piece_exact_problem(false));
        assert!(matches!(
            WasmCpuSearchSession::new_shared(generic),
            Err(WasmCpuSearchError::InvalidProblem {
                reason: "shared_terminal_memory_authority_requires_score"
            })
        ));

        let auto_cpu = Arc::new(one_piece_exact_problem_with_policy(
            true,
            RequestedSearchBackend::Auto,
            None,
            true,
        ));
        let session = WasmCpuSearchSession::new_shared(Arc::clone(&auto_cpu))
            .expect("small Auto workload resolves deterministically to CPU");
        assert!(session.shares_problem_arc(&auto_cpu));
        drop(session);

        let auto_gpu = Arc::new(ten_piece_auto_gpu_problem());
        assert!(matches!(
            WasmCpuSearchSession::new_shared(auto_gpu),
            Err(WasmCpuSearchError::Unsupported {
                reason: "shared_terminal_memory_authority_requires_cpu_backend"
            })
        ));

        let parallel_cpu = Arc::new(one_piece_exact_problem_with_policy(
            true,
            RequestedSearchBackend::Cpu,
            Some(2),
            false,
        ));
        assert!(matches!(
            WasmCpuSearchSession::new_shared(parallel_cpu),
            Err(WasmCpuSearchError::Unsupported {
                reason: "shared_terminal_memory_authority_requires_single_worker"
            })
        ));
    }

    #[test]
    fn percent_coverage_summary_omits_solution_set_and_trace() {
        let _resource_guard = score_resource_test_guard();
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xf3fcf),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_retained_trace_limit(0);
        let problem = ProblemCompiler::compile_scenario_percent(&query).expect("problem");
        let control = ExecutionControl::new(ExecutionCancellationToken::new());

        let result = WasmCpuSearchBackend::execute_with_control(&problem, &control)
            .expect("coverage summary");

        assert!(result.solution_found());
        assert_eq!(
            result.field("search_output_policy"),
            Some("coverage-summary")
        );
        assert_eq!(result.field("coverage_probability"), Some("1"));
        assert_eq!(result.field("covered_pattern_count"), Some("1"));
        assert_eq!(result.field("solution_count_calculated"), Some("false"));
        assert_eq!(result.field("solution_set_materialized"), Some("false"));
        assert_eq!(
            result.field("unique_solution_count"),
            Some("not-calculated")
        );
        assert_eq!(
            result.field("normalized_solution_set_hash"),
            Some("not-calculated")
        );
        assert!(result.normalized_solution_identities().is_empty());
        assert!(result.path_steps().is_empty());
    }

    #[test]
    fn unavailable_explicit_gpu_uses_an_explicit_cpu_fallback() {
        let _resource_guard = score_resource_test_guard();
        let problem = one_piece_problem(RequestedSearchBackend::Gpu, true, false);
        let control = ExecutionControl::new(ExecutionCancellationToken::new());

        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &control).expect("GPU fallback");

        assert_eq!(result.field("backend_selected"), Some("wasm-cpu"));
        assert_eq!(result.field("backend_fallback_used"), Some("true"));
        assert_eq!(result.field("gpu_available"), Some("false"));
        assert_eq!(
            result.field("gpu_disabled_reason"),
            Some("gpu_device_not_found")
        );
        assert_eq!(
            result.field("backend_fallback_reason"),
            Some("gpu_device_not_found")
        );
        assert_eq!(result.field("fallback_backend"), Some("wasm-cpu"));
    }

    #[test]
    fn unavailable_explicit_gpu_rejects_a_denied_fallback() {
        let _resource_guard = score_resource_test_guard();
        let problem = one_piece_problem(RequestedSearchBackend::Gpu, false, false);

        assert!(matches!(
            WasmCpuSearchSession::new(&problem),
            Err(WasmCpuSearchError::Unsupported {
                reason: "webgpu_backend_unavailable"
            })
        ));
    }

    #[test]
    fn unavailable_hybrid_selects_cpu_without_using_fallback() {
        let _resource_guard = score_resource_test_guard();
        for allow_fallback in [false, true] {
            let problem = one_piece_problem(RequestedSearchBackend::Hybrid, allow_fallback, false);
            let control = ExecutionControl::new(ExecutionCancellationToken::new());

            let result = WasmCpuSearchBackend::execute_with_control(&problem, &control)
                .expect("hybrid CPU selection");

            assert_eq!(result.field("backend_selected"), Some("wasm-cpu"));
            assert_eq!(result.field("backend_fallback_used"), Some("false"));
            assert_eq!(result.field("backend_fallback_reason"), Some("none"));
            assert_eq!(result.field("fallback_backend"), Some("none"));
            assert_eq!(result.field("hybrid_status"), Some("cpu-selected"));
            assert_eq!(
                result.field("hybrid_disabled_reason"),
                Some("gpu_device_not_found")
            );
        }
    }

    #[cfg(not(feature = "webgpu-search"))]
    #[test]
    fn explicit_gpu_without_a_compiled_kernel_uses_the_kernel_unavailable_contract() {
        let _resource_guard = score_resource_test_guard();
        let problem = one_piece_problem(RequestedSearchBackend::Gpu, true, true);
        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let result =
            WasmCpuSearchBackend::execute_with_control(&problem, &control).expect("GPU fallback");

        assert_eq!(result.field("backend_selected"), Some("wasm-cpu"));
        assert_eq!(result.field("backend_fallback_used"), Some("true"));
        assert_eq!(result.field("gpu_available"), Some("false"));
        assert_eq!(
            result.field("gpu_disabled_reason"),
            Some("gpu_kernel_unavailable")
        );
        assert_eq!(
            result.field("backend_fallback_reason"),
            Some("gpu_kernel_unavailable")
        );

        let denied = one_piece_problem(RequestedSearchBackend::Gpu, false, true);
        assert!(matches!(
            WasmCpuSearchSession::new(&denied),
            Err(WasmCpuSearchError::Unsupported {
                reason: "webgpu_backend_unavailable"
            })
        ));
    }
}

#[cfg(all(test, feature = "webgpu-search"))]
mod tests {
    use clearra_pc_graph::request::RequestedSearchBackend;

    use super::{score_resource_test_guard, select_webgpu_for_workload};

    #[test]
    fn auto_uses_cpu_for_small_geometry_and_gpu_for_large_geometry() {
        let _resource_guard = score_resource_test_guard();
        assert!(!select_webgpu_for_workload(RequestedSearchBackend::Auto, 5));
        assert!(select_webgpu_for_workload(RequestedSearchBackend::Auto, 7));
    }

    #[test]
    fn explicit_backend_selection_is_not_overridden_by_workload_size() {
        let _resource_guard = score_resource_test_guard();
        assert!(!select_webgpu_for_workload(RequestedSearchBackend::Cpu, 20));
        assert!(select_webgpu_for_workload(RequestedSearchBackend::Gpu, 1));
        assert!(select_webgpu_for_workload(
            RequestedSearchBackend::Hybrid,
            1
        ));
    }
}
#[cfg(test)]
static SCORE_RESOURCE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
fn score_resource_test_guard() -> std::sync::MutexGuard<'static, ()> {
    SCORE_RESOURCE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
