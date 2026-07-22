use super::{board_mask::BoardMask, board_size::BoardSize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardState {
    size: BoardSize,
    occupied: BoardMask,
}

impl BoardState {
    pub fn empty(size: BoardSize) -> Self {
        Self {
            size,
            occupied: BoardMask::EMPTY,
        }
    }
}
impl BoardState {
    pub fn new(size: BoardSize, occupied: BoardMask) -> Self {
        Self { size, occupied }
    }
}
impl BoardState {
    pub fn size(self) -> BoardSize {
        self.size
    }
}
impl BoardState {
    pub fn occupied(self) -> BoardMask {
        self.occupied
    }
}
impl BoardState {
    pub fn with_occupied(self, occupied: BoardMask) -> Self {
        Self {
            size: self.size,
            occupied,
        }
    }
}
impl BoardState {
    pub fn is_empty(self) -> bool {
        self.occupied.is_empty()
    }
}
