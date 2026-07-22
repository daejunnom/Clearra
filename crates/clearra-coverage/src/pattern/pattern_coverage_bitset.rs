use super::{
    pattern_bitset::{PatternBitSet, PatternBitSetError},
    pattern_id::PatternId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternCoverageBitSet {
    inner: PatternBitSet,
}

impl PatternCoverageBitSet {
    pub fn new(inner: PatternBitSet) -> Self {
        Self { inner }
    }
}
impl PatternCoverageBitSet {
    pub fn empty(pattern_count: usize) -> Self {
        Self::new(PatternBitSet::new(pattern_count))
    }
}
impl PatternCoverageBitSet {
    pub fn from_patterns(
        pattern_count: usize,
        patterns: impl IntoIterator<Item = PatternId>,
    ) -> Result<Self, PatternBitSetError> {
        PatternBitSet::from_patterns(pattern_count, patterns).map(Self::new)
    }
}
impl PatternCoverageBitSet {
    pub fn as_pattern_bitset(&self) -> &PatternBitSet {
        &self.inner
    }
}
impl PatternCoverageBitSet {
    pub fn into_pattern_bitset(self) -> PatternBitSet {
        self.inner
    }
}

impl From<PatternBitSet> for PatternCoverageBitSet {
    fn from(value: PatternBitSet) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
#[path = "pattern_coverage_bitset_tests.rs"]
mod tests;
