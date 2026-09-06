use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_replay::{RotationRequest, ScoringExecutionEdge};
use clearra_scoring::{event::spin_detector::SpinDetector, event::spin_event::SpinEvent};

use crate::{
    board::StructureBoard,
    corner::{self, CornerEvidence},
    entry::EntryLock,
    fill,
    model::{SpinStructureQuery, StructurePlacement},
};

// Classification mirrors the physical and logical lock evidence surfaces explicitly.
#[allow(clippy::too_many_arguments)]
pub(crate) fn classify_lock(
    query: &SpinStructureQuery,
    board_before: StructureBoard,
    board_after: StructureBoard,
    piece: PieceKind,
    lock: EntryLock,
    _physical_cleared_rows: u32,
    logical_cleared_rows: u32,
    cleared_lines: u8,
) -> Option<SpinEvent> {
    if query.mode.t_only() && piece != PieceKind::T {
        return None;
    }
    if !fill::accepts(query, logical_cleared_rows, cleared_lines) {
        return None;
    }
    let corners = corner::evidence(board_before, piece, lock, query.height);
    if !corner::can_classify(query.mode, piece, lock, corners) {
        return None;
    }
    if piece == PieceKind::T {
        if corners.blocked >= 3 {
            let privileged_vertical_fifth_test =
                matches!(lock.rotation, RotationState::Right | RotationState::Left)
                    && lock.evidence.first_success_confirmed()
                    && lock.evidence.kick_index() == 4
                    && matches!(
                        lock.evidence.rotation_request(),
                        RotationRequest::Clockwise | RotationRequest::CounterClockwise
                    );
            return Some(SpinEvent::new(
                'T',
                corners.front < 2 && !privileged_vertical_fifth_test,
                cleared_lines,
            ));
        }

        // Plus profiles admit the exact immobile fallback only as Mini.  The
        // ordinary three-corner branch above remains Regular/Mini according
        // to its exact front-corner and privileged-entry evidence.
        return (query.mode.plus() && lock.evidence.last_action_was_rotation() && lock.immobile)
            .then_some(SpinEvent::new('T', true, cleared_lines));
    }
    let edge = scoring_edge(piece, lock, corners, cleared_lines, board_after.is_empty());
    SpinDetector::detect_scoring_edge_with_profile(edge, query.mode.profile())
}

pub(crate) fn placement_from_lock(
    piece: PieceKind,
    lock: EntryLock,
    cleared_rows: u32,
    cleared_lines: u8,
) -> StructurePlacement {
    StructurePlacement {
        piece,
        rotation: lock.rotation,
        x: lock.x,
        y: lock.y,
        mask_before_clear: lock.mask,
        cleared_rows,
        cleared_lines,
        evidence: lock.evidence,
    }
}

fn scoring_edge(
    piece: PieceKind,
    lock: EntryLock,
    corners: CornerEvidence,
    cleared_lines: u8,
    perfect_clear: bool,
) -> ScoringExecutionEdge {
    ScoringExecutionEdge::new(
        0,
        0,
        piece,
        lock.rotation,
        lock.x,
        lock.y,
        cleared_lines,
        corners.blocked,
        corners.front,
        lock.evidence,
    )
    .with_perfect_clear(perfect_clear)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_replay::{RotationRequest, ScoringLockEvidence};
    use clearra_rules::profile::rule_profile::RuleProfileId;

    use super::*;
    use crate::{
        model::{PieceInventory, SpinLineRequirement, SpinStructureMode},
        StructureBoard,
    };

    fn query(mode: SpinStructureMode) -> SpinStructureQuery {
        let mut query = SpinStructureQuery::new(
            PieceInventory::from_pieces(PieceKind::STANDARD_TETROMINOES).expect("inventory"),
            mode,
        );
        query.line_requirement = SpinLineRequirement::Any;
        query.rule_profile = RuleProfileId::SrsPlus;
        query
    }

    fn rotated_lock(
        rotation: RotationState,
        x: i8,
        y: i8,
        mask: StructureBoard,
        immobile: bool,
    ) -> EntryLock {
        EntryLock {
            rotation,
            x,
            y,
            mask,
            evidence: ScoringLockEvidence::rotation(
                rotation.counter_clockwise(),
                RotationRequest::Clockwise,
                0,
                0,
                0,
                x,
                y,
            )
            .with_immobile_before_clear(immobile),
            immobile,
        }
    }

    fn t_zero_mask() -> StructureBoard {
        [(4, 1), (5, 1), (6, 1), (5, 2)]
            .into_iter()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).expect("T cell")
            })
    }

    fn board_with(cells: &[(u8, u8)]) -> StructureBoard {
        cells
            .iter()
            .copied()
            .fold(StructureBoard::EMPTY, |board, (x, y)| {
                board.with_cell(x, y).expect("board cell")
            })
    }

    #[test]
    fn regular_and_mini_t_results_stay_separate() {
        let lock = rotated_lock(RotationState::Zero, 4, 1, t_zero_mask(), false);
        let regular_board = board_with(&[(4, 2), (6, 2), (4, 0)]);
        let mini_board = board_with(&[(4, 0), (6, 0), (4, 2)]);
        let regular = classify_lock(
            &query(SpinStructureMode::TSpins),
            regular_board,
            regular_board.union(lock.mask),
            PieceKind::T,
            lock,
            0,
            0,
            0,
        )
        .expect("regular T-spin");
        let mini = classify_lock(
            &query(SpinStructureMode::TSpins),
            mini_board,
            mini_board.union(lock.mask),
            PieceKind::T,
            lock,
            0,
            0,
            0,
        )
        .expect("mini T-spin");
        assert!(!regular.is_mini());
        assert!(mini.is_mini());
    }

    #[test]
    fn plus_t_fallback_is_mini_and_does_not_leak_into_non_plus() {
        let lock = rotated_lock(RotationState::Zero, 4, 1, t_zero_mask(), true);
        let board = board_with(&[(4, 0), (6, 0)]);
        assert!(classify_lock(
            &query(SpinStructureMode::TSpins),
            board,
            board.union(lock.mask),
            PieceKind::T,
            lock,
            0,
            0,
            0,
        )
        .is_none());
        assert!(classify_lock(
            &query(SpinStructureMode::TSpinsPlus),
            board,
            board.union(lock.mask),
            PieceKind::T,
            lock,
            0,
            0,
            0,
        )
        .is_some_and(|event| event.is_mini()));
    }

    #[test]
    fn all_profile_pairs_share_non_t_geometry_and_only_change_the_label() {
        let mask = StructureBoard::from_rows(&[0b1111]).expect("I mask");
        let lock = rotated_lock(RotationState::Zero, 0, 0, mask, true);
        let after = mask;
        for (mini_mode, regular_mode) in [
            (SpinStructureMode::AllMini, SpinStructureMode::AllSpin),
            (
                SpinStructureMode::AllMiniPlus,
                SpinStructureMode::AllSpinPlus,
            ),
        ] {
            let mini = classify_lock(
                &query(mini_mode),
                StructureBoard::EMPTY,
                after,
                PieceKind::I,
                lock,
                0,
                0,
                0,
            )
            .expect("all-mini I-spin");
            let regular = classify_lock(
                &query(regular_mode),
                StructureBoard::EMPTY,
                after,
                PieceKind::I,
                lock,
                0,
                0,
                0,
            )
            .expect("all-spin I-spin");
            assert!(mini.is_mini());
            assert!(!regular.is_mini());
            assert_eq!(mini.piece(), regular.piece());
            assert_eq!(mini.lines(), regular.lines());
        }
        assert!(classify_lock(
            &query(SpinStructureMode::TSpinsPlus),
            StructureBoard::EMPTY,
            after,
            PieceKind::I,
            lock,
            0,
            0,
            0,
        )
        .is_none());
    }
}
