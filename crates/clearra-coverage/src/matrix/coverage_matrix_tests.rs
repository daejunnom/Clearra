use crate::{
    matrix::{coverage_matrix::CoverageMatrixError, coverage_row::CoverageRow},
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
};

use super::*;

#[test]
fn union_rows_rejects_out_of_range_row_index() {
    let matrix = CoverageMatrix::from_rows(
        2,
        vec![CoverageRow::new(
            0,
            PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("row"),
        )],
    )
    .expect("matrix");

    let result = matrix.union_rows(&[0, 1]);

    assert_eq!(
        result,
        Err(CoverageMatrixError::RowIndexOutOfRange {
            index: 1,
            row_count: 1
        })
    );
}

#[test]
fn union_rows_uses_requested_rows_when_indices_exist() {
    let matrix = CoverageMatrix::from_rows(
        3,
        vec![
            CoverageRow::new(
                0,
                PatternBitSet::from_patterns(3, [PatternId::new(0)]).expect("row 0"),
            ),
            CoverageRow::new(
                1,
                PatternBitSet::from_patterns(3, [PatternId::new(2)]).expect("row 1"),
            ),
        ],
    )
    .expect("matrix");

    let union = matrix.union_rows(&[0, 1]).expect("union rows");

    assert!(union.contains(PatternId::new(0)));
    assert!(!union.contains(PatternId::new(1)));
    assert!(union.contains(PatternId::new(2)));
}

#[test]
fn typed_coverage_matrix_rejects_zero_identity_when_universe_required() {
    let mut missing_universe = TypedCoverageMatrix::new(
        CoverageRowKind::Build,
        PatternUniverseId::new(0),
        PatternWeightModelId::new(7),
        4,
    );
    let row = TypedCoverageRow::new_without_piece_source_for_test(
        1,
        CoverageRowKind::Build,
        PatternUniverseId::new(0),
        PatternWeightModelId::new(7),
        PatternBitSet::from_patterns(4, [PatternId::new(0)]).expect("row"),
    );

    assert_eq!(
        missing_universe.push(row),
        Err(CoverageMatrixError::MissingPatternUniverseIdentity)
    );

    let mut missing_weight = TypedCoverageMatrix::new(
        CoverageRowKind::Build,
        PatternUniverseId::new(11),
        PatternWeightModelId::new(0),
        4,
    );
    let row = TypedCoverageRow::new_without_piece_source_for_test(
        1,
        CoverageRowKind::Build,
        PatternUniverseId::new(11),
        PatternWeightModelId::new(0),
        PatternBitSet::from_patterns(4, [PatternId::new(0)]).expect("row"),
    );

    assert_eq!(
        missing_weight.push(row),
        Err(CoverageMatrixError::MissingPatternWeightModelIdentity)
    );
}

#[test]
fn product_coverage_row_requires_piece_source_id() {
    let mut matrix = TypedCoverageMatrix::new(
        CoverageRowKind::Build,
        PatternUniverseId::new(11),
        PatternWeightModelId::new(7),
        4,
    );
    let row = TypedCoverageRow::new_without_piece_source_for_test(
        1,
        CoverageRowKind::Build,
        PatternUniverseId::new(11),
        PatternWeightModelId::new(7),
        PatternBitSet::from_patterns(4, [PatternId::new(0)]).expect("row"),
    );

    assert_eq!(
        matrix.push(row),
        Err(CoverageMatrixError::MissingPieceSourceIdentity)
    );
}

#[test]
fn identityless_coverage_row_constructor_is_test_only() {
    let row = TypedCoverageRow::new_without_piece_source_for_test(
        1,
        CoverageRowKind::Build,
        PatternUniverseId::new(11),
        PatternWeightModelId::new(7),
        PatternBitSet::from_patterns(4, [PatternId::new(0)]).expect("row"),
    );

    assert_eq!(row.piece_source_id(), 0);
}
