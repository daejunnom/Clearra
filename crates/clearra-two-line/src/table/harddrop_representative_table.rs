use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HarddropRepresentative {
    piece: PieceKind,
    rotation: RotationState,
    x: i16,
}

impl HarddropRepresentative {
    pub fn new(piece: PieceKind, rotation: RotationState, x: i16) -> Self {
        Self { piece, rotation, x }
    }
}
impl HarddropRepresentative {
    pub fn piece(self) -> PieceKind {
        self.piece
    }
}
impl HarddropRepresentative {
    pub fn rotation(self) -> RotationState {
        self.rotation
    }
}
impl HarddropRepresentative {
    pub fn x(self) -> i16 {
        self.x
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HarddropRepresentativeTable {
    representatives: Vec<HarddropRepresentative>,
}

impl HarddropRepresentativeTable {
    pub fn new(representatives: Vec<HarddropRepresentative>) -> Self {
        Self { representatives }
    }
}
impl HarddropRepresentativeTable {
    pub fn representatives(&self) -> &[HarddropRepresentative] {
        &self.representatives
    }
}
