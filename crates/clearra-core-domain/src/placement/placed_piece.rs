use crate::{board::board_mask::BoardMask, placement::placement::Placement};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlacedPiece {
    placement: Placement,
    occupied: BoardMask,
}

impl PlacedPiece {
    pub fn new(placement: Placement, occupied: BoardMask) -> Self {
        Self {
            placement,
            occupied,
        }
    }
}
impl PlacedPiece {
    pub fn placement(self) -> Placement {
        self.placement
    }
}
impl PlacedPiece {
    pub fn occupied(self) -> BoardMask {
        self.occupied
    }
}
