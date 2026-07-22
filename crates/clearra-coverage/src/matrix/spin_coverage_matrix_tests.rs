use crate::{
    matrix::coverage_matrix_error::CoverageMatrixError,
    pattern::{
        pattern_bitset::{PatternBitSet, PatternBitSetError},
        pattern_id::PatternId,
    },
    row::spin_coverage_row::SpinCoverageRow,
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

use super::*;

#[test]
fn spin_coverage_matrix_respects_memory_budget() {
    let spin_target_id = SpinTargetId::new("tsd");
    let universe = PatternUniverseId::new(1);
    let weight_model = PatternWeightModelId::new(2);
    let mut matrix = SpinCoverageMatrix::with_memory_budget(
        spin_target_id.clone(),
        universe,
        weight_model,
        64,
        SpinCoverageMatrixBudget::new(1, 1),
    )
    .expect("budgeted matrix");

    matrix
        .push(SpinCoverageRow::new(
            1,
            11,
            spin_target_id.clone(),
            universe,
            weight_model,
            PatternBitSet::from_patterns(64, [PatternId::new(0)]).expect("row"),
        ))
        .expect("first row fits");

    assert_eq!(
        matrix.push(SpinCoverageRow::new(
            2,
            11,
            spin_target_id,
            universe,
            weight_model,
            PatternBitSet::from_patterns(64, [PatternId::new(1)]).expect("row"),
        )),
        Err(CoverageMatrixError::SpinCoverageCapacityExceeded {
            row_count: 2,
            row_limit: 1
        })
    );
}

#[test]
fn spin_coverage_matrix_memory_budget_rejects_word_overflow() {
    let result = SpinCoverageMatrix::with_memory_budget(
        SpinTargetId::new("tsd"),
        PatternUniverseId::new(1),
        PatternWeightModelId::new(2),
        129,
        SpinCoverageMatrixBudget::new(8, 2),
    );

    assert_eq!(
        result,
        Err(CoverageMatrixError::Pattern(
            PatternBitSetError::WordCapacityExceeded {
                word_count: 3,
                word_limit: 2
            }
        ))
    );
}

#[test]
fn spin_coverage_row_requires_piece_source_id() {
    let spin_target_id = SpinTargetId::new("tsd");
    let universe = PatternUniverseId::new(1);
    let weight_model = PatternWeightModelId::new(2);
    let mut matrix = SpinCoverageMatrix::new(spin_target_id.clone(), universe, weight_model, 4);

    let result = matrix.push(SpinCoverageRow::new(
        1,
        0,
        spin_target_id,
        universe,
        weight_model,
        PatternBitSet::from_patterns(4, [PatternId::new(0)]).expect("row"),
    ));

    assert_eq!(result, Err(CoverageMatrixError::MissingPieceSourceIdentity));
}
