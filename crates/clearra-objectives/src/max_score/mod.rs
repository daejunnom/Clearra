pub mod materialized_score_matrix;
pub mod max_score_cover;
mod max_score_portfolio_enumerator;
pub mod max_score_selection;
mod optimal_pattern_minimum_cover;
pub mod score_aware_objective_invariant;
pub mod scored_coverage_candidate;

pub use materialized_score_matrix::{MaterializedScoreCell, MaterializedScoreMatrix};
pub use max_score_cover::MaxScoreCover;
pub use max_score_portfolio_enumerator::{
    MaxScoreCoverPortfolio, MaxScoreCoverPortfolioEnumerator, MaxScoreCoverPortfolioPage,
    MaxScoreCoverPortfolioRestart,
};
pub use max_score_selection::{
    MaxScoreCoverError, MaxScoreCoverPolicy, MaxScoreCoverPolicyError, MaxScoreCoverResult,
    PatternScoreContribution,
};
pub use score_aware_objective_invariant::ScoreAwareObjectiveInvariantReport;
pub use scored_coverage_candidate::ScoredCoverageCandidate;
