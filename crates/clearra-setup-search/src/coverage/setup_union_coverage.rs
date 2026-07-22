use clearra_core_domain::ids::setup_id::SetupFamilyId;
use clearra_coverage::{
    matrix::coverage_matrix::TypedCoverageMatrix, pattern::pattern_bitset::PatternBitSet,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupUnionCoverage {
    family_id: SetupFamilyId,
    covered_patterns: PatternBitSet,
}

impl SetupUnionCoverage {
    pub fn from_matrix(family_id: SetupFamilyId, matrix: &TypedCoverageMatrix) -> Self {
        Self {
            family_id,
            covered_patterns: matrix.union_all(),
        }
    }
}
impl SetupUnionCoverage {
    pub fn family_id(&self) -> SetupFamilyId {
        self.family_id
    }
}
impl SetupUnionCoverage {
    pub fn covered_patterns(&self) -> &PatternBitSet {
        &self.covered_patterns
    }
}
