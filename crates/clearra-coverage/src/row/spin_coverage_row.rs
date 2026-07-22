use clearra_core_domain::ids::SpinTargetId;

use crate::{
    pattern::pattern_bitset::PatternBitSet,
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinCoverageRow {
    row: CoverageRow,
}

impl SpinCoverageRow {
    pub fn new(
        candidate_id: u64,
        piece_source_id: u64,
        spin_target_id: SpinTargetId,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        coverage_bits: PatternBitSet,
    ) -> Self {
        Self {
            row: CoverageRow::new_with_piece_source(
                candidate_id,
                CoverageRowKind::SpinTarget(spin_target_id),
                piece_source_id,
                pattern_universe_id,
                pattern_weight_model_id,
                coverage_bits,
            ),
        }
    }
}
impl SpinCoverageRow {
    pub fn row(&self) -> &CoverageRow {
        &self.row
    }
}
impl SpinCoverageRow {
    pub fn into_row(self) -> CoverageRow {
        self.row
    }
}
