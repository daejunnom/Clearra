use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::registry::piece_registry::PieceRegistry;

use crate::{
    layout::{board64_layout::Board64Layout, board_backend::BoardBackendKind},
    placement::placement_mask::{PlacementMask, PlacementMaskError},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementTable {
    layout: Board64Layout,
    placements: Vec<PlacementMask>,
}

impl PlacementTable {
    pub fn generate(
        layout: Board64Layout,
        registry: PieceRegistry,
    ) -> Result<Self, PlacementMaskError> {
        let mut placements = Vec::new();

        for definition in registry.definitions() {
            for rotation in RotationState::ALL {
                let shape = definition.shape(rotation);
                if u16::from(shape.width()) > layout.width()
                    || u16::from(shape.height()) > layout.height()
                {
                    continue;
                }

                let max_x = layout.width() - u16::from(shape.width());
                let max_y = layout.height() - u16::from(shape.height());

                for y in 0..=max_y {
                    for x in 0..=max_x {
                        placements.push(PlacementMask::new(layout, *definition, rotation, x, y)?);
                    }
                }
            }
        }

        Ok(Self { layout, placements })
    }
}
impl PlacementTable {
    pub fn layout(&self) -> Board64Layout {
        self.layout
    }
}
impl PlacementTable {
    pub fn backend_kind(&self) -> BoardBackendKind {
        BoardBackendKind::Board64
    }
}
impl PlacementTable {
    pub fn placements(&self) -> &[PlacementMask] {
        &self.placements
    }
}
impl PlacementTable {
    pub fn len(&self) -> usize {
        self.placements.len()
    }
}
impl PlacementTable {
    pub fn is_empty(&self) -> bool {
        self.placements.is_empty()
    }
}
impl PlacementTable {
    pub fn placements_for(
        &self,
        piece_kind: PieceKind,
    ) -> impl Iterator<Item = PlacementMask> + '_ {
        self.placements
            .iter()
            .copied()
            .filter(move |placement| placement.piece_kind() == piece_kind)
    }
}

#[cfg(test)]
#[path = "placement_table_tests.rs"]
mod tests;
