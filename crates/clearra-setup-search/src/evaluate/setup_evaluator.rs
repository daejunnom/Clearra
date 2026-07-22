use clearra_coverage::{
    pattern::weighted_pattern_set::WeightedPatternSet,
    probability::union_probability::UnionProbabilityError,
};

use crate::{
    coverage::{setup_probability::SetupProbability, setup_union_coverage::SetupUnionCoverage},
    result::setup_result::SetupResult,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupEvaluator;

impl SetupEvaluator {
    pub fn evaluate_union(
        union: SetupUnionCoverage,
        weights: &WeightedPatternSet,
    ) -> Result<SetupResult, UnionProbabilityError> {
        let probability = SetupProbability::from_union(&union, weights)?;
        Ok(SetupResult::new(
            probability.family_id(),
            probability.probability(),
            union,
        ))
    }
}
