use crate::pattern::pattern_bitset::PatternBitSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverageRow {
    candidate_id: usize,
    patterns: PatternBitSet,
}

impl CoverageRow {
    pub fn new(candidate_id: usize, patterns: PatternBitSet) -> Self {
        Self {
            candidate_id,
            patterns,
        }
    }
}
impl CoverageRow {
    pub fn candidate_id(&self) -> usize {
        self.candidate_id
    }
}
impl CoverageRow {
    pub fn patterns(&self) -> &PatternBitSet {
        &self.patterns
    }
}
