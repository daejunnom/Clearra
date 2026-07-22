use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum HoldSlot {
    #[default]
    Empty,
    Occupied(PieceKind),
}

impl HoldSlot {
    pub fn is_empty(self) -> bool {
        matches!(self, Self::Empty)
    }
}
impl HoldSlot {
    pub fn piece(self) -> Option<PieceKind> {
        match self {
            Self::Empty => None,
            Self::Occupied(piece) => Some(piece),
        }
    }
}
