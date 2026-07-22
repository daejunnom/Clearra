use crate::{
    matrix::coverage_matrix_error::CoverageMatrixError,
    pattern::pattern_bitset::PatternBitSet,
    row::coverage_row::CoverageRow,
    universe::{
        coverage_pattern_budget::CoveragePatternBudget, pattern_universe_id::PatternUniverseId,
        pattern_weight_model_id::PatternWeightModelId,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoverageUniverseGuard {
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    pattern_count: usize,
}

impl CoverageUniverseGuard {
    pub const fn new(
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
    ) -> Self {
        Self {
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
        }
    }
}
impl CoverageUniverseGuard {
    pub fn with_capacity_limit(
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        max_pattern_count: usize,
    ) -> Result<Self, CoverageMatrixError> {
        CoveragePatternBudget::custom(max_pattern_count).check(pattern_count)?;
        Ok(Self::new(
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
        ))
    }
}
impl CoverageUniverseGuard {
    pub fn check_bits(&self, bits: &PatternBitSet) -> Result<(), CoverageMatrixError> {
        self.check_identity()?;
        if bits.pattern_count() != self.pattern_count {
            return Err(CoverageMatrixError::RowPatternCountMismatch {
                expected: self.pattern_count,
                actual: bits.pattern_count(),
            });
        }
        Ok(())
    }
}
impl CoverageUniverseGuard {
    pub fn check_row(&self, row: &CoverageRow) -> Result<(), CoverageMatrixError> {
        self.check_identity()?;
        if row.pattern_universe_id().get() == 0 {
            return Err(CoverageMatrixError::MissingPatternUniverseIdentity);
        }
        if row.pattern_weight_model_id().get() == 0 {
            return Err(CoverageMatrixError::MissingPatternWeightModelIdentity);
        }
        if row.pattern_universe_id() != self.pattern_universe_id {
            return Err(CoverageMatrixError::PatternUniverseIdMismatch {
                expected: self.pattern_universe_id,
                actual: row.pattern_universe_id(),
            });
        }
        if row.pattern_weight_model_id() != self.pattern_weight_model_id {
            return Err(CoverageMatrixError::PatternWeightModelIdMismatch {
                expected: self.pattern_weight_model_id,
                actual: row.pattern_weight_model_id(),
            });
        }
        self.check_bits(row.coverage_bits())
    }
}
impl CoverageUniverseGuard {
    pub fn pattern_universe_id(self) -> PatternUniverseId {
        self.pattern_universe_id
    }
}
impl CoverageUniverseGuard {
    pub fn pattern_weight_model_id(self) -> PatternWeightModelId {
        self.pattern_weight_model_id
    }
}
impl CoverageUniverseGuard {
    pub fn pattern_count(self) -> usize {
        self.pattern_count
    }
}
impl CoverageUniverseGuard {
    fn check_identity(&self) -> Result<(), CoverageMatrixError> {
        if self.pattern_universe_id.get() == 0 {
            return Err(CoverageMatrixError::MissingPatternUniverseIdentity);
        }
        if self.pattern_weight_model_id.get() == 0 {
            return Err(CoverageMatrixError::MissingPatternWeightModelIdentity);
        }
        Ok(())
    }
}
