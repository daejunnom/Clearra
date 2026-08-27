pub mod distributed_runtime;
mod distributed_wire;
pub mod host_contract_bridge;
mod json_event_envelope;
pub mod wasm_command_runtime;
pub mod wasm_host_capabilities;
pub mod wasm_worker_job;
pub mod webgpu;

pub use clearra_app::{
    CoveragePortfolioPageStore, PortfolioAlternativeAdvance, PortfolioAlternativeError,
    PortfolioAlternativePage, PortfolioMemberPage, ProductPageSourceOwner, ProductPageStore,
};
#[cfg(feature = "stage-profiling")]
pub use clearra_app::{
    ExecutorSearchProfileError, ExecutorSearchProfileSession, ExecutorSearchProfileStage,
};
#[cfg(feature = "webgpu-search")]
pub use clearra_core_executor::backend::{
    prewarm_gpu_search, prewarm_gpu_search_async, GpuSearchWarmupReport,
};
pub use clearra_core_executor::{
    install_pc4_compact_tablebase, release_pc4_compact_tablebase, Pc4TablebaseError,
    TilingSolutionPageStore,
};
pub use distributed_runtime::{
    serialize_distributed_final_events, WasmDistributedCoordinator, WasmDistributedFallbackReason,
    WasmDistributedMode, WasmDistributedPreparation, WasmDistributedProducerAdvance,
    WasmDistributedRequestedBackend, WasmDistributedVerifierRuntime,
};
pub use distributed_wire::{
    decode_candidate_batch, decode_partial_result, encode_candidate_batch, encode_partial_result,
    DistributedWireError,
};
pub use host_contract_bridge::wasm_worker_event_to_host_contract;
pub use json_event_envelope::{
    serialize_coverage_portfolio_advance_state, serialize_coverage_portfolio_page,
    serialize_governed_worker_events, serialize_parity_report_exhausted,
    serialize_parity_report_page, serialize_search_report_from_app_response, GovernedWasmJson,
};
pub use wasm_command_runtime::{
    GovernedWasmExecutionResult, WasmCommandRuntime, WasmCommandRuntimeError,
    WasmExecutionMemoryAuthority, WasmExecutionResult, WasmFinessePlacement,
    WasmFinessePolicyResult, WasmFinesseReport, WasmFinesseRepresentativeWitness,
    WasmFinesseSolutionAverage, WasmForwardPathStep, WasmForwardSearchOutcome, WasmSearchPathStep,
    WasmSearchReport, WasmSetupCandidate, WasmSetupFinderReport, WasmSetupHoldCondition,
    WasmSolutionAverageScore, WasmSolutionProbability, WasmSpinStructureOutcome,
    WasmSpinStructureReport, WasmStructureOperation,
};
pub use wasm_host_capabilities::WasmHostCapabilities;
pub use wasm_worker_job::{
    BackendStatus, BudgetStatus, CancelRequest, GovernedWasmWorkerEvents, JobDiagnosticEvent,
    JobFinalResponse, JobId, JobPartialResult, JobProgress, JobStatus, MemoryStatus,
    WasmCancellationToken, WasmWorkerAdvanceStatus, WasmWorkerJobEvent, WasmWorkerJobId,
    WasmWorkerJobRuntime, WasmWorkerJobStatus,
};
pub use webgpu::{
    WebGpuBackendOutcomeState, WebGpuBackendReport, WebGpuLimitsReport, WebGpuMemoryReport,
    WebGpuReportTrustState, WebGpuShaderReport,
};

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
