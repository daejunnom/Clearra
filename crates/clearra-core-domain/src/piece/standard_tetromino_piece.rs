use crate::{ids::piece_id::PieceDefinitionId, piece::piece_kind::PieceKind};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StandardTetrominoPiece {
    kind: PieceKind,
}

impl StandardTetrominoPiece {
    pub const AREA: usize = 4;
}
impl StandardTetrominoPiece {
    pub const fn new(kind: PieceKind) -> Self {
        Self { kind }
    }
}
impl StandardTetrominoPiece {
    pub fn all() -> [Self; 7] {
        PieceKind::STANDARD_TETROMINOES.map(Self::new)
    }
}
impl StandardTetrominoPiece {
    pub const fn kind(self) -> PieceKind {
        self.kind
    }
}
impl StandardTetrominoPiece {
    pub const fn area(self) -> usize {
        Self::AREA
    }
}
impl StandardTetrominoPiece {
    pub fn piece_definition_id(self) -> PieceDefinitionId {
        PieceDefinitionId::new(format!("std:{}", self.kind.as_ascii()))
    }
}

pub fn standard_tetromino_fast_path_unchanged() -> bool {
    StandardTetrominoPiece::all()
        .iter()
        .all(|piece| piece.area() == StandardTetrominoPiece::AREA)
}

#[cfg(test)]
#[path = "standard_tetromino_piece_tests.rs"]
mod tests;
