use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use crate::{
    board::StructureBoard,
    model::{canonical_geometry_rotation, StructureOperation},
};

/// The field before line compaction. Full rows remain present here so that a
/// placement keeps the same identity regardless of which earlier lock made a
/// row disappear from the physical playfield.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct LogicalBoard {
    board: StructureBoard,
}

/// Cumulative logical rows removed from the bounded structure field.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct DeletedLogicalRows {
    bits: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LogicalLockResult {
    pub(crate) identity: StructureOperation,
    pub(crate) board_after: LogicalBoard,
    pub(crate) deleted_after: DeletedLogicalRows,
    /// Newly completed rows in immutable logical coordinates.
    pub(crate) newly_deleted_rows: u32,
}

impl LogicalBoard {
    pub(crate) const fn from_initial(board: StructureBoard) -> Self {
        Self { board }
    }

    pub(crate) fn initial_deleted_rows(self, height: u8) -> DeletedLogicalRows {
        let mut deleted = DeletedLogicalRows::default();
        for row in 0..height {
            if self.board.row_bits(row) == 0x03ff {
                deleted.insert(row);
            }
        }
        deleted
    }

    pub(crate) fn compact(self, deleted: DeletedLogicalRows, height: u8) -> StructureBoard {
        let mut physical = StructureBoard::EMPTY;
        let mut physical_row = 0_u8;
        for logical_row in 0..height {
            if deleted.contains(logical_row) {
                continue;
            }
            let bits = self.board.row_bits(logical_row);
            for x in 0..StructureBoard::WIDTH {
                if bits & (1_u16 << x) != 0 {
                    physical.insert_index(
                        u16::from(physical_row) * u16::from(StructureBoard::WIDTH) + u16::from(x),
                    );
                }
            }
            physical_row += 1;
        }
        physical
    }
}

impl DeletedLogicalRows {
    const fn contains(self, row: u8) -> bool {
        self.bits & (1_u32 << row) != 0
    }

    fn insert(&mut self, row: u8) {
        self.bits |= 1_u32 << row;
    }

    fn select_alive(self, physical_row: u8, logical_height: u8) -> Option<u8> {
        let mut alive = 0_u8;
        for logical_row in 0..logical_height {
            if !self.contains(logical_row) {
                if alive == physical_row {
                    return Some(logical_row);
                }
                alive += 1;
            }
        }
        None
    }

    const fn restricted_to(self, bottom: u8, top: u8) -> u32 {
        let below_top = if top == 31 {
            u32::MAX
        } else {
            (1_u32 << (top + 1)) - 1
        };
        let below_bottom = (1_u32 << bottom) - 1;
        self.bits & below_top & !below_bottom
    }
}

/// Lifts a reachable physical lock into the bounded logical structure field.
/// A physical lock whose cells would require logical rows at or above
/// `logical_height` is outside the structure catalog and is rejected.
pub(crate) fn apply_physical_lock(
    board: LogicalBoard,
    deleted: DeletedLogicalRows,
    logical_height: u8,
    piece: PieceKind,
    rotation: RotationState,
    _physical_x: i8,
    physical_mask: StructureBoard,
) -> Option<LogicalLockResult> {
    let mut logical_mask = StructureBoard::EMPTY;
    let mut count = 0_u8;
    let mut bottom = u8::MAX;
    let mut top = 0_u8;
    for physical_y in 0..logical_height {
        let row = physical_mask.row_bits(physical_y);
        if row == 0 {
            continue;
        }
        let logical_y = deleted.select_alive(physical_y, logical_height)?;
        for cell_x in 0..StructureBoard::WIDTH {
            if row & (1_u16 << cell_x) != 0 {
                logical_mask.insert_index(
                    u16::from(logical_y) * u16::from(StructureBoard::WIDTH) + u16::from(cell_x),
                );
                count += 1;
                bottom = bottom.min(logical_y);
                top = top.max(logical_y);
            }
        }
    }
    if count != 4 {
        return None;
    }

    let definition = standard_tetromino_registry()
        .get(piece)
        .expect("standard tetromino exists");
    let canonical_rotation = canonical_geometry_rotation(piece, rotation);
    let canonical_shape = definition.shape(canonical_rotation);
    let minimum_shape_x = canonical_shape
        .cells()
        .iter()
        .map(|cell| cell.x())
        .min()
        .expect("tetromino has cells");
    let minimum_shape_y = canonical_shape
        .cells()
        .iter()
        .map(|cell| cell.y())
        .min()
        .expect("tetromino has cells");
    let left = (0..StructureBoard::WIDTH)
        .find(|cell_x| (bottom..=top).any(|row| logical_mask.contains(*cell_x, row)))
        .expect("tetromino has cells");
    let origin_x = i16::from(left) - i16::from(minimum_shape_x);
    let origin_x = i8::try_from(origin_x).ok()?;
    let origin_y = i16::from(bottom) - i16::from(minimum_shape_y);
    let origin_y = i8::try_from(origin_y).ok()?;
    let identity = StructureOperation::new(
        piece,
        canonical_rotation,
        origin_x,
        origin_y,
        logical_mask,
        deleted.restricted_to(bottom, top),
    );

    let board_after = LogicalBoard {
        board: board.board.union(logical_mask),
    };
    let mut deleted_after = deleted;
    let mut newly_deleted_rows = 0_u32;
    for row in bottom..=top {
        if logical_mask.row_bits(row) != 0
            && !deleted.contains(row)
            && board_after.board.row_bits(row) == 0x03ff
        {
            deleted_after.insert(row);
            newly_deleted_rows |= 1_u32 << row;
        }
    }

    Some(LogicalLockResult {
        identity,
        board_after,
        deleted_after,
        newly_deleted_rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_full_row_lift_is_deterministic() {
        let initial = StructureBoard::from_rows(&[0x03ff]).expect("initial field");
        let logical = LogicalBoard::from_initial(initial);
        let deleted = logical.initial_deleted_rows(4);
        let physical_mask = StructureBoard::from_rows(&[0b1111]).expect("I mask");

        let first = apply_physical_lock(
            logical,
            deleted,
            4,
            PieceKind::I,
            RotationState::Zero,
            0,
            physical_mask,
        )
        .expect("bounded lift");
        let second = apply_physical_lock(
            logical,
            deleted,
            4,
            PieceKind::I,
            RotationState::Zero,
            0,
            physical_mask,
        )
        .expect("bounded lift");

        assert_eq!(first, second);
        assert_eq!(first.identity.mask().row_bits(1), 0b1111);
        assert_eq!(first.identity.y(), 1);
        assert_eq!(first.identity.need_deleted_rows(), 0);
        assert_eq!(logical.compact(deleted, 4).row_bits(0), 0);

        let other_mask = StructureBoard::from_rows(&[0, 0b11110000]).expect("other I mask");
        let first_then_other = apply_physical_lock(
            first.board_after,
            first.deleted_after,
            4,
            PieceKind::I,
            RotationState::Zero,
            4,
            other_mask,
        )
        .expect("second bounded lift");
        let other = apply_physical_lock(
            logical,
            deleted,
            4,
            PieceKind::I,
            RotationState::Zero,
            4,
            other_mask,
        )
        .expect("other bounded lift");
        let other_then_first = apply_physical_lock(
            other.board_after,
            other.deleted_after,
            4,
            PieceKind::I,
            RotationState::Zero,
            0,
            physical_mask,
        )
        .expect("second bounded lift");

        let mut forward = [first.identity, first_then_other.identity];
        let mut reverse = [other.identity, other_then_first.identity];
        forward.sort_unstable();
        reverse.sort_unstable();
        assert_eq!(forward, reverse);
        assert_eq!(first_then_other.board_after, other_then_first.board_after);
        assert_eq!(
            first_then_other.deleted_after,
            other_then_first.deleted_after
        );
    }

    #[test]
    fn deleted_gap_inside_piece_span_is_part_of_identity() {
        let initial = StructureBoard::from_rows(&[0, 0, 0x03ff]).expect("initial field");
        let logical = LogicalBoard::from_initial(initial);
        let deleted = logical.initial_deleted_rows(6);
        let physical_mask = StructureBoard::from_rows(&[0b1, 0b1, 0b1, 0b1]).expect("I mask");
        let lifted = apply_physical_lock(
            logical,
            deleted,
            6,
            PieceKind::I,
            RotationState::Right,
            0,
            physical_mask,
        )
        .expect("bounded lift");

        assert_eq!(lifted.identity.need_deleted_rows(), 1 << 2);
        assert_eq!(lifted.identity.mask().row_bits(0), 1);
        assert_eq!(lifted.identity.mask().row_bits(1), 1);
        assert_eq!(lifted.identity.mask().row_bits(2), 0);
        assert_eq!(lifted.identity.mask().row_bits(3), 1);
        assert_eq!(lifted.identity.mask().row_bits(4), 1);
    }

    #[test]
    fn lift_rejects_cells_above_the_logical_catalog_height() {
        let initial = StructureBoard::from_rows(&[0x03ff]).expect("initial field");
        let logical = LogicalBoard::from_initial(initial);
        let deleted = logical.initial_deleted_rows(4);
        let top_physical_mask = StructureBoard::from_rows(&[0, 0, 0, 0b1111]).expect("top I mask");

        assert!(apply_physical_lock(
            logical,
            deleted,
            4,
            PieceKind::I,
            RotationState::Zero,
            0,
            top_physical_mask,
        )
        .is_none());
    }

    #[test]
    fn newly_deleted_rows_stay_in_logical_coordinates() {
        let initial = StructureBoard::from_rows(&[0x03ff, 0x03fe]).expect("initial field");
        let logical = LogicalBoard::from_initial(initial);
        let deleted = logical.initial_deleted_rows(4);
        let physical_mask = StructureBoard::from_rows(&[1, 0b111]).expect("L mask");
        let lifted = apply_physical_lock(
            logical,
            deleted,
            4,
            PieceKind::L,
            RotationState::Zero,
            0,
            physical_mask,
        )
        .expect("bounded lift");

        assert_eq!(lifted.newly_deleted_rows, 1 << 1);
    }

    #[test]
    fn symmetric_rotations_with_the_same_mask_share_one_identity() {
        let logical = LogicalBoard::default();
        let deleted = DeletedLogicalRows::default();
        for (piece, first, second, mask) in [
            (
                PieceKind::I,
                RotationState::Zero,
                RotationState::Two,
                StructureBoard::from_rows(&[0b1111]).expect("horizontal I"),
            ),
            (
                PieceKind::I,
                RotationState::Right,
                RotationState::Left,
                StructureBoard::from_rows(&[1, 1, 1, 1]).expect("vertical I"),
            ),
            (
                PieceKind::S,
                RotationState::Zero,
                RotationState::Two,
                StructureBoard::from_rows(&[0b0011, 0b0110]).expect("horizontal S"),
            ),
            (
                PieceKind::S,
                RotationState::Right,
                RotationState::Left,
                StructureBoard::from_rows(&[0b0010, 0b0011, 0b0001]).expect("vertical S"),
            ),
            (
                PieceKind::Z,
                RotationState::Zero,
                RotationState::Two,
                StructureBoard::from_rows(&[0b0110, 0b0011]).expect("horizontal Z"),
            ),
            (
                PieceKind::Z,
                RotationState::Right,
                RotationState::Left,
                StructureBoard::from_rows(&[0b0001, 0b0011, 0b0010]).expect("vertical Z"),
            ),
        ] {
            let left = apply_physical_lock(logical, deleted, 6, piece, first, 0, mask)
                .expect("first rotation");
            let right = apply_physical_lock(logical, deleted, 6, piece, second, 3, mask)
                .expect("symmetric rotation");
            assert_eq!(left.identity, right.identity, "{piece:?}");
        }
    }
}
