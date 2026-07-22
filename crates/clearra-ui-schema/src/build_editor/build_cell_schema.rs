use clearra_core_domain::board::cell::CellCoord;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildCellSchema {
    x: u16,
    y: u16,
}

impl BuildCellSchema {
    pub fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}
impl BuildCellSchema {
    pub fn from_coord(coord: CellCoord) -> Self {
        Self::new(coord.x(), coord.y())
    }
}
impl BuildCellSchema {
    pub fn x(self) -> u16 {
        self.x
    }
}
impl BuildCellSchema {
    pub fn y(self) -> u16 {
        self.y
    }
}
