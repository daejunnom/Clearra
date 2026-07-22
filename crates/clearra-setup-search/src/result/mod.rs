mod setup_build_score;
pub mod setup_result;
pub mod setup_result_filter;
pub mod setup_result_sorter;
pub mod setup_score_aggregation;
#[cfg(test)]
mod setup_score_aggregation_tests;
mod setup_score_hierarchy;
mod setup_score_input;

pub use setup_build_score::SetupBuildScore;
pub use setup_result::SetupResult;
pub use setup_result_filter::SetupResultFilter;
pub use setup_result_sorter::SetupResultSorter;
pub use setup_score_aggregation::{SetupScoreAggregation, SetupScoreAggregationError};
pub use setup_score_hierarchy::{SetupFamilyScore, SetupTilingScore};
pub use setup_score_input::SetupBuildScoreInput;
