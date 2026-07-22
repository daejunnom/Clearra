use crate::{
    cover::cover_selection::{
        CoverSelectionLimit, CoverSelectionOptimality, CoverSelectionStrategy,
    },
    matrix::coverage_matrix::TypedCoverageMatrix,
    pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

use super::*;

#[test]
fn exact_solver_finds_two_row_minimum_cover() {
    let required = PatternBitSet::from_patterns(
        4,
        [
            PatternId::new(0),
            PatternId::new(1),
            PatternId::new(2),
            PatternId::new(3),
        ],
    )
    .expect("required");
    let matrix = TypedCoverageMatrix::from_rows(
        CoverageRowKind::Pc,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        4,
        vec![
            CoverageRow::new_with_piece_source(
                0,
                CoverageRowKind::Pc,
                11,
                PatternUniverseId::new(1),
                PatternWeightModelId::new(7),
                PatternBitSet::from_patterns(4, [PatternId::new(0)]).expect("row 0"),
            ),
            CoverageRow::new_with_piece_source(
                1,
                CoverageRowKind::Pc,
                11,
                PatternUniverseId::new(1),
                PatternWeightModelId::new(7),
                PatternBitSet::from_patterns(
                    4,
                    [PatternId::new(0), PatternId::new(1), PatternId::new(2)],
                )
                .expect("row 1"),
            ),
            CoverageRow::new_with_piece_source(
                2,
                CoverageRowKind::Pc,
                11,
                PatternUniverseId::new(1),
                PatternWeightModelId::new(7),
                PatternBitSet::from_patterns(4, [PatternId::new(3)]).expect("row 2"),
            ),
        ],
    )
    .expect("matrix");

    let selection = MinimumCoverSolver::solve(&matrix, &required);

    assert!(selection.is_complete());
    assert!(selection.is_proven_minimum());
    assert_eq!(selection.strategy(), CoverSelectionStrategy::ExactSearch);
    assert_eq!(
        selection.optimality(),
        CoverSelectionOptimality::ProvenMinimum
    );
    assert_eq!(selection.limit(), CoverSelectionLimit::None);
    assert_eq!(selection.row_indices(), &[1, 2]);
}

#[test]
fn incomplete_cover_reports_partial_selection() {
    let required =
        PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)]).expect("required");
    let matrix = TypedCoverageMatrix::from_rows(
        CoverageRowKind::Pc,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        2,
        vec![CoverageRow::new_with_piece_source(
            0,
            CoverageRowKind::Pc,
            11,
            PatternUniverseId::new(1),
            PatternWeightModelId::new(7),
            PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("row"),
        )],
    )
    .expect("matrix");

    let selection = MinimumCoverSolver::solve(&matrix, &required);

    assert!(!selection.is_complete());
    assert_eq!(selection.strategy(), CoverSelectionStrategy::ExactSearch);
    assert_eq!(
        selection.optimality(),
        CoverSelectionOptimality::NoCompleteCover
    );
    assert_eq!(selection.limit(), CoverSelectionLimit::None);
}

#[test]
fn greedy_fallback_reports_approximate_budget_limited_result() {
    let required = PatternBitSet::from_patterns(1, [PatternId::new(0)]).expect("required pattern");
    let rows = (0..=EXACT_MIN_COVER_ROW_LIMIT)
        .map(|index| {
            CoverageRow::new_with_piece_source(
                index as u64,
                CoverageRowKind::Pc,
                11,
                PatternUniverseId::new(1),
                PatternWeightModelId::new(7),
                PatternBitSet::from_patterns(1, [PatternId::new(0)]).expect("row coverage"),
            )
        })
        .collect::<Vec<_>>();
    let matrix = TypedCoverageMatrix::from_rows(
        CoverageRowKind::Pc,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        1,
        rows,
    )
    .expect("matrix");

    let selection = MinimumCoverSolver::solve(&matrix, &required);

    assert!(selection.is_complete());
    assert!(!selection.is_proven_minimum());
    assert!(selection.used_greedy_fallback());
    assert!(selection.exceeded_exact_search_budget());
    assert_eq!(selection.strategy(), CoverSelectionStrategy::GreedyFallback);
    assert_eq!(
        selection.optimality(),
        CoverSelectionOptimality::Approximate
    );
    assert_eq!(
        selection.limit(),
        CoverSelectionLimit::ExactSearchRowLimitExceeded {
            row_count: EXACT_MIN_COVER_ROW_LIMIT + 1,
            limit: EXACT_MIN_COVER_ROW_LIMIT
        }
    );
}
