#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AreaModel {
    occupied_cells: u8,
}

impl AreaModel {
    pub fn from_mask(mask: u64) -> Self {
        Self {
            occupied_cells: mask.count_ones() as u8,
        }
    }
}
impl AreaModel {
    pub fn new(occupied_cells: u8) -> Self {
        Self { occupied_cells }
    }
}
impl AreaModel {
    pub fn occupied_cells(self) -> u8 {
        self.occupied_cells
    }
}
impl AreaModel {
    pub fn is_tetromino_tileable(self) -> bool {
        self.occupied_cells % 4 == 0
    }
}
impl AreaModel {
    pub fn tetromino_count(self) -> Option<u8> {
        self.is_tetromino_tileable()
            .then_some(self.occupied_cells / 4)
    }
}
