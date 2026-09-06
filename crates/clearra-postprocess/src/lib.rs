//! Connected post-processing for buildable execution evidence.
//!
//! Search proves PC/buildability. This crate evaluates owned replay traces into
//! score cells and may offload PatternBitSet union to a CPU-confirmed WebGPU
//! backend. Neither path can change PC search coverage truth.

pub mod coverage_batch;
pub mod pc_scoring;
pub mod score_batch;
mod score_profile_selection;

pub use score_profile_selection::{
    checked_score_profile_memory_projection, score_profile_with_memory_guard,
    ScoreProfileMemoryGuardError, ScoreProfileMemoryProjection, ScoreProfileMemoryReport,
};

#[cfg(feature = "webgpu-postprocess")]
pub use coverage_batch::{PostProcessCoverageUnion, PostProcessCoverageUnionError};
pub use pc_scoring::{
    PcPostProcessCancelled, PcScoringMemoryGuardError, PcScoringMemoryProjection,
    PcScoringMemoryReport, PcScoringPostProcessInput, PcScoringPostProcessResult,
    PcScoringPostProcessor,
};
#[cfg(feature = "stage-profiling")]
pub use score_batch::ExactScoringExecutionProfile;
pub use score_batch::{
    BackToBackEdgePolicy, BackToBackExecutionFilter, BackToBackFilterError,
    BackToBackFilterMemoryProjection, BackToBackFilterMemoryReport, CandidateExecution,
    CandidateExecutionAggregate, CandidatePatternCoverage, ExactReplayGraphLocation,
    ExactReplayLanguageSession, ExactReplayMaterializationError, ExactReplayMaterializationLimits,
    ExactReplayMaterializationReport, ExactScoreCellMaterialization,
    ExactScoreCellMaterializationError, ExactScoreCellMemoryProjection, ExactScoreCellMemoryReport,
    ExactScoredExecution, ExactScoringExecutionCancelled, ExactScoringExecutionMaterialization,
    ExactScoringExecutionMaterializer, ScoreCell, ScoreMatrix, ScoreMatrixMemoryGuardError,
    ScoreMatrixMemoryProjection, ScoreMatrixMemoryReport, SpinCoverageTarget,
    TSpinCoverageMaterializationError, TSpinCoverageMemoryProjection, TSpinCoverageMemoryReport,
    TSpinCoverageOnlyMaterialization, TSpinCoverageOnlyMaterializer,
};
