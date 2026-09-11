//! On-demand reconstruction of every inverse lock-clear temporal parent.
//! A fixed logical tetromino has at most one parent per rotation: its occupied
//! target rows and horizontal translation determine the row projection.

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InverseParentPolicy { EagerTable, EagerRaw, Deferred }

std::thread_local! {
    static PARENT_POLICY: std::cell::Cell<InverseParentPolicy> =
        const { std::cell::Cell::new(InverseParentPolicy::EagerTable) };
}

pub(super) fn inverse_parent_policy() -> InverseParentPolicy {
    PARENT_POLICY.with(std::cell::Cell::get)
}

pub(super) fn set_inverse_parent_policy(policy: InverseParentPolicy) -> InverseParentPolicy {
    PARENT_POLICY.with(|current| current.replace(policy))
}

/// Exercise the existing mode fixture under every parent representation while
/// restoring the thread-local experiment even if one assertion unwinds.
#[cfg(test)]
pub(super) fn assert_inverse_parent_policy_parity<T: core::fmt::Debug + PartialEq>(
    mut evaluate: impl FnMut() -> T,
) {
    struct Restore(InverseParentPolicy);
    impl Drop for Restore {
        fn drop(&mut self) {
            set_inverse_parent_policy(self.0);
        }
    }
    let _restore = Restore(inverse_parent_policy());
    let mut expected = None;
    for policy in [InverseParentPolicy::EagerTable, InverseParentPolicy::EagerRaw,
        InverseParentPolicy::Deferred] {
        set_inverse_parent_policy(policy);
        let actual = evaluate();
        if let Some(expected) = &expected {
            assert_eq!(&actual, expected, "parent policy {policy:?}");
        } else {
            expected = Some(actual);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct TemporalParent {
    pub required_deleted_rows: u32,
    pub rotation: RotationState,
    pub x: i8,
    pub target_anchor_y: i8,
}

/// Open until all four rotations have been examined. A missing parent from
/// one advance is Pending, never proof that this whole family is empty.
pub(super) enum ParentAdvance {
    Pending,
    Parent(TemporalParent),
    Complete,
}

pub(super) struct InverseParentCursor {
    width: u8,
    height: u8,
    piece: PieceKind,
    target_cells: [u16; 4],
    target_rows: [u8; 4],
    row_count: usize,
    minimum_x: i16,
    next_rotation: usize,
}

impl InverseParentCursor {
    pub fn new(width: u8, height: u8, piece: PieceKind, mut cells: [u16; 4]) -> Option<Self> {
        if width == 0 || width > 64 || height == 0 || height > 32 {
            return None;
        }
        if !PieceKind::STANDARD_TETROMINOES.contains(&piece) { return None; }
        cells.sort_unstable();
        if cells.windows(2).any(|pair| pair[0] == pair[1])
            || cells[3] >= u16::from(width) * u16::from(height)
        {
            return None;
        }
        let mut rows = [0; 4];
        let mut row_count = 0;
        let mut minimum_x = i16::from(width);
        for cell in cells {
            let row = (cell / u16::from(width)) as u8;
            if row_count == 0 || rows[row_count - 1] != row {
                rows[row_count] = row;
                row_count += 1;
            }
            minimum_x = minimum_x.min((cell % u16::from(width)) as i16);
        }
        Some(Self {
            width,
            height,
            piece,
            target_cells: cells,
            target_rows: rows,
            row_count,
            minimum_x,
            next_rotation: 0,
        })
    }

    /// One fixed, allocation-free rotation attempt. Callers must exhaust the
    /// cursor before issuing any complete-family negative conclusion.
    pub fn advance(&mut self) -> ParentAdvance {
        let Some(rotation) = RotationState::ALL.get(self.next_rotation).copied() else {
            return ParentAdvance::Complete;
        };
        self.next_rotation += 1;
        self.parent_for_rotation(rotation)
            .map_or(ParentAdvance::Pending, ParentAdvance::Parent)
    }

    fn parent_for_rotation(&self, rotation: RotationState) -> Option<TemporalParent> {
        let registry = standard_tetromino_registry();
        let shape = registry.get(self.piece)?.shape(rotation);
        let shape_cells = shape.cells();
        let mut local_rows = shape_cells.map(|cell| u8::try_from(cell.y()).ok());
        if local_rows.iter().any(Option::is_none) {
            return None;
        }
        local_rows.sort_unstable();
        let mut rows = [0_u8; 4];
        let mut count = 0;
        for row in local_rows.into_iter().flatten() {
            if count == 0 || rows[count - 1] != row {
                rows[count] = row;
                count += 1;
            }
        }
        if count != self.row_count {
            return None;
        }
        let x = self.minimum_x - i16::from(shape_cells.iter().map(|cell| cell.x()).min()?);
        if x < 0 || x > i16::from(self.width) - i16::from(shape.width()) {
            return None;
        }
        let mut required_deleted_rows = 0_u32;
        for index in 0..count {
            let target = self.target_rows[index];
            let minimum = if index == 0 {
                rows[0]
            } else {
                self.target_rows[index - 1].checked_add(rows[index] - rows[index - 1])?
            };
            let maximum = self
                .height
                .checked_sub(1)?
                .checked_sub(rows[count - 1] - rows[index])?;
            if target < minimum || target > maximum {
                return None;
            }
            if index != 0 {
                for row in minimum..target {
                    required_deleted_rows |= 1_u32 << row;
                }
            }
        }
        let mut projected = [0_u16; 4];
        for (index, cell) in shape_cells.into_iter().enumerate() {
            let local = rows[..count].binary_search(&(cell.y() as u8)).ok()?;
            let target_x = x + i16::from(cell.x());
            if target_x < 0 || target_x >= i16::from(self.width) {
                return None;
            }
            projected[index] =
                u16::from(self.target_rows[local]) * u16::from(self.width) + target_x as u16;
        }
        projected.sort_unstable();
        if projected != self.target_cells {
            return None;
        }
        Some(TemporalParent {
            required_deleted_rows,
            rotation,
            x: x as i8,
            target_anchor_y: self.target_rows[0] as i8,
        })
    }
}
