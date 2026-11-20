use clearra_core_domain::{
    board::cell::CellCoord, piece::piece_kind::PieceKind,
    probability::probability_value::ProbabilityValue,
};
use clearra_coverage::pattern::pattern_id::PatternId;

use crate::{
    coverage::BuildCoverageExecution,
    domain::slot_domain::SlotDomain,
    query::build_coverage_limits::BuildCoverageLimits,
    template::{BuildSlot, BuildTemplate},
};

use super::*;

const PATTERN_COUNT: usize = 3;

fn board() -> BoardSize {
    BoardSize::new(4, 4).expect("bounded test board")
}

fn scope() -> ConcreteCoverageScope {
    ConcreteCoverageScope::new_uniform(
        board(),
        3,
        PatternUniverseId::new(5),
        PatternWeightModelId::new(7),
        11,
    )
}

fn identity(piece: PieceKind, masks: &[u64]) -> StandardBoard64TilingIdentity {
    StandardBoard64TilingIdentity::from_placements(
        0,
        masks
            .iter()
            .copied()
            .map(|mask| PiecePlacementMask::new(piece, mask)),
    )
    .expect("concrete evidence identity")
}

fn query(masks: &[u64], pieces: &[PieceKind], max_assignments: usize) -> BuildCoverageQuery {
    let mut slots = Vec::new();
    let mut domains = Vec::new();
    for (index, mask) in masks.iter().copied().enumerate() {
        let slot = BuildSlotId::new(u32::try_from(index).expect("fixture slot fits u32"));
        let cells = (0..16_u16)
            .filter(|&bit| mask & (1_u64 << bit) != 0)
            .map(|bit| CellCoord::new_unchecked(bit % 4, bit / 4))
            .collect();
        slots.push(BuildSlot::new(slot, cells).with_allowed_pieces(pieces.to_vec()));
        domains.push(SlotDomain::new(slot, pieces.to_vec()));
    }
    BuildCoverageQuery::new(
        BuildTemplate::new("verified-concrete-coverage", slots).with_board_size(board()),
        domains,
        Vec::new(),
        PATTERN_COUNT,
        BuildCoverageLimits::new(max_assignments, PATTERN_COUNT),
    )
}

fn row(
    identity: StandardBoard64TilingIdentity,
    ordinals: &[usize],
) -> ConcreteAssignmentCoverageRow {
    ConcreteAssignmentCoverageRow::new(
        identity,
        scope(),
        PatternBitSet::from_patterns(PATTERN_COUNT, ordinals.iter().copied().map(PatternId::new))
            .expect("original ordinal bitset"),
        ConcreteCoverageCompleteness::Complete,
    )
}

#[test]
fn verified_assignment_rows_join_by_exact_identity_and_preserve_explicit_empty_rows() {
    let masks = [0xf];
    let query = query(&masks, &[PieceKind::I, PieceKind::O], 16);
    let positive = row(identity(PieceKind::I, &masks), &[0, 2]);
    let exact_empty = row(identity(PieceKind::O, &masks), &[]);
    let direct = BuildCoverageExecution::from_verified_concrete_assignment_rows(
        &query,
        scope(),
        vec![positive.clone(), exact_empty.clone()],
    )
    .expect("explicit row for every solved assignment");
    let shuffled = BuildCoverageExecution::from_verified_concrete_assignment_rows(
        &query,
        scope(),
        vec![exact_empty, positive],
    )
    .expect("provider row order is not an association");
    assert_eq!(direct, shuffled);
    assert!(shuffled.exact_cover_complete());
    assert_eq!(shuffled.c_coverage_row_count(), 0);
    assert_eq!(shuffled.matrix().matrix().rows().len(), 2);
    assert_eq!(shuffled.union().covered_patterns().words(), &[0b101]);
    assert_eq!(
        shuffled.result().probability(),
        ProbabilityValue::new(2.0 / 3.0).expect("uniform two of three"),
    );
    for (index, assignment) in shuffled.assignments().iter().enumerate() {
        let expected = if assignment.assigned_slots()[0].piece() == PieceKind::I {
            0b101
        } else {
            0
        };
        assert_eq!(
            shuffled.matrix().matrix().rows()[index]
                .coverage_bits()
                .word_at(0),
            expected,
        );
    }
}

#[test]
fn verified_assignment_rows_reject_a_different_concrete_split_of_the_same_color_set() {
    let horizontal = [0x000f, 0x00f0, 0x0f00, 0xf000];
    let vertical = [0x1111, 0x2222, 0x4444, 0x8888];
    assert_eq!(
        horizontal.into_iter().reduce(|left, right| left | right),
        vertical.into_iter().reduce(|left, right| left | right)
    );
    let query = query(&horizontal, &[PieceKind::I], 16);
    let horizontal_identity = identity(PieceKind::I, &horizontal);
    let vertical_identity = identity(PieceKind::I, &vertical);
    assert_ne!(horizontal_identity, vertical_identity);
    BuildCoverageExecution::from_verified_concrete_assignment_rows(
        &query,
        scope(),
        vec![row(horizontal_identity, &[1])],
    )
    .expect("only the supplied concrete split belongs to this template");
    assert_eq!(
        BuildCoverageExecution::from_verified_concrete_assignment_rows(
            &query,
            scope(),
            vec![row(vertical_identity, &[1])],
        ),
        Err(Error::ForeignConcreteCoverageRow { row: 0 })
    );
}

#[test]
fn verified_assignment_rows_fail_closed_on_missing_duplicate_or_incomplete_rows() {
    let masks = [0xf];
    let query = query(&masks, &[PieceKind::I, PieceKind::O], 16);
    let positive = row(identity(PieceKind::I, &masks), &[0]);
    let mut incomplete = row(identity(PieceKind::O, &masks), &[]);
    incomplete.completeness = ConcreteCoverageCompleteness::Incomplete;
    assert!(matches!(
        BuildCoverageExecution::from_verified_concrete_assignment_rows(
            &query,
            scope(),
            vec![positive.clone()],
        ),
        Err(Error::MissingConcreteCoverageRow { .. })
    ));
    assert!(matches!(
        BuildCoverageExecution::from_verified_concrete_assignment_rows(
            &query,
            scope(),
            vec![positive.clone(), positive.clone()],
        ),
        Err(Error::DuplicateConcreteCoverageRow { .. })
    ));
    assert_eq!(
        BuildCoverageExecution::from_verified_concrete_assignment_rows(
            &query,
            scope(),
            vec![positive, incomplete],
        ),
        Err(Error::IncompleteConcreteCoverageRow { row: 1 })
    );
}

#[test]
fn verified_assignment_rows_require_independent_scope_and_original_denominator() {
    let masks = [0xf];
    let query = query(&masks, &[PieceKind::I], 16);
    let mut wrong_scope = row(identity(PieceKind::I, &masks), &[0]);
    let matching = wrong_scope.clone();
    wrong_scope.scope.universe = PatternUniverseId::new(91);
    let mut wrong_execution = matching.clone();
    wrong_execution.scope.execution_scope = 101;
    let mut wrong_denominator = matching.clone();
    wrong_denominator.covered_patterns = PatternBitSet::new(PATTERN_COUNT + 1);
    let mut wrong_frame = scope();
    wrong_frame.board = BoardSize::new(8, 2).expect("foreign row frame with same area");
    assert_eq!(
        BuildCoverageExecution::from_verified_concrete_assignment_rows(
            &query,
            scope(),
            vec![wrong_scope],
        ),
        Err(Error::ConcreteCoverageScopeMismatch { row: 0 })
    );
    assert_eq!(
        BuildCoverageExecution::from_verified_concrete_assignment_rows(
            &query,
            scope(),
            vec![wrong_denominator],
        ),
        Err(Error::CoveragePatternCountMismatch {
            expected: PATTERN_COUNT,
            actual: PATTERN_COUNT + 1
        })
    );
    assert_eq!(
        BuildCoverageExecution::from_verified_concrete_assignment_rows(
            &query,
            scope(),
            vec![wrong_execution],
        ),
        Err(Error::ConcreteCoverageScopeMismatch { row: 0 })
    );
    assert_eq!(
        BuildCoverageExecution::from_verified_concrete_assignment_rows(
            &query,
            wrong_frame,
            vec![matching],
        ),
        Err(Error::InvalidConcreteCoverageScope)
    );
}

#[test]
fn verified_assignment_rows_never_reduce_a_truncated_assignment_search() {
    let masks = [0xf];
    let query = query(&masks, &[PieceKind::I, PieceKind::O], 1);
    assert_eq!(
        BuildCoverageExecution::from_verified_concrete_assignment_rows(
            &query,
            scope(),
            vec![row(identity(PieceKind::I, &masks), &[0])],
        ),
        Err(Error::ConcreteAssignmentSearchIncomplete)
    );
}
