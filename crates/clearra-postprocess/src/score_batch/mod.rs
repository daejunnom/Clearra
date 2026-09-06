mod b2b_execution_filter;
pub mod candidate_execution_aggregate;
mod exact_replay_language;
mod exact_scoring_execution_materializer;
mod execution_supply;
pub mod score_matrix;
mod t_spin_coverage_only_materializer;

pub use b2b_execution_filter::{
    BackToBackEdgePolicy, BackToBackExecutionFilter, BackToBackFilterError,
    BackToBackFilterMemoryProjection, BackToBackFilterMemoryReport,
};
pub use candidate_execution_aggregate::{CandidateExecution, CandidateExecutionAggregate};
pub use exact_replay_language::{ExactReplayGraphLocation, ExactReplayLanguageSession};
#[cfg(feature = "stage-profiling")]
pub use exact_scoring_execution_materializer::ExactScoringExecutionProfile;
pub use exact_scoring_execution_materializer::{
    ExactReplayMaterializationError, ExactReplayMaterializationLimits,
    ExactReplayMaterializationReport, ExactScoreCellMaterialization,
    ExactScoreCellMaterializationError, ExactScoreCellMemoryProjection, ExactScoreCellMemoryReport,
    ExactScoredExecution, ExactScoringExecutionCancelled, ExactScoringExecutionMaterialization,
    ExactScoringExecutionMaterializer,
};
pub use score_matrix::{
    ScoreCell, ScoreMatrix, ScoreMatrixMemoryGuardError, ScoreMatrixMemoryProjection,
    ScoreMatrixMemoryReport,
};
pub use t_spin_coverage_only_materializer::{
    CandidatePatternCoverage, SpinCoverageTarget, TSpinCoverageMaterializationError,
    TSpinCoverageMemoryProjection, TSpinCoverageMemoryReport, TSpinCoverageOnlyMaterialization,
    TSpinCoverageOnlyMaterializer,
};
