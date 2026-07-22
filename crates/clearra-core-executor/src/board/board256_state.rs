use clearra_core_domain::board::{
    board_size::BoardSize,
    standard_pc_board::{Board256Mask, Board256MaskError},
};
use clearra_geometry::layout::{board256_layout::Board256Layout, board_backend::BoardBackendKind};

use super::board_state_backend::{BoardBackendError, BoardStateBackend};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Board256State {
    layout: Board256Layout,
    occupied: Board256Mask,
}

impl Board256State {
    pub fn empty(layout: Board256Layout) -> Self {
        Self {
            layout,
            occupied: Board256Mask::EMPTY,
        }
    }

    pub fn new(layout: Board256Layout, occupied: Board256Mask) -> Result<Self, Board256StateError> {
        if !occupied
            .fits_cell_count(layout.cell_count())
            .map_err(Board256StateError::Mask)?
        {
            return Err(Board256StateError::OccupancyOutsideLayout);
        }
        Ok(Self { layout, occupied })
    }

    pub const fn layout(self) -> Board256Layout {
        self.layout
    }

    pub const fn occupied(self) -> Board256Mask {
        self.occupied
    }
}

impl BoardStateBackend for Board256State {
    type Mask = Board256Mask;

    fn backend_kind(&self) -> BoardBackendKind {
        BoardBackendKind::Board256
    }

    fn size(&self) -> BoardSize {
        self.layout.size()
    }

    fn occupied_count(&self) -> u32 {
        self.occupied.count_ones()
    }

    fn row_mask(&self, y: u16) -> Option<Self::Mask> {
        Board256Mask::row(self.layout.width(), self.layout.height(), y).ok()
    }

    fn singleton_mask(&self, cell_index: u32) -> Option<Self::Mask> {
        let cell_index = u16::try_from(cell_index).ok()?;
        if cell_index >= self.layout.cell_count() {
            return None;
        }
        Board256Mask::singleton(cell_index).ok()
    }

    fn collides_mask(&self, mask: &Self::Mask) -> bool {
        self.occupied.intersects(*mask)
    }

    fn place_mask(&self, mask: &Self::Mask) -> Result<Self, BoardBackendError> {
        if !mask
            .fits_cell_count(self.layout.cell_count())
            .map_err(|_| BoardBackendError::MaskOutsideLayout)?
        {
            return Err(BoardBackendError::MaskOutsideLayout);
        }
        if self.collides_mask(mask) {
            return Err(BoardBackendError::Collision);
        }
        Self::new(self.layout, self.occupied.union(*mask))
            .map_err(|_| BoardBackendError::MaskOutsideLayout)
    }

    fn clear_full_rows(&self) -> (Self, u8) {
        let width = self.layout.width();
        let mut compacted = Board256Mask::EMPTY;
        let mut write_y = 0_u16;
        let mut cleared_lines = 0_u8;

        for read_y in 0..self.layout.height() {
            let row = Board256Mask::row(width, self.layout.height(), read_y)
                .expect("read row is inside Board256 layout");
            if self.occupied.intersects(row) && self.occupied.union(row) == self.occupied {
                cleared_lines = cleared_lines.saturating_add(1);
                continue;
            }
            for x in 0..width {
                let source = read_y * width + x;
                if self.occupied.contains_index(source) {
                    let target = write_y * width + x;
                    compacted = compacted.union(
                        Board256Mask::singleton(target)
                            .expect("compacted cell remains inside Board256"),
                    );
                }
            }
            write_y += 1;
        }

        (
            Self::new(self.layout, compacted).expect("compacted board stays inside layout"),
            cleared_lines,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Board256StateError {
    OccupancyOutsideLayout,
    Mask(Board256MaskError),
}
