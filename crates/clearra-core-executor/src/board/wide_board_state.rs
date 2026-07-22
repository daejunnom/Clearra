use std::collections::BTreeSet;

use clearra_core_domain::board::board_size::BoardSize;
use clearra_geometry::layout::{
    board_backend::BoardBackendKind, wide_board_layout::WideBoardLayout,
};

use super::board_state_backend::{BoardBackendError, BoardStateBackend};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WideBoardMask {
    cells: Vec<u32>,
}

impl WideBoardMask {
    pub fn new(cells: impl IntoIterator<Item = u32>) -> Self {
        let mut cells = cells.into_iter().collect::<Vec<_>>();
        cells.sort_unstable();
        cells.dedup();
        Self { cells }
    }
}
impl WideBoardMask {
    pub fn cells(&self) -> &[u32] {
        &self.cells
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WideBoardState {
    layout: WideBoardLayout,
    occupied: BTreeSet<u32>,
}

impl WideBoardState {
    pub fn empty(layout: WideBoardLayout) -> Self {
        Self {
            layout,
            occupied: BTreeSet::new(),
        }
    }
}
impl WideBoardState {
    pub fn new(
        layout: WideBoardLayout,
        occupied: impl IntoIterator<Item = u32>,
    ) -> Result<Self, WideBoardStateError> {
        let mut cells = BTreeSet::new();
        let cell_count = layout.cell_count();
        for cell_index in occupied {
            if cell_index >= cell_count {
                return Err(WideBoardStateError::OccupancyOutsideLayout {
                    cell_index,
                    cell_count,
                });
            }
            cells.insert(cell_index);
        }
        Ok(Self {
            layout,
            occupied: cells,
        })
    }
}
impl WideBoardState {
    pub fn layout(&self) -> WideBoardLayout {
        self.layout
    }
}
impl WideBoardState {
    pub fn occupied_cells(&self) -> &BTreeSet<u32> {
        &self.occupied
    }
}

impl BoardStateBackend for WideBoardState {
    type Mask = WideBoardMask;

    fn backend_kind(&self) -> BoardBackendKind {
        BoardBackendKind::Wide
    }

    fn size(&self) -> BoardSize {
        self.layout.size()
    }

    fn occupied_count(&self) -> u32 {
        self.occupied.len() as u32
    }

    fn row_mask(&self, y: u16) -> Option<Self::Mask> {
        if y >= self.layout.height() {
            return None;
        }

        let width = u32::from(self.layout.width());
        let start = u32::from(y) * width;
        Some(WideBoardMask::new(start..start + width))
    }

    fn singleton_mask(&self, cell_index: u32) -> Option<Self::Mask> {
        if cell_index >= self.layout.cell_count() {
            return None;
        }
        Some(WideBoardMask::new([cell_index]))
    }

    fn collides_mask(&self, mask: &Self::Mask) -> bool {
        mask.cells()
            .iter()
            .any(|cell_index| self.occupied.contains(cell_index))
    }

    fn place_mask(&self, mask: &Self::Mask) -> Result<Self, BoardBackendError> {
        if mask
            .cells()
            .iter()
            .any(|cell_index| *cell_index >= self.layout.cell_count())
        {
            return Err(BoardBackendError::MaskOutsideLayout);
        }
        if self.collides_mask(mask) {
            return Err(BoardBackendError::Collision);
        }

        let mut occupied = self.occupied.clone();
        occupied.extend(mask.cells().iter().copied());
        Ok(Self {
            layout: self.layout,
            occupied,
        })
    }

    fn clear_full_rows(&self) -> (Self, u8) {
        let width = u32::from(self.layout.width());
        let mut compacted = BTreeSet::new();
        let mut write_y = 0_u32;
        let mut cleared_lines = 0_u8;

        for read_y in 0..u32::from(self.layout.height()) {
            let row_start = read_y * width;
            let row_count = (0..width)
                .filter(|x| self.occupied.contains(&(row_start + *x)))
                .count() as u32;

            if row_count == width {
                cleared_lines += 1;
                continue;
            }

            let write_start = write_y * width;
            for x in 0..width {
                if self.occupied.contains(&(row_start + x)) {
                    compacted.insert(write_start + x);
                }
            }
            write_y += 1;
        }

        (
            Self {
                layout: self.layout,
                occupied: compacted,
            },
            cleared_lines,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WideBoardStateError {
    OccupancyOutsideLayout { cell_index: u32, cell_count: u32 },
}

#[cfg(test)]
#[path = "wide_board_state_tests.rs"]
mod tests;
