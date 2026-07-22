pub mod candidate_execution_aggregate;
mod exact_scoring_execution_materializer;
mod execution_supply;
pub mod score_matrix;
mod t_spin_coverage_only_materializer;

pub use candidate_execution_aggregate::{CandidateExecution, CandidateExecutionAggregate};
#[cfg(feature = "stage-profiling")]
pub use exact_scoring_execution_materializer::ExactScoringExecutionProfile;
pub use exact_scoring_execution_materializer::{
    ExactScoredExecution, ExactScoringExecutionCancelled, ExactScoringExecutionMaterialization,
    ExactScoringExecutionMaterializer,
};
pub use score_matrix::{ScoreCell, ScoreMatrix};
pub use t_spin_coverage_only_materializer::{
    SpinCoverageTarget, TSpinCoverageOnlyMaterialization, TSpinCoverageOnlyMaterializer,
};
