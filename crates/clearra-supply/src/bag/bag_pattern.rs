use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BagPattern {
    pieces: Vec<PieceKind>,
}

impl BagPattern {
    pub fn new(pieces: Vec<PieceKind>) -> Result<Self, BagPatternError> {
        if pieces.is_empty() {
            return Err(BagPatternError::Empty);
        }
        for (index, piece) in pieces.iter().enumerate() {
            if pieces[..index].contains(piece) {
                return Err(BagPatternError::DuplicatePiece { piece: *piece });
            }
        }
        Ok(Self { pieces })
    }
}
impl BagPattern {
    pub fn standard_7() -> Self {
        Self::new(PieceKind::STANDARD_TETROMINOES.to_vec()).expect("standard bag is unique")
    }
}
impl BagPattern {
    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }
}
impl BagPattern {
    pub fn len(&self) -> usize {
        self.pieces.len()
    }
}
impl BagPattern {
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BagPatternError {
    Empty,
    DuplicatePiece { piece: PieceKind },
}
