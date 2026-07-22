use crate::{
    matrix::coverage_row::CoverageRow,
    pattern::pattern_bitset::PatternBitSet,
    row::{coverage_row::CoverageRow as TypedCoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        coverage_universe_guard::CoverageUniverseGuard, pattern_universe_id::PatternUniverseId,
        pattern_weight_model_id::PatternWeightModelId,
    },
};

pub use crate::matrix::coverage_matrix_error::CoverageMatrixError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverageMatrix {
    pattern_count: usize,
    rows: Vec<CoverageRow>,
}

impl CoverageMatrix {
    pub fn new(pattern_count: usize) -> Self {
        Self {
            pattern_count,
            rows: Vec::new(),
        }
    }
}
impl CoverageMatrix {
    pub fn from_rows(
        pattern_count: usize,
        rows: Vec<CoverageRow>,
    ) -> Result<Self, CoverageMatrixError> {
        let mut matrix = Self::new(pattern_count);
        for row in rows {
            matrix.push(row)?;
        }
        Ok(matrix)
    }
}
impl CoverageMatrix {
    pub fn push(&mut self, row: CoverageRow) -> Result<(), CoverageMatrixError> {
        if row.patterns().pattern_count() != self.pattern_count {
            return Err(CoverageMatrixError::RowPatternCountMismatch {
                expected: self.pattern_count,
                actual: row.patterns().pattern_count(),
            });
        }
        self.rows.push(row);
        Ok(())
    }
}
impl CoverageMatrix {
    pub fn pattern_count(&self) -> usize {
        self.pattern_count
    }
}
impl CoverageMatrix {
    pub fn rows(&self) -> &[CoverageRow] {
        &self.rows
    }
}
impl CoverageMatrix {
    pub fn row(&self, index: usize) -> Option<&CoverageRow> {
        self.rows.get(index)
    }
}
impl CoverageMatrix {
    pub fn union_all(&self) -> PatternBitSet {
        let mut union = PatternBitSet::new(self.pattern_count);
        for row in &self.rows {
            union
                .union_with(row.patterns())
                .expect("coverage matrix row pattern_count invariant");
        }
        union
    }
}
impl CoverageMatrix {
    pub fn union_rows(&self, row_indices: &[usize]) -> Result<PatternBitSet, CoverageMatrixError> {
        let mut union = PatternBitSet::new(self.pattern_count);
        for index in row_indices {
            let row = self
                .row(*index)
                .ok_or(CoverageMatrixError::RowIndexOutOfRange {
                    index: *index,
                    row_count: self.rows.len(),
                })?;
            union
                .union_with(row.patterns())
                .expect("coverage matrix row pattern_count invariant");
        }
        Ok(union)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedCoverageMatrix {
    guard: CoverageUniverseGuard,
    row_kind: CoverageRowKind,
    piece_source_id: Option<u64>,
    rows: Vec<TypedCoverageRow>,
}

impl TypedCoverageMatrix {
    pub fn new(
        row_kind: CoverageRowKind,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
    ) -> Self {
        Self {
            guard: CoverageUniverseGuard::new(
                pattern_universe_id,
                pattern_weight_model_id,
                pattern_count,
            ),
            row_kind,
            piece_source_id: None,
            rows: Vec::new(),
        }
    }
}
impl TypedCoverageMatrix {
    pub fn new_with_piece_source(
        row_kind: CoverageRowKind,
        piece_source_id: u64,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
    ) -> Self {
        Self {
            guard: CoverageUniverseGuard::new(
                pattern_universe_id,
                pattern_weight_model_id,
                pattern_count,
            ),
            row_kind,
            piece_source_id: Some(piece_source_id),
            rows: Vec::new(),
        }
    }
}
impl TypedCoverageMatrix {
    pub fn with_capacity_limit(
        row_kind: CoverageRowKind,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        max_pattern_count: usize,
    ) -> Result<Self, CoverageMatrixError> {
        Ok(Self {
            guard: CoverageUniverseGuard::with_capacity_limit(
                pattern_universe_id,
                pattern_weight_model_id,
                pattern_count,
                max_pattern_count,
            )?,
            row_kind,
            piece_source_id: None,
            rows: Vec::new(),
        })
    }
}
impl TypedCoverageMatrix {
    pub fn from_rows(
        row_kind: CoverageRowKind,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        pattern_count: usize,
        rows: Vec<TypedCoverageRow>,
    ) -> Result<Self, CoverageMatrixError> {
        let mut matrix = Self::new(
            row_kind,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
        );
        for row in rows {
            matrix.push(row)?;
        }
        Ok(matrix)
    }
}
impl TypedCoverageMatrix {
    pub fn push(&mut self, row: TypedCoverageRow) -> Result<(), CoverageMatrixError> {
        if row.row_kind() != &self.row_kind {
            return Err(CoverageMatrixError::RowKindMismatch {
                expected: self.row_kind.clone(),
                actual: row.row_kind().clone(),
            });
        }
        self.guard.check_row(&row)?;
        self.check_piece_source(&row)?;
        self.rows.push(row);
        Ok(())
    }
}
impl TypedCoverageMatrix {
    pub fn row_kind(&self) -> &CoverageRowKind {
        &self.row_kind
    }
}
impl TypedCoverageMatrix {
    pub fn piece_source_id(&self) -> Option<u64> {
        self.piece_source_id
    }
}
impl TypedCoverageMatrix {
    pub fn pattern_universe_id(&self) -> PatternUniverseId {
        self.guard.pattern_universe_id()
    }
}
impl TypedCoverageMatrix {
    pub fn pattern_weight_model_id(&self) -> PatternWeightModelId {
        self.guard.pattern_weight_model_id()
    }
}
impl TypedCoverageMatrix {
    pub fn pattern_count(&self) -> usize {
        self.guard.pattern_count()
    }
}
impl TypedCoverageMatrix {
    pub fn rows(&self) -> &[TypedCoverageRow] {
        &self.rows
    }
}
impl TypedCoverageMatrix {
    pub fn union_all(&self) -> PatternBitSet {
        let mut union = PatternBitSet::new(self.pattern_count());
        for row in &self.rows {
            union
                .union_with(row.coverage_bits())
                .expect("typed coverage row universe guard invariant");
        }
        union
    }
}
impl TypedCoverageMatrix {
    pub fn union_rows(&self, row_indices: &[usize]) -> Result<PatternBitSet, CoverageMatrixError> {
        let mut union = PatternBitSet::new(self.pattern_count());
        for index in row_indices {
            let row = self
                .rows
                .get(*index)
                .ok_or(CoverageMatrixError::RowIndexOutOfRange {
                    index: *index,
                    row_count: self.rows.len(),
                })?;
            union
                .union_with(row.coverage_bits())
                .map_err(CoverageMatrixError::Pattern)?;
        }
        Ok(union)
    }
}
impl TypedCoverageMatrix {
    fn check_piece_source(&mut self, row: &TypedCoverageRow) -> Result<(), CoverageMatrixError> {
        let actual = row.piece_source_id();
        if actual == 0 {
            return Err(CoverageMatrixError::MissingPieceSourceIdentity);
        }
        match self.piece_source_id {
            Some(expected) if expected != actual => {
                Err(CoverageMatrixError::PieceSourceIdMismatch { expected, actual })
            }
            Some(_) => Ok(()),
            None => {
                self.piece_source_id = Some(actual);
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "coverage_matrix_tests.rs"]
mod tests;
