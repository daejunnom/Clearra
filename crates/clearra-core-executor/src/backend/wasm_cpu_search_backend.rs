use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_problem::SearchProblem;

use crate::CoreExecutionResult;

#[cfg(feature = "webgpu-search")]
use super::wasm_cpu::WasmWebGpuSearchSession;
use super::wasm_cpu::{ExactSearchAdvance, WasmExactSearchError, WasmExactSearchSession};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WasmCpuSearchError {
    Unsupported { reason: &'static str },
    InvalidProblem { reason: &'static str },
    WorkerPoolUnavailable,
    Cancelled,
}

impl WasmCpuSearchError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Unsupported { reason } | Self::InvalidProblem { reason } => reason,
            Self::WorkerPoolUnavailable => "wasm_cpu_worker_pool_unavailable",
            Self::Cancelled => "wasm_cpu_search_cancelled",
        }
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
        let worker_count = runtime_worker_count(problem.backend_policy().workers());
        #[cfg(not(feature = "parallel"))]
        let gpu_selected = cfg!(feature = "webgpu-search") && should_use_webgpu(problem);
        #[cfg(not(feature = "parallel"))]
        if worker_count > 1 && !gpu_selected && Self::distributed_execution_is_worthwhile(problem) {
            return Err(WasmCpuSearchError::WorkerPoolUnavailable);
        }
        let mut session = WasmCpuSearchSession::new(problem)?;
        #[cfg(feature = "parallel")]
        if worker_count > 1 {
            if let Some(result) = session.execute_parallel_if_worthwhile(worker_count, control)? {
                return Ok(result);
            }
        }
        loop {
            match session.advance(4096, control)? {
                WasmCpuSearchAdvance::Pending => {}
                WasmCpuSearchAdvance::Completed(result) => return Ok(result),
                WasmCpuSearchAdvance::Cancelled => return Err(WasmCpuSearchError::Cancelled),
            }
        }
    }
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
        WasmExactSearchError::Cancelled => WasmCpuSearchError::Cancelled,
    }
}

#[cfg(test)]
mod coverage_summary_tests {
    use clearra_core_domain::{
        execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
        piece::piece_kind::PieceKind,
    };
    use clearra_pc_graph::request::{
        PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PieceWindow, RequestedSearchBackend,
    };
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{WasmCpuSearchBackend, WasmCpuSearchError, WasmCpuSearchSession};

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

    #[test]
    fn percent_coverage_summary_omits_solution_set_and_trace() {
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

    use super::select_webgpu_for_workload;

    #[test]
    fn auto_uses_cpu_for_small_geometry_and_gpu_for_large_geometry() {
        assert!(!select_webgpu_for_workload(RequestedSearchBackend::Auto, 5));
        assert!(select_webgpu_for_workload(RequestedSearchBackend::Auto, 7));
    }

    #[test]
    fn explicit_backend_selection_is_not_overridden_by_workload_size() {
        assert!(!select_webgpu_for_workload(RequestedSearchBackend::Cpu, 20));
        assert!(select_webgpu_for_workload(RequestedSearchBackend::Gpu, 1));
        assert!(select_webgpu_for_workload(
            RequestedSearchBackend::Hybrid,
            1
        ));
    }
}
