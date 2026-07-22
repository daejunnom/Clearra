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
