use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_geometry::layout::board64_layout::Board64Layout;

use crate::trace::solution_trace::SolutionTrace;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColoredCellOwner {
    step_index: usize,
    piece: PieceKind,
}

impl ColoredCellOwner {
    pub fn new(step_index: usize, piece: PieceKind) -> Self {
        Self { step_index, piece }
    }
}
impl ColoredCellOwner {
    pub fn step_index(self) -> usize {
        self.step_index
    }
}
impl ColoredCellOwner {
    pub fn piece(self) -> PieceKind {
        self.piece
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ColoredCellOwnership {
    layout: Board64Layout,
    owners: Vec<Option<ColoredCellOwner>>,
}

impl ColoredCellOwnership {
    pub fn from_trace(trace: &SolutionTrace) -> Result<Self, ColoredCellOwnershipError> {
        let layout = trace
            .steps()
            .first()
            .map(|step| step.board_before().layout())
            .ok_or(ColoredCellOwnershipError::EmptyTrace)?;
        let mut owners = vec![None; usize::from(layout.cell_count())];

        for step in trace.steps() {
            if step.board_before().layout() != layout
                || step.board_after().after_placement().layout() != layout
                || step.board_after().after_line_clear().layout() != layout
            {
                return Err(ColoredCellOwnershipError::LayoutMismatch);
            }

            let owner =
                ColoredCellOwner::new(step.step_index(), step.piece_decision().active_piece());
            apply_placement_owners(&mut owners, step.placement().mask(), owner)?;
            owners = compact_owners_after_line_clear(
                layout,
                owners,
                step.board_after().after_placement().occupied(),
            );
        }

        Ok(Self { layout, owners })
    }
}
impl ColoredCellOwnership {
    pub fn layout(&self) -> Board64Layout {
        self.layout
    }
}
impl ColoredCellOwnership {
    pub fn owner_at(&self, x: u16, y: u16) -> Option<ColoredCellOwner> {
        if x >= self.layout.width() || y >= self.layout.height() {
            return None;
        }
        let index = usize::from(y) * usize::from(self.layout.width()) + usize::from(x);
        self.owners.get(index).copied().flatten()
    }
}
impl ColoredCellOwnership {
    pub fn owners(&self) -> &[Option<ColoredCellOwner>] {
        &self.owners
    }
}
impl ColoredCellOwnership {
    pub fn owned_cell_count(&self) -> usize {
        self.owners.iter().filter(|owner| owner.is_some()).count()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColoredCellOwnershipError {
    EmptyTrace,
    LayoutMismatch,
    PlacementOutsideLayout { mask: u64, layout_mask: u64 },
}

fn apply_placement_owners(
    owners: &mut [Option<ColoredCellOwner>],
    mask: u64,
    owner: ColoredCellOwner,
) -> Result<(), ColoredCellOwnershipError> {
    let layout_mask = if owners.len() == 64 {
        u64::MAX
    } else {
        (1_u64 << owners.len()) - 1
    };
    if mask & !layout_mask != 0 {
        return Err(ColoredCellOwnershipError::PlacementOutsideLayout { mask, layout_mask });
    }
    for index in 0..owners.len() {
        if (mask & (1_u64 << index)) != 0 {
            owners[index] = Some(owner);
        }
    }
    Ok(())
}

fn compact_owners_after_line_clear(
    layout: Board64Layout,
    owners: Vec<Option<ColoredCellOwner>>,
    occupied_after_placement: u64,
) -> Vec<Option<ColoredCellOwner>> {
    let width = usize::from(layout.width());
    let height = usize::from(layout.height());
    let mut compacted = vec![None; owners.len()];
    let mut dest_y = 0_usize;

    for source_y in 0..height {
        let row_mask = row_mask(width, source_y);
        if occupied_after_placement & row_mask == row_mask {
            continue;
        }

        for x in 0..width {
            let source = source_y * width + x;
            let dest = dest_y * width + x;
            compacted[dest] = owners[source];
        }
        dest_y += 1;
    }

    compacted
}

fn row_mask(width: usize, y: usize) -> u64 {
    let start = y * width;
    ((1_u64 << width) - 1) << start
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_geometry::{
        layout::board64_layout::Board64Layout, placement::placement_mask::PlacementMask,
    };
    use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

    use crate::{
        board::board64_state::Board64State,
        trace::{
            BoardAfterStep, HoldDecision, LineClearEvent, PieceDecision, PlacementStep,
            SolutionTrace,
        },
    };

    use super::*;

    #[test]
    fn ownership_compacts_with_line_clears() {
        let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
        let registry = standard_tetromino_registry();
        let piece = registry.get(PieceKind::O).expect("O");
        let placement =
            PlacementMask::new(layout, piece, RotationState::Zero, 0, 0).expect("placement");
        let board_before = Board64State::new(layout, 0x03fc).expect("board");
        let after_placement = Board64State::new(layout, 0x0fff).expect("after placement");
        let after_line_clear = Board64State::empty(layout);
        let trace = SolutionTrace::new(vec![PlacementStep::new(
            0,
            PieceDecision::new(PieceKind::O, 0, 1, None, None, HoldDecision::None),
            placement,
            board_before,
            BoardAfterStep::new(after_placement, after_line_clear),
            LineClearEvent::new(2),
        )]);

        let ownership = ColoredCellOwnership::from_trace(&trace).expect("ownership");

        assert_eq!(ownership.owned_cell_count(), 2);
        assert_eq!(
            ownership.owner_at(0, 0),
            Some(ColoredCellOwner::new(0, PieceKind::O))
        );
        assert_eq!(
            ownership.owner_at(1, 0),
            Some(ColoredCellOwner::new(0, PieceKind::O))
        );
    }
}
