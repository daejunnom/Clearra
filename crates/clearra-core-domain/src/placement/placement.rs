use crate::{
    board::cell::CellCoord,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Placement {
    piece: PieceKind,
    rotation: RotationState,
    origin: CellCoord,
}

impl Placement {
    pub fn new(piece: PieceKind, rotation: RotationState, origin: CellCoord) -> Self {
        Self {
            piece,
            rotation,
            origin,
        }
    }
}
impl Placement {
    pub fn piece(self) -> PieceKind {
        self.piece
    }
}
impl Placement {
    pub fn rotation(self) -> RotationState {
        self.rotation
    }
}
impl Placement {
    pub fn origin(self) -> CellCoord {
        self.origin
    }
}
