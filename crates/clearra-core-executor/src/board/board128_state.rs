use clearra_core_domain::board::board_size::BoardSize;
use clearra_geometry::layout::{
    board128_layout::Board128Layout, board_backend::BoardBackendKind, row_mask_builder::row_mask128,
};

use super::board_state_backend::{BoardBackendError, BoardStateBackend};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Board128State {
    layout: Board128Layout,
    occupied: u128,
}

impl Board128State {
    pub fn empty(layout: Board128Layout) -> Self {
        Self {
            layout,
            occupied: 0,
        }
    }
}
impl Board128State {
    pub fn new(layout: Board128Layout, occupied: u128) -> Result<Self, Board128StateError> {
        let layout_mask = layout.all_cells_mask();
        if occupied & !layout_mask != 0 {
            return Err(Board128StateError::OccupancyOutsideLayout {
                occupied,
                layout_mask,
            });
        }
        Ok(Self { layout, occupied })
    }
}
impl Board128State {
    pub fn layout(self) -> Board128Layout {
        self.layout
    }
}
impl Board128State {
    pub fn occupied(self) -> u128 {
        self.occupied
    }
}

impl BoardStateBackend for Board128State {
    type Mask = u128;

    fn backend_kind(&self) -> BoardBackendKind {
        BoardBackendKind::Board128
    }

    fn size(&self) -> BoardSize {
        self.layout.size()
    }

    fn occupied_count(&self) -> u32 {
        self.occupied.count_ones()
    }

    fn row_mask(&self, y: u16) -> Option<Self::Mask> {
        row_mask128(self.layout, y)
    }

    fn singleton_mask(&self, cell_index: u32) -> Option<Self::Mask> {
        if cell_index >= u32::from(self.layout.cell_count()) {
            return None;
        }
        Some(1_u128 << cell_index)
    }

    fn collides_mask(&self, mask: &Self::Mask) -> bool {
        self.occupied & *mask != 0
    }

    fn place_mask(&self, mask: &Self::Mask) -> Result<Self, BoardBackendError> {
        if *mask & !self.layout.all_cells_mask() != 0 {
            return Err(BoardBackendError::MaskOutsideLayout);
        }
        if self.collides_mask(mask) {
            return Err(BoardBackendError::Collision);
        }
        Self::new(self.layout, self.occupied | *mask)
            .map_err(|_| BoardBackendError::MaskOutsideLayout)
    }

    fn clear_full_rows(&self) -> (Self, u8) {
        let width = self.layout.width();
        let full_row = if width == 128 {
            u128::MAX
        } else {
            (1_u128 << width) - 1
        };
        let mut compacted = 0_u128;
        let mut write_y = 0_u16;
        let mut cleared_lines = 0_u8;

        for read_y in 0..self.layout.height() {
            let mask = row_mask128(self.layout, read_y).expect("read_y is within layout");
            let row = (self.occupied & mask) >> (u32::from(read_y) * u32::from(width));

            if row == full_row {
                cleared_lines += 1;
                continue;
            }

            compacted |= row << (u32::from(write_y) * u32::from(width));
            write_y += 1;
        }

        (
            Self::new(self.layout, compacted).expect("compacted board stays inside layout"),
            cleared_lines,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Board128StateError {
    OccupancyOutsideLayout { occupied: u128, layout_mask: u128 },
}

#[cfg(test)]
#[path = "board128_state_tests.rs"]
mod tests;
