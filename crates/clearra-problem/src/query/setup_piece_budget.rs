use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PieceBudget {
    allowed_pieces: Vec<PieceKind>,
    max_piece_count: u8,
}

impl PieceBudget {
    pub fn new(
        allowed_pieces: Vec<PieceKind>,
        max_piece_count: u8,
    ) -> Result<Self, PieceBudgetError> {
        if allowed_pieces.is_empty() {
            return Err(PieceBudgetError::EmptyAllowedPieces);
        }
        if max_piece_count == 0 {
            return Err(PieceBudgetError::ZeroMaxPieceCount);
        }

        for (index, piece) in allowed_pieces.iter().enumerate() {
            if allowed_pieces[..index].contains(piece) {
                return Err(PieceBudgetError::DuplicateAllowedPiece { piece: *piece });
            }
        }

        Ok(Self {
            allowed_pieces,
            max_piece_count,
        })
    }
}
impl PieceBudget {
    pub fn standard_7_bag(max_piece_count: u8) -> Self {
        Self::new(PieceKind::STANDARD_TETROMINOES.to_vec(), max_piece_count)
            .expect("standard tetromino set is non-empty and unique")
    }
}
impl PieceBudget {
    pub fn allowed_pieces(&self) -> &[PieceKind] {
        &self.allowed_pieces
    }
}
impl PieceBudget {
    pub fn max_piece_count(&self) -> u8 {
        self.max_piece_count
    }
}
impl PieceBudget {
    pub fn allows(&self, piece: PieceKind) -> bool {
        self.allowed_pieces.contains(&piece)
    }
}

impl Default for PieceBudget {
    fn default() -> Self {
        Self::standard_7_bag(7)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PieceBudgetError {
    EmptyAllowedPieces,
    DuplicateAllowedPiece { piece: PieceKind },
    ZeroMaxPieceCount,
}
