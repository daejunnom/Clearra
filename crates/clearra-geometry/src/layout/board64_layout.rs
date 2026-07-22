use clearra_core_domain::board::board_size::{BoardSize, BoardSizeError};

use super::board_backend::{BoardBackendKind, BoardLayoutBackend};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Board64Layout {
    size: BoardSize,
}

impl Board64Layout {
    pub fn new(size: BoardSize) -> Result<Self, Board64LayoutError> {
        let area = size.area();
        if area > 64 {
            return Err(Board64LayoutError::TooManyCells { area });
        }
        Ok(Self { size })
    }
}
impl Board64Layout {
    pub fn standard_10_by_lines(lines: u8) -> Result<Self, Board64LayoutError> {
        let size =
            BoardSize::new(10, u16::from(lines)).map_err(Board64LayoutError::InvalidBoardSize)?;
        Self::new(size)
    }
}
impl Board64Layout {
    pub fn size(self) -> BoardSize {
        self.size
    }
}
impl Board64Layout {
    pub fn width(self) -> u16 {
        self.size.width()
    }
}
impl Board64Layout {
    pub fn height(self) -> u16 {
        self.size.height()
    }
}
impl Board64Layout {
    pub fn cell_count(self) -> u8 {
        self.size.area() as u8
    }
}
impl Board64Layout {
    pub fn all_cells_mask(self) -> u64 {
        let cell_count = self.cell_count();
        if cell_count == 64 {
            u64::MAX
        } else {
            (1_u64 << cell_count) - 1
        }
    }
}

impl BoardLayoutBackend for Board64Layout {
    fn size(self) -> BoardSize {
        self.size()
    }

    fn backend_kind(self) -> BoardBackendKind {
        BoardBackendKind::Board64
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Board64LayoutError {
    InvalidBoardSize(BoardSizeError),
    TooManyCells { area: u32 },
}

#[cfg(test)]
#[path = "board64_layout_tests.rs"]
mod tests;
