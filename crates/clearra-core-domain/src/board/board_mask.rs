use super::{board_size::BoardSize, cell::CellCoord};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BoardMask(u64);

impl BoardMask {
    pub const EMPTY: Self = Self(0);
}
impl BoardMask {
    pub const fn from_bits(bits: u64) -> Self {
        Self(bits)
    }
}
impl BoardMask {
    pub fn bits(self) -> u64 {
        self.0
    }
}
impl BoardMask {
    pub fn from_cell(cell: CellCoord, board_size: BoardSize) -> Result<Self, BoardMaskError> {
        let index = cell_index(cell, board_size)?;
        Ok(Self(1_u64 << index))
    }
}
impl BoardMask {
    pub fn contains(self, cell: CellCoord, board_size: BoardSize) -> bool {
        cell_index(cell, board_size)
            .map(|index| (self.0 & (1_u64 << index)) != 0)
            .unwrap_or(false)
    }
}
impl BoardMask {
    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}
impl BoardMask {
    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}
impl BoardMask {
    pub fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}
impl BoardMask {
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}
impl BoardMask {
    pub fn count_ones(self) -> u32 {
        self.0.count_ones()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardMaskError {
    BoardTooLarge { area: u32 },
    CellOutOfMaskRange { index: u32 },
}

fn cell_index(cell: CellCoord, board_size: BoardSize) -> Result<u32, BoardMaskError> {
    let area = board_size.area();
    if area > 64 {
        return Err(BoardMaskError::BoardTooLarge { area });
    }
    let index = u32::from(cell.y()) * u32::from(board_size.width()) + u32::from(cell.x());
    if index >= 64 {
        return Err(BoardMaskError::CellOutOfMaskRange { index });
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_mask_from_cell() {
        let size = BoardSize::new(10, 6).expect("size");
        let cell = CellCoord::new(1, 2, size).expect("cell");
        let mask = BoardMask::from_cell(cell, size).expect("mask");

        assert!(mask.contains(cell, size));
        assert_eq!(mask.count_ones(), 1);
    }
}
