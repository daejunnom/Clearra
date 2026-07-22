use clearra_core_domain::board::board_size::{BoardSize, BoardSizeError};

use super::board_backend::{BoardBackendKind, BoardLayoutBackend};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Board128Layout {
    size: BoardSize,
}

impl Board128Layout {
    pub fn new(size: BoardSize) -> Result<Self, Board128LayoutError> {
        let area = size.area();
        if area <= 64 {
            return Err(Board128LayoutError::TooFewCells { area });
        }
        if area > 128 {
            return Err(Board128LayoutError::TooManyCells { area });
        }
        Ok(Self { size })
    }
}
impl Board128Layout {
    pub fn standard_10_by_lines(lines: u8) -> Result<Self, Board128LayoutError> {
        let size =
            BoardSize::new(10, u16::from(lines)).map_err(Board128LayoutError::InvalidBoardSize)?;
        Self::new(size)
    }
}
impl Board128Layout {
    pub fn size(self) -> BoardSize {
        self.size
    }
}
impl Board128Layout {
    pub fn width(self) -> u16 {
        self.size.width()
    }
}
impl Board128Layout {
    pub fn height(self) -> u16 {
        self.size.height()
    }
}
impl Board128Layout {
    pub fn cell_count(self) -> u8 {
        self.size.area() as u8
    }
}
impl Board128Layout {
    pub fn all_cells_mask(self) -> u128 {
        let cell_count = self.cell_count();
        if cell_count == 128 {
            u128::MAX
        } else {
            (1_u128 << cell_count) - 1
        }
    }
}

impl BoardLayoutBackend for Board128Layout {
    fn size(self) -> BoardSize {
        self.size()
    }

    fn backend_kind(self) -> BoardBackendKind {
        BoardBackendKind::Board128
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Board128LayoutError {
    InvalidBoardSize(BoardSizeError),
    TooFewCells { area: u32 },
    TooManyCells { area: u32 },
}

#[cfg(test)]
#[path = "board128_layout_tests.rs"]
mod tests;
