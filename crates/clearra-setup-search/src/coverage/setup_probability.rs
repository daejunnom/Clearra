use clearra_core_domain::{
    ids::setup_id::SetupFamilyId, probability::probability_value::ProbabilityValue,
};
use clearra_coverage::{
    pattern::weighted_pattern_set::WeightedPatternSet,
    probability::union_probability::{union_probability, UnionProbabilityError},
};

use super::setup_union_coverage::SetupUnionCoverage;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetupProbability {
    family_id: SetupFamilyId,
    probability: ProbabilityValue,
}

impl SetupProbability {
    pub fn from_union(
        union: &SetupUnionCoverage,
        weights: &WeightedPatternSet,
    ) -> Result<Self, UnionProbabilityError> {
        Ok(Self {
            family_id: union.family_id(),
            probability: union_probability(union.covered_patterns(), weights)?,
        })
    }
}
impl SetupProbability {
    pub fn family_id(self) -> SetupFamilyId {
        self.family_id
    }
}
impl SetupProbability {
    pub fn probability(self) -> ProbabilityValue {
        self.probability
    }
}

#[cfg(test)]
#[path = "setup_probability_tests.rs"]
mod tests;
