use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::trace::hold_decision::HoldDecision;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceDecision {
    active_piece: PieceKind,
    input_cursor: usize,
    output_cursor: usize,
    input_hold_piece: Option<PieceKind>,
    output_hold_piece: Option<PieceKind>,
    hold_decision: HoldDecision,
}

impl PieceDecision {
    /// Projects an already selected supply transition; this does not decide
    /// whether a queue makes the transition available. The supply automaton
    /// remains that authority. In particular, terminal release is its existing
    /// projected-lookahead marker, not permission to draw from an empty queue.
    pub fn from_selected_hold(
        active_piece: PieceKind,
        input_cursor: usize,
        input_hold_piece: Option<PieceKind>,
        hold_decision: HoldDecision,
    ) -> Option<Self> {
        let (consumed, output_hold_piece) = match hold_decision {
            HoldDecision::None => (1, input_hold_piece),
            HoldDecision::SwapWithHold {
                incoming_piece,
                held_piece,
            } if input_hold_piece == Some(held_piece) && active_piece == held_piece => {
                (1, Some(incoming_piece))
            }
            HoldDecision::StoreIncoming {
                stored_piece,
                drawn_piece,
            } if input_hold_piece.is_none() && active_piece == drawn_piece => {
                (2, Some(stored_piece))
            }
            HoldDecision::ReleaseHeldAtTerminal { held_piece }
                if input_hold_piece == Some(held_piece) && active_piece == held_piece =>
            {
                (1, input_hold_piece)
            }
            _ => return None,
        };
        Some(Self::new(
            active_piece,
            input_cursor,
            input_cursor.checked_add(consumed)?,
            input_hold_piece,
            output_hold_piece,
            hold_decision,
        ))
    }

    pub fn new(
        active_piece: PieceKind,
        input_cursor: usize,
        output_cursor: usize,
        input_hold_piece: Option<PieceKind>,
        output_hold_piece: Option<PieceKind>,
        hold_decision: HoldDecision,
    ) -> Self {
        Self {
            active_piece,
            input_cursor,
            output_cursor,
            input_hold_piece,
            output_hold_piece,
            hold_decision,
        }
    }
}
impl PieceDecision {
    pub fn active_piece(self) -> PieceKind {
        self.active_piece
    }
}
impl PieceDecision {
    pub fn input_cursor(self) -> usize {
        self.input_cursor
    }
}
impl PieceDecision {
    pub fn output_cursor(self) -> usize {
        self.output_cursor
    }
}
impl PieceDecision {
    pub fn input_hold_piece(self) -> Option<PieceKind> {
        self.input_hold_piece
    }
}
impl PieceDecision {
    pub fn output_hold_piece(self) -> Option<PieceKind> {
        self.output_hold_piece
    }
}
impl PieceDecision {
    pub fn hold_decision(self) -> HoldDecision {
        self.hold_decision
    }
}
