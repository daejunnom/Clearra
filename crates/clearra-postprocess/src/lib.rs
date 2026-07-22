//! Connected post-processing for buildable execution evidence.
//!
//! Search proves PC/buildability. This crate evaluates owned replay traces into
//! score cells and may offload PatternBitSet union to a CPU-confirmed WebGPU
//! backend. Neither path can change PC search coverage truth.

pub mod coverage_batch;
pub mod pc_scoring;
pub mod score_batch;
mod score_profile_selection;

#[cfg(feature = "webgpu-postprocess")]
pub use coverage_batch::{PostProcessCoverageUnion, PostProcessCoverageUnionError};
pub use pc_scoring::{
    PcPostProcessCancelled, PcScoringPostProcessInput, PcScoringPostProcessResult,
    PcScoringPostProcessor,
};
#[cfg(feature = "stage-profiling")]
pub use score_batch::ExactScoringExecutionProfile;
pub use score_batch::{
    CandidateExecution, CandidateExecutionAggregate, ExactScoredExecution,
    ExactScoringExecutionCancelled, ExactScoringExecutionMaterialization,
    ExactScoringExecutionMaterializer, ScoreCell, ScoreMatrix, SpinCoverageTarget,
    TSpinCoverageOnlyMaterialization, TSpinCoverageOnlyMaterializer,
};
