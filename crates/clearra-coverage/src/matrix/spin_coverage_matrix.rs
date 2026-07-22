use clearra_core_domain::ids::SpinTargetId;

use crate::{
    matrix::{
        coverage_matrix::{CoverageMatrixError, TypedCoverageMatrix},
        coverage_matrix_error,
    },
    pattern::pattern_bitset::PatternBitSet,
    row::{
        coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind,
        spin_coverage_row::SpinCoverageRow,
    },
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpinCoverageMatrixBudget {
    max_rows: usize,
    max_pattern_words: usize,
}

impl SpinCoverageMatrixBudget {
    pub const fn new(max_rows: usize, max_pattern_words: usize) -> Self {
        Self {
            max_rows,
            max_pattern_words,
        }
    }
}
impl SpinCoverageMatrixBudget {
    pub const fn max_rows(self) -> usize {
        self.max_rows
    }
}
impl SpinCoverageMatrixBudget {
    pub const fn max_pattern_words(self) -> usize {
        self.max_pattern_words
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinCoverageMatrix {
    spin_target_id: SpinTargetId,
    inner: TypedCoverageMatrix,
    budget: Option<SpinCoverageMatrixBudget>,
}

impl SpinCoverageMatrix {
    pub fn new(
        spin_target_id: SpinTargetId,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
    ) -> Self {
        Self {
            inner: TypedCoverageMatrix::new(
                CoverageRowKind::SpinTarget(spin_target_id.clone()),
                pattern_universe_id,
                pattern_weight_model_id,
                pattern_count,
            ),
            spin_target_id,
            budget: None,
        }
    }
}
impl SpinCoverageMatrix {
    pub fn with_capacity_limit(
        spin_target_id: SpinTargetId,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        max_pattern_count: usize,
    ) -> Result<Self, coverage_matrix_error::CoverageMatrixError> {
        Ok(Self {
            inner: TypedCoverageMatrix::with_capacity_limit(
                CoverageRowKind::SpinTarget(spin_target_id.clone()),
                pattern_universe_id,
                pattern_weight_model_id,
                pattern_count,
                max_pattern_count,
            )?,
            spin_target_id,
            budget: None,
        })
    }
}
impl SpinCoverageMatrix {
    pub fn with_memory_budget(
        spin_target_id: SpinTargetId,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        budget: SpinCoverageMatrixBudget,
    ) -> Result<Self, CoverageMatrixError> {
        PatternBitSet::new_with_word_budget(pattern_count, budget.max_pattern_words())
            .map_err(CoverageMatrixError::Pattern)?;
        Ok(Self {
            inner: TypedCoverageMatrix::new(
                CoverageRowKind::SpinTarget(spin_target_id.clone()),
                pattern_universe_id,
                pattern_weight_model_id,
                pattern_count,
            ),
            spin_target_id,
            budget: Some(budget),
        })
    }
}
impl SpinCoverageMatrix {
    pub fn push(&mut self, row: SpinCoverageRow) -> Result<(), CoverageMatrixError> {
        if let Some(budget) = self.budget {
            let next_row_count = self.inner.rows().len() + 1;
            if next_row_count > budget.max_rows() {
                return Err(CoverageMatrixError::SpinCoverageCapacityExceeded {
                    row_count: next_row_count,
                    row_limit: budget.max_rows(),
                });
            }
        }
        self.inner.push(row.into_row())
    }
}
impl SpinCoverageMatrix {
    pub fn spin_target_id(&self) -> &SpinTargetId {
        &self.spin_target_id
    }
}
impl SpinCoverageMatrix {
    pub fn rows(&self) -> &[CoverageRow] {
        self.inner.rows()
    }
}
impl SpinCoverageMatrix {
    pub fn pattern_count(&self) -> usize {
        self.inner.pattern_count()
    }
}
impl SpinCoverageMatrix {
    pub fn pattern_universe_id(&self) -> PatternUniverseId {
        self.inner.pattern_universe_id()
    }
}
impl SpinCoverageMatrix {
    pub fn pattern_weight_model_id(&self) -> PatternWeightModelId {
        self.inner.pattern_weight_model_id()
    }
}
impl SpinCoverageMatrix {
    pub fn union_all(&self) -> PatternBitSet {
        self.inner.union_all()
    }
}
impl SpinCoverageMatrix {
    pub fn inner(&self) -> &TypedCoverageMatrix {
        &self.inner
    }
}

#[cfg(test)]
#[path = "spin_coverage_matrix_tests.rs"]
mod tests;
