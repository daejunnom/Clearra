use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ObservedQueue {
    pieces: Vec<PieceKind>,
}

impl ObservedQueue {
    pub fn new(pieces: Vec<PieceKind>) -> Self {
        Self { pieces }
    }
}
impl ObservedQueue {
    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }
}
impl ObservedQueue {
    pub fn len(&self) -> usize {
        self.pieces.len()
    }
}
impl ObservedQueue {
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }
}
