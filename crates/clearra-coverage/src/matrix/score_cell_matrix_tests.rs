use crate::{
    matrix::coverage_matrix_error::CoverageMatrixError,
    pattern::{
        pattern_bitset::{PatternBitSet, PatternBitSetError},
        pattern_id::PatternId,
    },
    row::{coverage_row_kind::ScoreObjectiveCellId, score_cell_row::ScoreCellRow},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

use super::*;

#[test]
fn score_cell_matrix_reports_capacity_exceeded() {
    let score_cell_id = ScoreObjectiveCellId::new("b2b-tspin");
    let universe = PatternUniverseId::new(11);
    let weight_model = PatternWeightModelId::new(22);
    let mut matrix = ScoreCellMatrix::with_memory_budget(
        score_cell_id.clone(),
        universe,
        weight_model,
        64,
        ScoreCellMatrixBudget::new(1, 1),
    )
    .expect("budgeted matrix");

    matrix
        .push(ScoreCellRow::new(
            1,
            11,
            score_cell_id.clone(),
            universe,
            weight_model,
            PatternBitSet::from_patterns(64, [PatternId::new(0)]).expect("row"),
        ))
        .expect("first row fits");

    assert_eq!(
        matrix.push(ScoreCellRow::new(
            2,
            11,
            score_cell_id,
            universe,
            weight_model,
            PatternBitSet::from_patterns(64, [PatternId::new(1)]).expect("row"),
        )),
        Err(CoverageMatrixError::ScoreCellCapacityExceeded {
            row_count: 2,
            row_limit: 1
        })
    );
}

#[test]
fn score_cell_matrix_memory_budget_rejects_word_overflow() {
    let result = ScoreCellMatrix::with_memory_budget(
        ScoreObjectiveCellId::new("b2b-tspin"),
        PatternUniverseId::new(11),
        PatternWeightModelId::new(22),
        129,
        ScoreCellMatrixBudget::new(8, 2),
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
fn score_cell_row_requires_piece_source_id() {
    let score_cell_id = ScoreObjectiveCellId::new("b2b-tspin");
    let universe = PatternUniverseId::new(11);
    let weight_model = PatternWeightModelId::new(22);
    let mut matrix = ScoreCellMatrix::new(score_cell_id.clone(), universe, weight_model, 4);

    let result = matrix.push(ScoreCellRow::new(
        1,
        0,
        score_cell_id,
        universe,
        weight_model,
        PatternBitSet::from_patterns(4, [PatternId::new(0)]).expect("row"),
    ));

    assert_eq!(result, Err(CoverageMatrixError::MissingPieceSourceIdentity));
}
