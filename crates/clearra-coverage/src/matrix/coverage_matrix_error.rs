use crate::{
    pattern::pattern_bitset::PatternBitSetError,
    row::coverage_row_kind::CoverageRowKind,
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoverageMatrixError {
    MissingPieceSourceIdentity,
    MissingPatternUniverseIdentity,
    MissingPatternWeightModelIdentity,
    RowPatternCountMismatch {
        expected: usize,
        actual: usize,
    },
    RowIndexOutOfRange {
        index: usize,
        row_count: usize,
    },
    PatternUniverseIdMismatch {
        expected: PatternUniverseId,
        actual: PatternUniverseId,
    },
    PatternWeightModelIdMismatch {
        expected: PatternWeightModelId,
        actual: PatternWeightModelId,
    },
    PieceSourceIdMismatch {
        expected: u64,
        actual: u64,
    },
    RowKindMismatch {
        expected: CoverageRowKind,
        actual: CoverageRowKind,
    },
    PatternBitSetCapacityExceeded {
        pattern_count: usize,
        max_pattern_count: usize,
    },
    SpinCoverageCapacityExceeded {
        row_count: usize,
        row_limit: usize,
    },
    ScoreCellCapacityExceeded {
        row_count: usize,
        row_limit: usize,
    },
    Pattern(PatternBitSetError),
}
