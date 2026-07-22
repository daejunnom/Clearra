pub(crate) mod candidate_pattern_index;
pub mod hybrid_scheduler_report;
pub mod packing_error;
pub mod packing_memory_report;
pub mod packing_metrics;
#[cfg(all(test, feature = "webgpu-search"))]
pub mod packing_native_bridge;
pub mod packing_problem_preparer;
pub(crate) mod packing_queue;
pub mod packing_runner;
pub(crate) mod scenario_packing_witness;

use clearra_core_ffi::{CPackingProblem, CPackingState};

pub use hybrid_scheduler_report::HybridSchedulerReport;
pub use packing_error::PackingRunnerError;
pub use packing_memory_report::{PackingMemoryLeakCheckState, PackingMemoryReport};
pub use packing_metrics::{GpuPackingBackendReport, PackingExecutionSource};
pub use packing_runner::{PackingRunResult, PackingRunner};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PackingState {
    raw: CPackingState,
}

impl PackingState {
    pub fn from_raw(raw: CPackingState) -> Self {
        Self { raw }
    }
}
impl PackingState {
    pub fn raw(self) -> CPackingState {
        self.raw
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackingExecutionPlan {
    problem: CPackingProblem,
    initial_state: PackingState,
}

impl PackingExecutionPlan {
    pub fn new(problem: CPackingProblem, initial_state: PackingState) -> Self {
        Self {
            problem,
            initial_state,
        }
    }
}
impl PackingExecutionPlan {
    pub fn problem(self) -> CPackingProblem {
        self.problem
    }
}
impl PackingExecutionPlan {
    pub fn initial_state(self) -> PackingState {
        self.initial_state
    }
}
