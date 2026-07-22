use clearra_coverage::{
    matrix::coverage_matrix::TypedCoverageMatrix, pattern::pattern_bitset::PatternBitSet,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildUnionCoverage {
    covered_patterns: PatternBitSet,
}

impl BuildUnionCoverage {
    pub fn from_matrix(matrix: &TypedCoverageMatrix) -> Self {
        Self {
            covered_patterns: matrix.union_all(),
        }
    }
}
impl BuildUnionCoverage {
    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }
}
