use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct QueuePattern {
    pieces: Vec<PieceKind>,
}

impl QueuePattern {
    pub fn new(pieces: Vec<PieceKind>) -> Self {
        Self { pieces }
    }
}
impl QueuePattern {
    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }
}
