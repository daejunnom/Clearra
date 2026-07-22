use clearra_coverage::{
    matrix::spin_coverage_matrix::SpinCoverageMatrix,
    pattern::weighted_pattern_set::{WeightedPatternSet, WeightedPatternSetError},
    probability::union_probability::{union_probability, UnionProbabilityError},
    probability::SpinProbabilityResult,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpinTargetResultReducerError {
    Weights(WeightedPatternSetError),
    Probability(UnionProbabilityError),
}

pub struct SpinTargetResultReducer;

impl SpinTargetResultReducer {
    pub fn reduce_uniform(
        matrix: &SpinCoverageMatrix,
    ) -> Result<SpinProbabilityResult, SpinTargetResultReducerError> {
        Self::reduce_uniform_with_completeness(matrix, true, None)
    }
}
impl SpinTargetResultReducer {
    pub fn reduce_uniform_with_completeness(
        matrix: &SpinCoverageMatrix,
        probability_complete: bool,
        truncation_reason: Option<String>,
    ) -> Result<SpinProbabilityResult, SpinTargetResultReducerError> {
        let union = matrix.union_all();
        let weights = WeightedPatternSet::uniform(matrix.pattern_count())
            .map_err(SpinTargetResultReducerError::Weights)?;
        let probability = union_probability(&union, &weights)
            .map_err(SpinTargetResultReducerError::Probability)?;

        Ok(SpinProbabilityResult::new(
            matrix.spin_target_id().clone(),
            union.count_ones() as usize,
            matrix.pattern_count(),
            matrix.pattern_universe_id(),
            matrix.pattern_weight_model_id(),
            probability,
            probability_complete,
            weights.total_weight(),
            false,
            truncation_reason,
        ))
    }
}
