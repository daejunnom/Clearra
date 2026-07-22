use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PieceSetProfileId {
    StandardTetrominoes,
}

impl PieceSetProfileId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StandardTetrominoes => "standard-tetrominoes",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceSetProfile {
    id: PieceSetProfileId,
    pieces: &'static [PieceKind],
}

impl PieceSetProfile {
    pub const fn new(id: PieceSetProfileId, pieces: &'static [PieceKind]) -> Self {
        Self { id, pieces }
    }
}
impl PieceSetProfile {
    pub fn id(self) -> PieceSetProfileId {
        self.id
    }
}
impl PieceSetProfile {
    pub fn pieces(self) -> &'static [PieceKind] {
        self.pieces
    }
}
impl PieceSetProfile {
    pub fn len(self) -> usize {
        self.pieces.len()
    }
}
impl PieceSetProfile {
    pub fn is_empty(self) -> bool {
        self.pieces.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piece_set_profile_ids_expose_stable_canonical_strings() {
        assert_eq!(
            PieceSetProfileId::StandardTetrominoes.as_str(),
            "standard-tetrominoes"
        );
    }
}
