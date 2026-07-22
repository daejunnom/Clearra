use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::{
    pattern::weighted_pattern_set::WeightedPatternSet,
    probability::union_probability::{union_probability, UnionProbabilityError},
};

use super::build_union_coverage::BuildUnionCoverage;

#[derive(Clone, Debug, PartialEq)]
pub struct BuildCoverageResult {
    union_coverage: BuildUnionCoverage,
    probability: ProbabilityValue,
}

impl BuildCoverageResult {
    pub fn from_union(
        union_coverage: BuildUnionCoverage,
        weights: &WeightedPatternSet,
    ) -> Result<Self, UnionProbabilityError> {
        let probability = union_probability(union_coverage.covered_patterns(), weights)?;
        Ok(Self {
            union_coverage,
            probability,
        })
    }
}
impl BuildCoverageResult {
    pub fn union_coverage(&self) -> &BuildUnionCoverage {
        &self.union_coverage
    }
}
impl BuildCoverageResult {
    pub fn probability(&self) -> ProbabilityValue {
        self.probability
    }
}

#[cfg(test)]
#[path = "build_coverage_result_tests.rs"]
mod tests;
