use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlacementEvent {
    step_index: usize,
    piece: PieceKind,
    rotation: RotationState,
    x: u16,
    y: u16,
    placed_mask: u64,
}

impl PlacementEvent {
    pub fn new(
        step_index: usize,
        piece: PieceKind,
        rotation: RotationState,
        x: u16,
        y: u16,
        placed_mask: u64,
    ) -> Self {
        Self {
            step_index,
            piece,
            rotation,
            x,
            y,
            placed_mask,
        }
    }
}
impl PlacementEvent {
    pub fn step_index(self) -> usize {
        self.step_index
    }
}
impl PlacementEvent {
    pub fn piece(self) -> PieceKind {
        self.piece
    }
}
impl PlacementEvent {
    pub fn rotation(self) -> RotationState {
        self.rotation
    }
}
impl PlacementEvent {
    pub fn x(self) -> u16 {
        self.x
    }
}
impl PlacementEvent {
    pub fn y(self) -> u16 {
        self.y
    }
}
impl PlacementEvent {
    pub fn placed_mask(self) -> u64 {
        self.placed_mask
    }
}
