use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::registry::piece_registry::PieceDefinition;

use crate::{
    layout::{board64_layout::Board64Layout, cell_indexer::try_cell_index},
    placement::placement_bounds::shape_fits,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlacementMask {
    piece_kind: PieceKind,
    rotation: RotationState,
    x: u16,
    y: u16,
    mask: u64,
}

impl PlacementMask {
    pub fn new(
        layout: Board64Layout,
        definition: PieceDefinition,
        rotation: RotationState,
        x: u16,
        y: u16,
    ) -> Result<Self, PlacementMaskError> {
        let shape = definition.shape(rotation);
        if !shape_fits(layout, shape, x, y) {
            return Err(PlacementMaskError::OutOfBounds);
        }

        let mut mask = 0_u64;
        for cell in shape.cells() {
            let absolute_x = i32::from(x) + i32::from(cell.x());
            let absolute_y = i32::from(y) + i32::from(cell.y());
            if absolute_x < 0 || absolute_y < 0 {
                return Err(PlacementMaskError::OutOfBounds);
            }
            let index = try_cell_index(layout, absolute_x as u16, absolute_y as u16)
                .map_err(|_| PlacementMaskError::OutOfBounds)?;
            mask |= 1_u64 << index;
        }

        Ok(Self {
            piece_kind: definition.kind(),
            rotation,
            x,
            y,
            mask,
        })
    }
}
impl PlacementMask {
    pub fn piece_kind(self) -> PieceKind {
        self.piece_kind
    }
}
impl PlacementMask {
    pub fn rotation(self) -> RotationState {
        self.rotation
    }
}
impl PlacementMask {
    pub fn x(self) -> u16 {
        self.x
    }
}
impl PlacementMask {
    pub fn y(self) -> u16 {
        self.y
    }
}
impl PlacementMask {
    pub fn mask(self) -> u64 {
        self.mask
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementMaskError {
    OutOfBounds,
}

#[cfg(test)]
#[path = "placement_mask_tests.rs"]
mod tests;
