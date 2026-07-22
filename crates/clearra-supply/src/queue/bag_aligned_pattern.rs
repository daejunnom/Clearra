use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BagAlignedPattern {
    pieces: Vec<PieceKind>,
}

impl BagAlignedPattern {
    pub fn new(pieces: Vec<PieceKind>) -> Self {
        Self { pieces }
    }
}
impl BagAlignedPattern {
    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }
}
impl BagAlignedPattern {
    pub fn into_pieces(self) -> Vec<PieceKind> {
        self.pieces
    }
}
impl BagAlignedPattern {
    pub fn len(&self) -> usize {
        self.pieces.len()
    }
}
impl BagAlignedPattern {
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }
}
