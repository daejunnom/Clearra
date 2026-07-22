use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};

use crate::{
    coverage::{SetupCoverageBuilder, SetupUnionCoverage},
    identity::shape_family::ShapeFamily,
    variant::build_variant::BuildVariant,
};

use super::setup_search_service::SetupSearchExecutionError;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SetupCoveragePlan {
    family: ShapeFamily,
    builder: SetupCoverageBuilder,
}

impl SetupCoveragePlan {
    pub(crate) fn new(family: ShapeFamily, pattern_count: usize) -> Self {
        Self {
            family,
            builder: SetupCoverageBuilder::new(family, pattern_count),
        }
    }
}
impl SetupCoveragePlan {
    pub(crate) fn push_variant(
        &mut self,
        variant: &BuildVariant,
    ) -> Result<(), SetupSearchExecutionError> {
        self.builder
            .push_variant(variant)
            .map_err(|_| SetupSearchExecutionError::BuildCoverage)
    }
}
impl SetupCoveragePlan {
    pub(crate) fn build_union(self) -> Result<SetupUnionCoverage, SetupSearchExecutionError> {
        let matrix = self
            .builder
            .build()
            .map_err(|_| SetupSearchExecutionError::BuildCoverage)?;
        Ok(SetupUnionCoverage::from_matrix(self.family.id(), &matrix))
    }
}

pub(crate) fn coverage_for_patterns(
    pattern_count: usize,
    patterns: impl IntoIterator<Item = usize>,
) -> Result<PatternBitSet, SetupSearchExecutionError> {
    PatternBitSet::from_patterns(pattern_count, patterns.into_iter().map(PatternId::new))
        .map_err(|_| SetupSearchExecutionError::BuildCoverage)
}
