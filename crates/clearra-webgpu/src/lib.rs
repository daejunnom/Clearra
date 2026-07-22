pub mod adapter_selection;
pub mod geometry_exact_cover_backend;
mod geometry_exact_cover_cpu_confirm;
mod geometry_exact_cover_dispatch;
mod geometry_exact_cover_model;
mod geometry_exact_cover_reduce;
mod geometry_exact_cover_result;
mod geometry_exact_cover_timing;
pub mod shader_contract;
pub mod webgpu_backend;

pub use geometry_exact_cover_backend::{
    WebGpuExactCoverCatalog, WebGpuGeometryCatalogIdentity, WebGpuGeometryExactCoverBackend,
    WebGpuGeometryExactCoverBatch, WebGpuGeometryExactCoverConnected,
    WebGpuGeometryExactCoverIncomplete, WebGpuGeometryExactCoverInputError,
    WebGpuGeometryExactCoverOutcome, WebGpuGeometryExactCoverSession,
    WebGpuGeometryExactCoverSessionOutcome, WebGpuPackingTrustState, WebGpuPlacementSkeleton,
};
pub use geometry_exact_cover_result::{
    WebGpuGeometryCandidatePath, WebGpuGeometryPathCursor, WebGpuGeometryPathStreamError,
    WebGpuGeometrySolutionGraph,
};
pub use geometry_exact_cover_timing::WebGpuGeometryExactCoverTimings;
pub use shader_contract::{WebGpuShaderContract, WebGpuShaderContractError};
pub use webgpu_backend::{
    WebGpuBackend, WebGpuBatchInputError, WebGpuBatchOutcome, WebGpuBitsetBatch,
    WebGpuConnectedResult, WebGpuLimits, WebGpuRejectedMismatch, WebGpuTrustState,
    WebGpuUnavailableResult,
};

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
pub use adapter_selection::{
    enumerate_adapter_summaries, select_adapter_summary, WebGpuAdapterDeviceType,
    WebGpuAdapterSelection, WebGpuAdapterSelectionError, WebGpuAdapterSummary,
};
