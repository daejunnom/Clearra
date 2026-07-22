use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};
use clearra_supply::piece_source::PieceSource;

use crate::buildup::{
    buildup_error::BuildUpRunnerError, objective_incomplete_reason::ObjectiveIncompleteReason,
    objective_pattern_inputs::ObjectivePatternInputs,
    objective_pattern_materialization::ObjectivePatternMaterialization,
};

pub(crate) fn materialize_objective_pattern_inputs(
    piece_source: &PieceSource,
    pattern_count: usize,
) -> Result<ObjectivePatternMaterialization, BuildUpRunnerError> {
    let required_patterns =
        PatternBitSet::from_patterns(pattern_count, (0..pattern_count).map(PatternId::new))
            .map_err(BuildUpRunnerError::Pattern)?;
    let Some(weights) = piece_source.materialized_pattern_weights() else {
        return Ok(ObjectivePatternMaterialization::Incomplete(
            ObjectiveIncompleteReason::PatternWeightModelNotMaterialized,
        ));
    };
    if weights.len() != pattern_count {
        return Ok(ObjectivePatternMaterialization::Incomplete(
            ObjectiveIncompleteReason::PatternWeightCountMismatch,
        ));
    }
    Ok(ObjectivePatternMaterialization::Ready(
        ObjectivePatternInputs::new(required_patterns, weights.clone()),
    ))
}
