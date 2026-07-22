use crate::{
    pattern::pattern_bitset::PatternBitSet,
    row::{
        coverage_row::CoverageRow,
        coverage_row_kind::{CoverageRowKind, ScoreObjectiveCellId},
    },
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreCellRow {
    row: CoverageRow,
}

impl ScoreCellRow {
    pub fn new(
        candidate_id: u64,
        piece_source_id: u64,
        score_cell_id: ScoreObjectiveCellId,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        coverage_bits: PatternBitSet,
    ) -> Self {
        Self {
            row: CoverageRow::new_with_piece_source(
                candidate_id,
                CoverageRowKind::ScoreCell(score_cell_id),
                piece_source_id,
                pattern_universe_id,
                pattern_weight_model_id,
                coverage_bits,
            ),
        }
    }
}
impl ScoreCellRow {
    pub fn row(&self) -> &CoverageRow {
        &self.row
    }
}
impl ScoreCellRow {
    pub fn into_row(self) -> CoverageRow {
        self.row
    }
}
