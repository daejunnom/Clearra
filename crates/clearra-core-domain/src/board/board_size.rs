#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoardSize {
    width: u16,
    height: u16,
}

impl BoardSize {
    pub const fn standard_10x20() -> Self {
        Self {
            width: 10,
            height: 20,
        }
    }
}
impl BoardSize {
    pub fn new(width: u16, height: u16) -> Result<Self, BoardSizeError> {
        if width == 0 {
            return Err(BoardSizeError::ZeroWidth);
        }
        if height == 0 {
            return Err(BoardSizeError::ZeroHeight);
        }
        Ok(Self { width, height })
    }
}
impl BoardSize {
    pub fn width(self) -> u16 {
        self.width
    }
}
impl BoardSize {
    pub fn height(self) -> u16 {
        self.height
    }
}
impl BoardSize {
    pub fn area(self) -> u32 {
        u32::from(self.width) * u32::from(self.height)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardSizeError {
    ZeroWidth,
    ZeroHeight,
}

#[cfg(test)]
#[path = "board_size_tests.rs"]
mod tests;
