use clearra_core_domain::probability::probability_value::ProbabilityValue;

use crate::{
    pattern::{pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet},
    probability::probability_guard::{guard_probability, ProbabilityGuardError},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnionProbabilityError {
    MissingWeight { pattern_index: usize },
    Probability(ProbabilityGuardError),
}

pub fn union_probability(
    covered_patterns: &PatternBitSet,
    weights: &WeightedPatternSet,
) -> Result<ProbabilityValue, UnionProbabilityError> {
    let mut total = 0.0;
    for pattern in covered_patterns.covered_patterns() {
        let weight = weights
            .weight(pattern)
            .ok_or(UnionProbabilityError::MissingWeight {
                pattern_index: pattern.index(),
            })?;
        total += weight.get();
    }

    guard_probability(total).map_err(UnionProbabilityError::Probability)
}

#[cfg(test)]
mod tests {
    use crate::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};

    use super::*;

    #[test]
    fn overlapping_patterns_are_measured_once_after_union() {
        let weights = WeightedPatternSet::uniform(4).expect("uniform weights");
        let first = PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(1)])
            .expect("first coverage");
        let second = PatternBitSet::from_patterns(4, [PatternId::new(1), PatternId::new(2)])
            .expect("second coverage");

        let union = first.union(&second).expect("matching pattern universe");
        let probability = union_probability(&union, &weights).expect("valid probability");

        assert_eq!(probability.get(), 0.75);
    }
}
