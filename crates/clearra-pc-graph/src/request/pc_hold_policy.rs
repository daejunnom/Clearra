use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_supply::hold::hold_slot::HoldSlot;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PcHoldPolicy {
    Disabled,
    #[default]
    EnabledEmpty,
    EnabledWithPiece(PieceKind),
}

impl PcHoldPolicy {
    pub fn enabled_from_slot(slot: HoldSlot) -> Self {
        match slot {
            HoldSlot::Empty => Self::EnabledEmpty,
            HoldSlot::Occupied(piece) => Self::EnabledWithPiece(piece),
        }
    }
}
impl PcHoldPolicy {
    pub fn is_enabled(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}
impl PcHoldPolicy {
    pub fn initial_slot(self) -> Option<HoldSlot> {
        match self {
            Self::Disabled => None,
            Self::EnabledEmpty => Some(HoldSlot::Empty),
            Self::EnabledWithPiece(piece) => Some(HoldSlot::Occupied(piece)),
        }
    }
}
impl PcHoldPolicy {
    pub fn initial_piece(self) -> Option<PieceKind> {
        self.initial_slot().and_then(HoldSlot::piece)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pc_hold_policy_tracks_enabled_state_and_initial_piece() {
        let policy = PcHoldPolicy::enabled_from_slot(HoldSlot::Occupied(PieceKind::T));

        assert!(policy.is_enabled());
        assert_eq!(policy.initial_piece(), Some(PieceKind::T));
        assert_eq!(PcHoldPolicy::Disabled.initial_slot(), None);
    }
}
