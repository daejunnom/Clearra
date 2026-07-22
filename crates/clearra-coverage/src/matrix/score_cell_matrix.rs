use crate::{
    matrix::coverage_matrix::{CoverageMatrixError, TypedCoverageMatrix},
    pattern::pattern_bitset::PatternBitSet,
    row::{
        coverage_row::CoverageRow,
        coverage_row_kind::{CoverageRowKind, ScoreObjectiveCellId},
        score_cell_row::ScoreCellRow,
    },
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreCellMatrixBudget {
    max_rows: usize,
    max_pattern_words: usize,
}

impl ScoreCellMatrixBudget {
    pub const fn new(max_rows: usize, max_pattern_words: usize) -> Self {
        Self {
            max_rows,
            max_pattern_words,
        }
    }
}
impl ScoreCellMatrixBudget {
    pub const fn max_rows(self) -> usize {
        self.max_rows
    }
}
impl ScoreCellMatrixBudget {
    pub const fn max_pattern_words(self) -> usize {
        self.max_pattern_words
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreCellMatrix {
    score_cell_id: ScoreObjectiveCellId,
    inner: TypedCoverageMatrix,
    budget: Option<ScoreCellMatrixBudget>,
}

impl ScoreCellMatrix {
    pub fn new(
        score_cell_id: ScoreObjectiveCellId,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
    ) -> Self {
        Self {
            inner: TypedCoverageMatrix::new(
                CoverageRowKind::ScoreCell(score_cell_id.clone()),
                pattern_universe_id,
                pattern_weight_model_id,
                pattern_count,
            ),
            score_cell_id,
            budget: None,
        }
    }
}
impl ScoreCellMatrix {
    pub fn with_capacity_limit(
        score_cell_id: ScoreObjectiveCellId,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        max_pattern_count: usize,
    ) -> Result<Self, CoverageMatrixError> {
        Ok(Self {
            inner: TypedCoverageMatrix::with_capacity_limit(
                CoverageRowKind::ScoreCell(score_cell_id.clone()),
                pattern_universe_id,
                pattern_weight_model_id,
                pattern_count,
                max_pattern_count,
            )?,
            score_cell_id,
            budget: None,
        })
    }
}
impl ScoreCellMatrix {
    pub fn with_memory_budget(
        score_cell_id: ScoreObjectiveCellId,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        budget: ScoreCellMatrixBudget,
    ) -> Result<Self, CoverageMatrixError> {
        PatternBitSet::new_with_word_budget(pattern_count, budget.max_pattern_words())
            .map_err(CoverageMatrixError::Pattern)?;
        Ok(Self {
            inner: TypedCoverageMatrix::new(
                CoverageRowKind::ScoreCell(score_cell_id.clone()),
                pattern_universe_id,
                pattern_weight_model_id,
                pattern_count,
            ),
            score_cell_id,
            budget: Some(budget),
        })
    }
}
impl ScoreCellMatrix {
    pub fn push(&mut self, row: ScoreCellRow) -> Result<(), CoverageMatrixError> {
        if let Some(budget) = self.budget {
            let next_row_count = self.inner.rows().len() + 1;
            if next_row_count > budget.max_rows() {
                return Err(CoverageMatrixError::ScoreCellCapacityExceeded {
                    row_count: next_row_count,
                    row_limit: budget.max_rows(),
                });
            }
        }
        self.inner.push(row.into_row())
    }
}
impl ScoreCellMatrix {
    pub fn score_cell_id(&self) -> &ScoreObjectiveCellId {
        &self.score_cell_id
    }
}
impl ScoreCellMatrix {
    pub fn rows(&self) -> &[CoverageRow] {
        self.inner.rows()
    }
}
impl ScoreCellMatrix {
    pub fn pattern_count(&self) -> usize {
        self.inner.pattern_count()
    }
}
impl ScoreCellMatrix {
    pub fn pattern_universe_id(&self) -> PatternUniverseId {
        self.inner.pattern_universe_id()
    }
}
impl ScoreCellMatrix {
    pub fn pattern_weight_model_id(&self) -> PatternWeightModelId {
        self.inner.pattern_weight_model_id()
    }
}
impl ScoreCellMatrix {
    pub fn union_all(&self) -> PatternBitSet {
        self.inner.union_all()
    }
}
impl ScoreCellMatrix {
    pub fn inner(&self) -> &TypedCoverageMatrix {
        &self.inner
    }
}

#[cfg(test)]
#[path = "score_cell_matrix_tests.rs"]
mod tests;
