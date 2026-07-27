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
        if webgpu_requested_but_unavailable(problem) {
            if !problem.backend_policy().allow_backend_fallback() {
                return Err(WasmCpuSearchError::Unsupported {
                    reason: "webgpu_backend_unavailable",
                });
            }
            let mut cpu = WasmExactSearchSession::new(problem).map_err(map_error)?;
            cpu.mark_cpu_fallback(
                "gpu_kernel_unavailable",
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

fn webgpu_requested_but_unavailable(problem: &SearchProblem) -> bool {
    use clearra_pc_graph::request::RequestedSearchBackend;

    !problem.backend_policy().runtime_webgpu_available()
        && matches!(
            problem.backend_policy().requested_backend(),
            RequestedSearchBackend::Gpu | RequestedSearchBackend::Hybrid
        )
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
