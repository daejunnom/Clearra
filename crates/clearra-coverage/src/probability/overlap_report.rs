use crate::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverlapReport {
    overlapping_patterns: Vec<PatternId>,
}

impl OverlapReport {
    pub fn between(left: &PatternBitSet, right: &PatternBitSet) -> Self {
        let overlapping_patterns = left
            .covered_patterns()
            .into_iter()
            .filter(|pattern| right.contains(*pattern))
            .collect();
        Self {
            overlapping_patterns,
        }
    }
}
impl OverlapReport {
    pub fn overlapping_patterns(&self) -> &[PatternId] {
        &self.overlapping_patterns
    }
}
impl OverlapReport {
    pub fn has_overlap(&self) -> bool {
        !self.overlapping_patterns.is_empty()
    }
}
