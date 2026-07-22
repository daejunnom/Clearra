use clearra_core_domain::board::board_size::BoardSize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardGeometry {
    size: BoardSize,
}

impl BoardGeometry {
    pub const fn new(size: BoardSize) -> Self {
        Self { size }
    }
}
impl BoardGeometry {
    pub fn size(self) -> BoardSize {
        self.size
    }
}
impl BoardGeometry {
    pub fn width(self) -> u16 {
        self.size.width()
    }
}
impl BoardGeometry {
    pub fn height(self) -> u16 {
        self.size.height()
    }
}
