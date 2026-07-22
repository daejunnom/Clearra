use clearra_core_domain::{
    board::{board_size::BoardSize, cell::CellCoord},
    piece::piece_kind::PieceKind,
};
use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};

use crate::{
    domain::slot_domain::SlotDomain,
    query::{build_coverage_limits::BuildCoverageLimits, build_coverage_query::BuildCoverageQuery},
    template::{BuildSlot, BuildSlotId, BuildTemplate},
};

use super::*;

#[test]
fn build_coverage_executor_uses_c_buildup_rows_and_union_probability() {
    let query = one_slot_query(2);
    let c_rows = vec![CoverageRow::new_with_piece_source(
        0,
        CoverageRowKind::Build,
        11,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("coverage"),
    )];

    let execution =
        BuildCoverageExecution::from_c_buildup_rows(&query, &c_rows).expect("execution");

    assert_eq!(execution.assignments().len(), 1);
    assert!(execution.exact_cover_complete());
    assert_eq!(execution.c_coverage_row_count(), 1);
    assert_eq!(execution.matrix().matrix().rows().len(), 1);
    assert_eq!(execution.union().covered_patterns().count_ones(), 1);
    assert_eq!(execution.result().probability().get(), 0.5);
}

#[test]
fn c_buildup_coverage_row_generated_false_for_empty_rows() {
    let query = one_slot_query(2);

    let execution =
        BuildCoverageExecution::from_c_buildup_rows(&query, &[]).expect("zero coverage");

    assert_eq!(execution.assignments().len(), 1);
    assert_eq!(execution.c_coverage_row_count(), 0);
    assert_eq!(execution.matrix().matrix().rows().len(), 0);
    assert_eq!(execution.union().covered_patterns().count_ones(), 0);
    assert_eq!(execution.result().probability().get(), 0.0);
}

#[test]
fn build_coverage_executor_rejects_c_row_assignment_count_mismatch() {
    let query = one_slot_query_with_domain(2, vec![PieceKind::I, PieceKind::O]);
    let c_rows = vec![CoverageRow::new_with_piece_source(
        0,
        CoverageRowKind::Build,
        11,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("coverage"),
    )];

    let result = BuildCoverageExecution::from_c_buildup_rows(&query, &c_rows);

    assert_eq!(
        result,
        Err(
            BuildCoverageExecutionError::CoverageRowAssignmentCountMismatch {
                assignments: 2,
                c_coverage_rows: 1
            }
        )
    );
}

#[test]
fn build_coverage_executor_rejects_c_row_pattern_universe_mismatch() {
    let query = one_slot_query(2);
    let c_rows = vec![CoverageRow::new_with_piece_source(
        0,
        CoverageRowKind::Build,
        11,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
        PatternBitSet::from_patterns(3, [PatternId::new(0)]).expect("coverage"),
    )];

    let result = BuildCoverageExecution::from_c_buildup_rows(&query, &c_rows);

    assert_eq!(
        result,
        Err(BuildCoverageExecutionError::CoveragePatternCountMismatch {
            expected: 2,
            actual: 3
        })
    );
}

#[test]
fn coverage_row_universe_mismatch_rejected() {
    let query = one_slot_query_with_domain(2, vec![PieceKind::I, PieceKind::O]);
    let c_rows = vec![
        CoverageRow::new_with_piece_source(
            0,
            CoverageRowKind::Build,
            11,
            PatternUniverseId::new(1),
            PatternWeightModelId::new(7),
            PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("coverage 0"),
        ),
        CoverageRow::new_with_piece_source(
            1,
            CoverageRowKind::Build,
            11,
            PatternUniverseId::new(2),
            PatternWeightModelId::new(7),
            PatternBitSet::from_patterns(2, [PatternId::new(1)]).expect("coverage 1"),
        ),
    ];

    let result = BuildCoverageExecution::from_c_buildup_rows(&query, &c_rows);

    assert_eq!(
        result,
        Err(BuildCoverageExecutionError::CoverageUniverseMismatch {
            expected: PatternUniverseId::new(1),
            actual: PatternUniverseId::new(2)
        })
    );
}

fn one_slot_query(pattern_count: usize) -> BuildCoverageQuery {
    one_slot_query_with_domain(pattern_count, vec![PieceKind::I])
}

fn one_slot_query_with_domain(pattern_count: usize, pieces: Vec<PieceKind>) -> BuildCoverageQuery {
    let slot = BuildSlotId::new(1);
    BuildCoverageQuery::new(
        BuildTemplate::new(
            "single-slot",
            vec![BuildSlot::new(
                slot,
                vec![CellCoord::new(0, 0, BoardSize::new(10, 4).expect("board")).expect("cell")],
            )],
        )
        .with_board_size(BoardSize::new(10, 4).expect("board")),
        vec![SlotDomain::new(slot, pieces)],
        Vec::new(),
        pattern_count,
        BuildCoverageLimits::new(16, pattern_count),
    )
}
