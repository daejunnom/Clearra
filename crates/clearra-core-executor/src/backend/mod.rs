pub mod backend_capability;
pub mod backend_fallback;
pub mod backend_kind;
pub mod backend_selector;
pub mod backend_types;
#[cfg(feature = "native-c-core")]
mod buildable_geometry_graph_executor;
#[cfg(feature = "native-c-core")]
mod buildable_geometry_task_reducer;
mod buildable_packing_executor;
pub mod cpu_geometry_exact_cover_backend;
pub mod cpu_parallel_geometry_exact_cover_backend;
pub mod gpu_execution_failure;
#[cfg(feature = "experimental-native-gpu")]
pub mod gpu_trust_state;
#[cfg(feature = "experimental-native-gpu")]
pub mod gpu_worker;
#[cfg(feature = "experimental-native-gpu")]
pub mod hybrid_backpressure_report;
mod native_packing_executor_registry;
#[cfg(test)]
mod native_packing_executors;
#[cfg(all(feature = "webgpu-search", feature = "native-c-core"))]
pub mod native_webgpu_packing_executor;
#[cfg(test)]
mod packing_backend_dispatch;
pub mod search_backend_capability_provider;
pub mod search_backend_executor;
pub mod search_backend_warmup;
pub mod wasm_build_probability_backend;
mod wasm_cpu;
pub mod wasm_cpu_search_backend;
pub mod wasm_setup_parallel_backend;
pub mod wasm_setup_search_backend;

pub use backend_capability::BackendCapability;
pub use backend_fallback::BackendFallback;
pub use backend_kind::BackendKind;
pub use backend_selector::{
    BackendSelectionError, PcBackendSelection, PcBackendSelectionContext, PcBackendSelector,
    SearchBackendReport,
};
pub use backend_types::{
    BackendSolutionTraceMode, ComputeDeviceKind, GpuDeviceSummary, GpuUnavailableReason,
    SearchBackendFallbackReason, SearchBackendSelectionReason, SearchResultModel,
    SearchTraversalModel, SelectedSearchBackend,
};
pub(crate) use buildable_packing_executor::execute_selected_buildable_packing;
pub use cpu_geometry_exact_cover_backend::CpuGeometryExactCoverBackend;
pub use cpu_parallel_geometry_exact_cover_backend::CpuParallelGeometryExactCoverBackend;
pub use gpu_execution_failure::{
    GpuExecutionFailure, GpuExecutionFailureClass, GpuExecutionFailureConstructionError,
    GpuExecutionFailureResolution, GpuExecutionFailureStage, GpuFailureDisposition,
    GpuFallbackBackend, GpuPartialResultDisposition,
};
#[cfg(feature = "experimental-native-gpu")]
pub use gpu_trust_state::GpuTrustState;
#[cfg(feature = "experimental-native-gpu")]
pub use gpu_worker::{
    GpuBackendCapability, GpuBackendError, GpuBackendKind, GpuBackendSelection, GpuCandidateHash,
    GpuCoverageBitsetOrError, GpuCoverageBitsetOrHelper, GpuCpuConfirmBridge,
    GpuCpuConfirmBridgeDecision, GpuCpuConfirmBridgeError, GpuCpuExactConfirmOptimizer,
    GpuCpuExactConfirmReport, GpuDeviceSelector, GpuDominancePrefilter,
    GpuDominancePrefilterReport, GpuExecutionCompletion, GpuFenceEpoch, GpuLargerBatchPlan,
    GpuLargerBatchPlanner, GpuMemoryTicket, GpuPackingCandidate, GpuPackingStrengthening,
    GpuPackingStrengtheningReport, GpuReadbackCompression, GpuReadbackCompressionError,
    GpuWorkerAutotune, GpuWorkerAutotuneDecision, GpuWorkerBackendReport, GpuWorkerBackpressure,
    GpuWorkerBatchSizeDecision, GpuWorkerBatchSizer, GpuWorkerBudget, GpuWorkerError,
    GpuWorkerExactnessGate, GpuWorkerMemoryPressure, GpuWorkerMemoryPressureLevel,
    GpuWorkerMetrics, GpuWorkerReduction, GpuWorkerRequest, GpuWorkerResult,
    GpuWorkerResultReducer, GpuWorkerResultValidationError, GpuWorkerState, GpuWorkerSubmission,
    GpuWorkerSubmissionStatus, PackingBatchDescriptor, PackingBatchDescriptorBuilder,
    PackingBatchId,
};
#[cfg(feature = "experimental-native-gpu")]
pub use hybrid_backpressure_report::{HybridBackpressureReport, HybridThrottleReason};
pub use native_packing_executor_registry::NativePackingExecutorRegistry;
#[cfg(test)]
pub use native_packing_executors::{
    NativeCpuPackingExecutor, NativeGpuPackingExecutor, NativeParallelPackingExecutor,
};
#[cfg(test)]
pub(crate) use packing_backend_dispatch::execute_selected_packing;
pub use search_backend_capability_provider::{
    CapabilityQueryError, GpuSearchCapability, NativeSearchBackendCapabilityProvider,
    SearchBackendCapabilityProvider,
};
#[cfg(test)]
pub use search_backend_executor::SearchBackendExecutor;
pub use search_backend_executor::{
    BackendTrustReport, BackendTrustState, PackingBackendOutcome, SearchBackendExecutorResolver,
};
pub use search_backend_warmup::{
    prewarm_gpu_search, prewarm_gpu_search_async, GpuSearchWarmupReport,
};
pub use wasm_build_probability_backend::{
    WasmBuildProbabilityAdvance, WasmBuildProbabilityBackend, WasmBuildProbabilitySession,
};
#[cfg(feature = "webgpu-search")]
pub use wasm_cpu::WasmWebGpuCandidateProducer;
pub use wasm_cpu::{
    compile_pc4_compact_tablebase, install_pc4_compact_tablebase, release_pc4_compact_tablebase,
    Pc4CompactTablebase, Pc4CompactTablebaseArtifact, Pc4TablebaseError, Pc4TablebaseLookup,
    WasmBuildProbabilityCandidateProducer, WasmBuildProbabilityDistributedResultMerger,
    WasmBuildProbabilityDistributedVerifier, WasmCandidatePacket, WasmCandidateProducerAdvance,
    WasmCpuCandidateProducer, WasmDistributedBackendExecution, WasmDistributedGeometrySummary,
    WasmDistributedProgress, WasmDistributedResultMerger, WasmDistributedVerifier,
    PC4_COMPACT_TABLEBASE_MAX_BYTES,
};
pub use wasm_cpu_search_backend::{
    WasmCpuSearchAdvance, WasmCpuSearchBackend, WasmCpuSearchError, WasmCpuSearchSession,
    WasmProductSearchBackend,
};
pub use wasm_setup_parallel_backend::{
    WasmSetupParallelCoordinator, WasmSetupParallelProduce, WasmSetupParallelWorker,
};
pub use wasm_setup_search_backend::{
    WasmSetupSearchAdvance, WasmSetupSearchBackend, WasmSetupSearchSession,
};

#[cfg(all(test, feature = "experimental-native-gpu"))]
mod hybrid_scheduler_contract_tests;
