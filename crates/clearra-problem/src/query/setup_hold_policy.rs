use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_supply::hold::hold_slot::HoldSlot;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SetupHoldPolicy {
    Disabled,
    #[default]
    EnabledEmpty,
    EnabledWithPiece(PieceKind),
}

impl SetupHoldPolicy {
    pub fn enabled_from_slot(slot: HoldSlot) -> Self {
        match slot {
            HoldSlot::Empty => Self::EnabledEmpty,
            HoldSlot::Occupied(piece) => Self::EnabledWithPiece(piece),
        }
    }
}
impl SetupHoldPolicy {
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}
impl SetupHoldPolicy {
    pub fn initial_slot(self) -> Option<HoldSlot> {
        match self {
            Self::Disabled => None,
            Self::EnabledEmpty => Some(HoldSlot::Empty),
            Self::EnabledWithPiece(piece) => Some(HoldSlot::Occupied(piece)),
        }
    }
}
impl SetupHoldPolicy {
    pub fn initial_piece(self) -> Option<PieceKind> {
        self.initial_slot().and_then(HoldSlot::piece)
    }
}
