use clearra_core_domain::board::{
    board_size::{BoardSize, BoardSizeError},
    standard_pc_board::Board256Mask,
};

use super::board_backend::{BoardBackendKind, BoardLayoutBackend};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Board256Layout {
    size: BoardSize,
}

impl Board256Layout {
    pub fn new(size: BoardSize) -> Result<Self, Board256LayoutError> {
        let area = size.area();
        if area <= 128 {
            return Err(Board256LayoutError::TooFewCells { area });
        }
        if area > 256 {
            return Err(Board256LayoutError::TooManyCells { area });
        }
        Ok(Self { size })
    }

    pub fn standard_10_by_lines(lines: u8) -> Result<Self, Board256LayoutError> {
        let size =
            BoardSize::new(10, u16::from(lines)).map_err(Board256LayoutError::InvalidBoardSize)?;
        Self::new(size)
    }

    pub const fn size(self) -> BoardSize {
        self.size
    }

    pub fn width(self) -> u16 {
        self.size.width()
    }

    pub fn height(self) -> u16 {
        self.size.height()
    }

    pub fn cell_count(self) -> u16 {
        self.size.area() as u16
    }

    pub fn all_cells_mask(self) -> Board256Mask {
        Board256Mask::all_cells(self.cell_count())
            .expect("Board256 layout always has 1..=256 cells")
    }
}

impl BoardLayoutBackend for Board256Layout {
    fn size(self) -> BoardSize {
        self.size()
    }

    fn backend_kind(self) -> BoardBackendKind {
        BoardBackendKind::Board256
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Board256LayoutError {
    InvalidBoardSize(BoardSizeError),
    TooFewCells { area: u32 },
    TooManyCells { area: u32 },
}
