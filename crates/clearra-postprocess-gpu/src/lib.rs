//! GPU/WebGPU trust contracts for PostProcess-only bulk work.
//!
//! Search GPU and PostProcess GPU are separate job types. PostProcess GPU may
//! accelerate replay/evidence/score/render batches, but it cannot change search
//! backend reports or PC coverage truth.

pub mod backend_policy;
pub mod post_gpu_capability;
pub mod post_gpu_result;
pub mod post_gpu_trust_state;
pub mod postprocess_gpu_backend;

pub use backend_policy::{
    BackendFallbackPolicy, BackendPolicy, GpuDeviceSelection, PostBackendRequest,
    SearchBackendRequest,
};
pub use post_gpu_capability::{PostGpuCapability, PostGpuCapabilityState};
pub use post_gpu_result::PostGpuResult;
pub use post_gpu_trust_state::PostGpuTrustState;
pub use postprocess_gpu_backend::{PostProcessGpuBackend, PostProcessGpuError};
