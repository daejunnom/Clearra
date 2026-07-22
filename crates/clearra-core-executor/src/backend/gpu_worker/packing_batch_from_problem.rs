use clearra_core_ffi::CPackingProblem;
use clearra_problem::SearchProblem;

use crate::buildup::buildup_coverage_bridge::coverage_universe_identity;

use super::{PackingBatchId, PackingBatchSource, PackingBatchSourceError};

pub fn packing_batch_source_from_problem(
    problem: &SearchProblem,
    compact: &CPackingProblem,
    batch_id: Option<PackingBatchId>,
    pattern_universe_id: Option<u64>,
    pattern_weight_model_id: Option<u64>,
) -> Result<PackingBatchSource, PackingBatchSourceError> {
    let identity = coverage_universe_identity(problem);
    let candidate_capacity = problem.budget().max_results().min(u32::MAX as usize) as u32;

    PackingBatchSource::from_compact_problem_with_identity(
        compact,
        batch_id,
        pattern_universe_id.unwrap_or(identity.pattern_universe_id),
        pattern_weight_model_id.unwrap_or(identity.pattern_weight_model_id),
        Some(candidate_capacity),
    )
}
