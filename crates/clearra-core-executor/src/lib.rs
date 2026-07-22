//! Rust executor facade for the connected core-c packing path.

pub mod area;
pub mod backend;
pub mod board;
pub mod buildup;
pub mod core_execution_result;
pub mod core_executor;
pub mod core_postprocess_execution;
pub mod core_postprocess_score_cell;
pub mod core_postprocess_spin_coverage;
#[cfg(feature = "parallel")]
mod cpu_worker_pool;
pub mod diagnostics;
#[cfg(test)]
mod execution_worker_limit;
pub mod memory;
#[cfg(test)]
pub(crate) mod order_language;
pub mod packing;
pub mod performance;
pub mod problem_lowering;
pub mod resource;
pub mod result_views;
pub mod service;
pub mod solution_probability;
pub mod spin;

#[cfg(feature = "webgpu-search")]
pub use backend::WasmWebGpuCandidateProducer;
pub use backend::{
    WasmBuildProbabilityAdvance, WasmBuildProbabilityBackend,
    WasmBuildProbabilityCandidateProducer, WasmBuildProbabilityDistributedResultMerger,
    WasmBuildProbabilityDistributedVerifier, WasmBuildProbabilitySession, WasmCandidatePacket,
    WasmCandidateProducerAdvance, WasmCpuCandidateProducer, WasmCpuSearchAdvance,
    WasmCpuSearchBackend, WasmCpuSearchError, WasmCpuSearchSession,
    WasmDistributedBackendExecution, WasmDistributedGeometrySummary, WasmDistributedProgress,
    WasmDistributedResultMerger, WasmDistributedVerifier, WasmProductSearchBackend,
};
pub use buildup::{
    BuildUpEvent, BuildUpReducerReport, BuildUpRunResult, BuildUpRunner, BuildUpState,
};
pub use core_execution_result::{CoreExecutionResult, CorePathStep};
pub use core_executor::{CoreExecutionError, CoreExecutor};
pub use core_postprocess_execution::CorePostProcessExecution;
pub use core_postprocess_score_cell::CorePostProcessScoreCell;
pub use core_postprocess_spin_coverage::CorePostProcessSpinCoverage;
pub use memory::ScopeGuard;
pub use packing::{PackingExecutionPlan, PackingRunResult, PackingRunner, PackingState};
#[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
pub use performance::{
    ExecutorSearchProfileError, ExecutorSearchProfileSession, ExecutorSearchProfileStage,
};
pub use result_views::{
    BackendReport, BuildUpResult, BuildVariantView, CoverageResult, CoverageRowView,
    ObjectiveResult, PackingCandidateView, PackingResult, ReplayTrace, SearchExecutionReport,
};
pub use service::{
    CoverService, CoverServiceError, PcService, PcServiceError, PercentService,
    PercentServiceError, SetupService, SetupServiceError,
};
pub use solution_probability::{SolutionCoverage, SolutionProbabilityReport};
pub use spin::{BuildVariantReplayEvidence, BuildVariantReplayEvidenceError};

pub fn native_core_runtime_available() -> bool {
    clearra_core_ffi::CoreCNative::linked()
}
#[cfg(test)]
pub use spin::{
    SpinProbabilityResult, SpinTargetExecutionReport, SpinTargetRunResult, SpinTargetRunner,
    SpinTargetRunnerError,
};
