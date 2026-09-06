use crate::{
    cover::cover_selection::{
        CoverSelectionLimit, CoverSelectionOptimality, CoverSelectionStrategy,
    },
    cover::exact_minimum_cover::exact_minimum_cover,
    matrix::{
        coverage_matrix::{CoverageMatrix, TypedCoverageMatrix},
        coverage_row::CoverageRow as MatrixCoverageRow,
    },
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

#[test]
fn unbounded_exact_matrix_solver_is_fieldwise_equal_to_the_existing_exact_authority() {
    let required =
        PatternBitSet::from_patterns(4, (0..4).map(PatternId::new)).expect("required patterns");
    let rows = vec![
        PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(1)]).expect("first row"),
        PatternBitSet::from_patterns(4, [PatternId::new(2), PatternId::new(3)])
            .expect("second row"),
        PatternBitSet::from_patterns(4, [PatternId::new(0), PatternId::new(2)]).expect("third row"),
        PatternBitSet::from_patterns(4, [PatternId::new(1), PatternId::new(3)])
            .expect("fourth row"),
    ];
    let matrix = CoverageMatrix::from_rows(
        4,
        rows.iter()
            .cloned()
            .enumerate()
            .map(|(candidate_id, patterns)| MatrixCoverageRow::new(candidate_id, patterns))
            .collect(),
    )
    .expect("coverage matrix");

    let expected = exact_minimum_cover(&required, &rows).expect("existing exact authority");
    let actual =
        MinimumCoverSolver::solve_exact(&matrix, &required).expect("matrix exact authority");

    assert_eq!(actual.row_indices(), expected.row_indices());
    assert_eq!(actual.covered_patterns(), expected.covered_patterns());
    assert_eq!(actual.is_complete(), expected.complete());
    assert_eq!(actual.strategy(), CoverSelectionStrategy::ExactSearch);
    assert_eq!(actual.optimality(), CoverSelectionOptimality::ProvenMinimum);
    assert_eq!(actual.limit(), CoverSelectionLimit::None);
}

#[test]
fn guarded_canonical_matrix_solver_uses_original_row_lex_first_identity() {
    let required =
        PatternBitSet::from_patterns(3, (0..3).map(PatternId::new)).expect("required patterns");
    let rows = vec![
        PatternBitSet::from_patterns(3, [PatternId::new(1), PatternId::new(2)]).expect("first row"),
        PatternBitSet::from_patterns(3, [PatternId::new(0)])
            .expect("properly dominated second row"),
        PatternBitSet::from_patterns(3, [PatternId::new(0), PatternId::new(1)]).expect("third row"),
    ];
    let matrix = CoverageMatrix::from_rows(
        3,
        rows.into_iter()
            .enumerate()
            .map(|(candidate_id, patterns)| MatrixCoverageRow::new(candidate_id, patterns))
            .collect(),
    )
    .expect("coverage matrix");

    let selection = MinimumCoverSolver::solve_exact_canonical_with_memory_guard(
        &matrix,
        &required,
        &mut |_| Ok(()),
    )
    .expect("canonical exact cover");

    // The proof may discard row 1 as dominated by row 2, but [0, 1] is the
    // lexicographically first minimum portfolio over the original matrix.
    assert_eq!(selection.row_indices(), [0, 1]);
    assert!(selection.is_complete());
    assert!(selection.is_proven_minimum());
}

#[test]
fn unbounded_exact_matrix_solver_preserves_partial_authority_when_no_full_cover_exists() {
    let required =
        PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)]).expect("required");
    let row = PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("partial row");
    let matrix = CoverageMatrix::from_rows(2, vec![MatrixCoverageRow::new(7, row.clone())])
        .expect("coverage matrix");
    let expected = exact_minimum_cover(&required, &[row]).expect("existing exact authority");

    let actual =
        MinimumCoverSolver::solve_exact(&matrix, &required).expect("matrix exact authority");

    assert_eq!(actual.row_indices(), expected.row_indices());
    assert_eq!(actual.covered_patterns(), expected.covered_patterns());
    assert!(!actual.is_complete());
    assert_eq!(actual.strategy(), CoverSelectionStrategy::ExactSearch);
    assert_eq!(
        actual.optimality(),
        CoverSelectionOptimality::NoCompleteCover
    );
    assert_eq!(actual.limit(), CoverSelectionLimit::None);
}

#[test]
fn typed_portfolio_adapter_preserves_equal_original_rows() {
    let required = PatternBitSet::from_patterns(1, [PatternId::new(0)]).expect("required pattern");
    let rows = [7_u64, 3_u64]
        .into_iter()
        .map(|candidate_id| {
            CoverageRow::new_with_piece_source(
                candidate_id,
                CoverageRowKind::Pc,
                11,
                PatternUniverseId::new(1),
                PatternWeightModelId::new(7),
                required.clone(),
            )
        })
        .collect();
    let matrix = TypedCoverageMatrix::from_rows(
        CoverageRowKind::Pc,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        1,
        rows,
    )
    .expect("matrix");
    let mut portfolios =
        MinimumCoverSolver::exact_typed_portfolios(&matrix, &required).expect("portfolios");

    let page = portfolios.next_page(10, 10).expect("all portfolios");

    assert_eq!(
        page.portfolios()
            .iter()
            .map(|portfolio| portfolio.row_indices().to_vec())
            .collect::<Vec<_>>(),
        vec![vec![0], vec![1]]
    );
    assert_eq!(page.total_alternative_count_decimal(), Some("2"));
}
