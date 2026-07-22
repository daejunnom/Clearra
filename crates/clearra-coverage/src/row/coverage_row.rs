use crate::{
    pattern::{pattern_bitset::PatternBitSet, pattern_coverage_bitset::PatternCoverageBitSet},
    row::coverage_row_kind::CoverageRowKind,
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverageRow {
    candidate_id: u64,
    row_kind: CoverageRowKind,
    piece_source_id: u64,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    coverage_bits: PatternCoverageBitSet,
}

impl CoverageRow {
    #[cfg(test)]
    pub fn new_without_piece_source_for_test(
        candidate_id: u64,
        row_kind: CoverageRowKind,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        coverage_bits: PatternBitSet,
    ) -> Self {
        Self::new_with_piece_source(
            candidate_id,
            row_kind,
            0,
            pattern_universe_id,
            pattern_weight_model_id,
            coverage_bits,
        )
    }
}
impl CoverageRow {
    pub fn new_with_piece_source(
        candidate_id: u64,
        row_kind: CoverageRowKind,
        piece_source_id: u64,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        coverage_bits: PatternBitSet,
    ) -> Self {
        Self {
            candidate_id,
            row_kind,
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
            coverage_bits: PatternCoverageBitSet::from(coverage_bits),
        }
    }
}
impl CoverageRow {
    pub fn candidate_id(&self) -> u64 {
        self.candidate_id
    }
}
impl CoverageRow {
    pub fn row_kind(&self) -> &CoverageRowKind {
        &self.row_kind
    }
}
impl CoverageRow {
    pub fn piece_source_id(&self) -> u64 {
        self.piece_source_id
    }
}
impl CoverageRow {
    pub fn pattern_universe_id(&self) -> PatternUniverseId {
        self.pattern_universe_id
    }
}
impl CoverageRow {
    pub fn pattern_weight_model_id(&self) -> PatternWeightModelId {
        self.pattern_weight_model_id
    }
}
impl CoverageRow {
    pub fn coverage_bits(&self) -> &PatternBitSet {
        self.coverage_bits.as_pattern_bitset()
    }
}
impl CoverageRow {
    pub fn pattern_coverage_bits(&self) -> &PatternCoverageBitSet {
        &self.coverage_bits
    }
}
impl CoverageRow {
    pub fn pattern_count(&self) -> usize {
        self.coverage_bits.as_pattern_bitset().pattern_count()
    }
}
