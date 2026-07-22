use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HoldDecision {
    None,
    SwapWithHold {
        incoming_piece: PieceKind,
        held_piece: PieceKind,
    },
    StoreIncoming {
        stored_piece: PieceKind,
        drawn_piece: PieceKind,
    },
    ReleaseHeldAtTerminal {
        held_piece: PieceKind,
    },
}

impl HoldDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::SwapWithHold { .. } => "swap-with-hold",
            Self::StoreIncoming { .. } => "store-incoming",
            Self::ReleaseHeldAtTerminal { .. } => "release-held-at-terminal",
        }
    }
}
