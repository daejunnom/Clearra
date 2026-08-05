use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

use crate::{board::StructureBoard, entry::EntryLock, model::SpinStructureMode};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CornerEvidence {
    pub blocked: u8,
    pub front: u8,
}

pub(crate) fn evidence(
    board_before: StructureBoard,
    piece: PieceKind,
    lock: EntryLock,
    height: u8,
) -> CornerEvidence {
    if piece != PieceKind::T {
        return CornerEvidence::default();
    }
    let (center_x, center_y) = t_center(lock.rotation, lock.x, lock.y);
    let occupied = board_before.union(lock.mask);
    let blocked = [(-1, -1), (1, -1), (-1, 1), (1, 1)]
        .into_iter()
        .filter(|(dx, dy)| blocked_at(occupied, center_x + dx, center_y + dy, height))
        .count() as u8;
    let front_offsets = match lock.rotation {
        RotationState::Zero => [(-1, 1), (1, 1)],
        RotationState::Right => [(1, -1), (1, 1)],
        RotationState::Two => [(-1, -1), (1, -1)],
        RotationState::Left => [(-1, -1), (-1, 1)],
    };
    let front = front_offsets
        .into_iter()
        .filter(|(dx, dy)| blocked_at(occupied, center_x + dx, center_y + dy, height))
        .count() as u8;
    CornerEvidence { blocked, front }
}

/// Exact recognition precondition used only to avoid constructing scoring
/// edges that the profile contract must reject.  Non-T targets never pass
/// through the T-corner branch.
pub(crate) fn can_classify(
    mode: SpinStructureMode,
    piece: PieceKind,
    lock: EntryLock,
    corners: CornerEvidence,
) -> bool {
    if !lock.evidence.last_action_was_rotation() {
        return false;
    }
    if piece == PieceKind::T {
        corners.blocked >= 3 || (mode.plus() && lock.immobile)
    } else {
        !mode.t_only() && lock.immobile
    }
}

fn t_center(rotation: RotationState, x: i8, y: i8) -> (i16, i16) {
    match rotation {
        RotationState::Zero => (i16::from(x) + 1, i16::from(y)),
        RotationState::Right => (i16::from(x), i16::from(y) + 1),
        RotationState::Two | RotationState::Left => (i16::from(x) + 1, i16::from(y) + 1),
    }
}

fn blocked_at(board: StructureBoard, x: i16, y: i16, height: u8) -> bool {
    if x < 0 || x >= i16::from(StructureBoard::WIDTH) || y < 0 {
        return true;
    }
    if y >= i16::from(height) {
        return false;
    }
    board.contains(x as u8, y as u8)
}
