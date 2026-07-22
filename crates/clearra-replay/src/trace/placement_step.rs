use clearra_core_domain::operation::operation::OperationId;
use clearra_geometry::placement::placement_mask::PlacementMask;

use crate::{
    board::board64_state::Board64State,
    trace::{
        board_after_step::BoardAfterStep, line_clear_event::LineClearEvent,
        piece_decision::PieceDecision,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlacementStep {
    step_index: usize,
    operation_id: OperationId,
    piece_decision: PieceDecision,
    placement: PlacementMask,
    board_before: Board64State,
    board_after: BoardAfterStep,
    line_clear: LineClearEvent,
}

impl PlacementStep {
    pub fn new(
        step_index: usize,
        piece_decision: PieceDecision,
        placement: PlacementMask,
        board_before: Board64State,
        board_after: BoardAfterStep,
        line_clear: LineClearEvent,
    ) -> Self {
        Self {
            step_index,
            operation_id: OperationId(u16::try_from(step_index).unwrap_or(u16::MAX)),
            piece_decision,
            placement,
            board_before,
            board_after,
            line_clear,
        }
    }
}
impl PlacementStep {
    pub fn with_operation_id(mut self, operation_id: OperationId) -> Self {
        self.operation_id = operation_id;
        self
    }
}
impl PlacementStep {
    pub fn step_index(self) -> usize {
        self.step_index
    }
}
impl PlacementStep {
    pub fn operation_id(self) -> OperationId {
        self.operation_id
    }
}
impl PlacementStep {
    pub fn piece_decision(self) -> PieceDecision {
        self.piece_decision
    }
}
impl PlacementStep {
    pub fn placement(self) -> PlacementMask {
        self.placement
    }
}
impl PlacementStep {
    pub fn board_before(self) -> Board64State {
        self.board_before
    }
}
impl PlacementStep {
    pub fn board_after(self) -> BoardAfterStep {
        self.board_after
    }
}
impl PlacementStep {
    pub fn line_clear(self) -> LineClearEvent {
        self.line_clear
    }
}
impl PlacementStep {
    pub fn with_step_index(self, step_index: usize) -> Self {
        Self { step_index, ..self }
    }
}
