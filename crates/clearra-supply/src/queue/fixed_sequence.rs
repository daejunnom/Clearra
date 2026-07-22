use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FixedSequence {
    pieces: Vec<PieceKind>,
}

impl FixedSequence {
    pub fn new(pieces: Vec<PieceKind>) -> Self {
        Self { pieces }
    }
}
impl FixedSequence {
    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }
}
impl FixedSequence {
    pub fn into_pieces(self) -> Vec<PieceKind> {
        self.pieces
    }
}
impl FixedSequence {
    pub fn len(&self) -> usize {
        self.pieces.len()
    }
}
impl FixedSequence {
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }
}
