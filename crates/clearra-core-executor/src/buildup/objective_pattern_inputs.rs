use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, weighted_pattern_set::WeightedPatternSet,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ObjectivePatternInputs {
    required_patterns: PatternBitSet,
    weights: WeightedPatternSet,
}

impl ObjectivePatternInputs {
    pub(crate) fn new(required_patterns: PatternBitSet, weights: WeightedPatternSet) -> Self {
        Self {
            required_patterns,
            weights,
        }
    }
}
impl ObjectivePatternInputs {
    pub(crate) fn required_patterns(&self) -> &PatternBitSet {
        &self.required_patterns
    }
}
impl ObjectivePatternInputs {
    pub(crate) fn weights(&self) -> &WeightedPatternSet {
        &self.weights
    }
}
