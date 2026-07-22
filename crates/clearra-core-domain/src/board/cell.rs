use super::board_size::BoardSize;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CellCoord {
    x: u16,
    y: u16,
}

impl CellCoord {
    pub const fn new_unchecked(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}
impl CellCoord {
    pub fn new(x: u16, y: u16, board_size: BoardSize) -> Result<Self, CellCoordError> {
        if x >= board_size.width() {
            return Err(CellCoordError::XOutOfBounds {
                x,
                width: board_size.width(),
            });
        }
        if y >= board_size.height() {
            return Err(CellCoordError::YOutOfBounds {
                y,
                height: board_size.height(),
            });
        }
        Ok(Self { x, y })
    }
}
impl CellCoord {
    pub fn x(self) -> u16 {
        self.x
    }
}
impl CellCoord {
    pub fn y(self) -> u16 {
        self.y
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CellCoordError {
    XOutOfBounds { x: u16, width: u16 },
    YOutOfBounds { y: u16, height: u16 },
}

#[cfg(test)]
#[path = "cell_tests.rs"]
mod tests;
