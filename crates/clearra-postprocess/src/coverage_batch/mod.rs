#[cfg(feature = "webgpu-postprocess")]
pub mod gpu_coverage_union;

#[cfg(feature = "webgpu-postprocess")]
pub use gpu_coverage_union::{PostProcessCoverageUnion, PostProcessCoverageUnionError};
