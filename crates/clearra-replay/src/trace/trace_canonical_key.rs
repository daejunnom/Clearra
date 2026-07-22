use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::{
    layout::board64_layout::Board64Layout, placement::placement_mask::PlacementMask,
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use crate::{
    trace::{hold_decision::HoldDecision, solution_trace::SolutionTrace},
    ScoringExecutionEdge,
};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TraceCanonicalKey {
    steps: Vec<TraceStepCanonicalKey>,
}

impl TraceCanonicalKey {
    pub fn from_trace(trace: &SolutionTrace) -> Self {
        let steps = trace
            .steps()
            .iter()
            .map(|step| {
                let placement = step.placement();
                let decision = step.piece_decision();
                TraceStepCanonicalKey {
                    active_piece: decision.active_piece(),
                    input_cursor: decision.input_cursor(),
                    output_cursor: decision.output_cursor(),
                    input_hold_piece: decision.input_hold_piece(),
                    output_hold_piece: decision.output_hold_piece(),
                    hold_decision: decision.hold_decision(),
                    placed_piece: placement.piece_kind(),
                    rotation: placement.rotation(),
                    x: placement.x(),
                    y: placement.y(),
                    mask: placement.mask(),
                }
            })
            .collect();
        Self { steps }
    }

    pub fn from_scoring_path(
        layout: Board64Layout,
        edges: &[ScoringExecutionEdge],
        hold_decisions: &[HoldDecision],
    ) -> Option<Self> {
        if edges.len() != hold_decisions.len() {
            return None;
        }
        let registry = standard_tetromino_registry();
        let mut steps = Vec::with_capacity(edges.len());
        for (step_index, (edge, hold_decision)) in edges
            .iter()
            .copied()
            .zip(hold_decisions.iter().copied())
            .enumerate()
        {
            let definition = registry.get(edge.piece())?;
            let x = u16::try_from(edge.x()).ok()?;
            let y = u16::try_from(edge.y()).ok()?;
            let placement = PlacementMask::new(layout, definition, edge.rotation(), x, y).ok()?;
            steps.push(TraceStepCanonicalKey {
                active_piece: edge.piece(),
                input_cursor: step_index,
                output_cursor: step_index + 1,
                input_hold_piece: None,
                output_hold_piece: None,
                hold_decision,
                placed_piece: edge.piece(),
                rotation: edge.rotation(),
                x,
                y,
                mask: placement.mask(),
            });
        }
        Some(Self { steps })
    }
}
impl TraceCanonicalKey {
    pub fn len(&self) -> usize {
        self.steps.len()
    }
}
impl TraceCanonicalKey {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}
impl TraceCanonicalKey {
    pub fn stable_key(&self) -> String {
        let steps = self
            .steps
            .iter()
            .map(TraceStepCanonicalKey::stable_key)
            .collect::<Vec<_>>()
            .join("~");
        format!("trk1:{steps}")
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct TraceStepCanonicalKey {
    active_piece: PieceKind,
    input_cursor: usize,
    output_cursor: usize,
    input_hold_piece: Option<PieceKind>,
    output_hold_piece: Option<PieceKind>,
    hold_decision: HoldDecision,
    placed_piece: PieceKind,
    rotation: RotationState,
    x: u16,
    y: u16,
    mask: u64,
}

impl TraceStepCanonicalKey {
    fn stable_key(&self) -> String {
        format!(
            "a{}i{}o{}ih{}oh{}d{}p{}r{}x{}y{}m{:016x}",
            self.active_piece.as_ascii(),
            self.input_cursor,
            self.output_cursor,
            format_hold_piece(self.input_hold_piece),
            format_hold_piece(self.output_hold_piece),
            format_hold_decision(self.hold_decision),
            self.placed_piece.as_ascii(),
            self.rotation.quarter_turns(),
            self.x,
            self.y,
            self.mask,
        )
    }
}

fn format_hold_piece(piece: Option<PieceKind>) -> String {
    piece
        .map(|piece| piece.as_ascii().to_string())
        .unwrap_or_else(|| "none".to_owned())
}

fn format_hold_decision(decision: HoldDecision) -> String {
    match decision {
        HoldDecision::None => "none".to_owned(),
        HoldDecision::SwapWithHold {
            incoming_piece,
            held_piece,
        } => format!("swap{}{}", incoming_piece.as_ascii(), held_piece.as_ascii()),
        HoldDecision::StoreIncoming {
            stored_piece,
            drawn_piece,
        } => format!("store{}{}", stored_piece.as_ascii(), drawn_piece.as_ascii()),
        HoldDecision::ReleaseHeldAtTerminal { held_piece } => {
            format!("terminal{}", held_piece.as_ascii())
        }
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_geometry::{
        layout::board64_layout::Board64Layout, placement::placement_mask::PlacementMask,
    };
    use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

    use crate::{
        board::board64_state::Board64State,
        trace::{
            BoardAfterStep, HoldDecision, LineClearEvent, PieceDecision, PlacementStep,
            SolutionTrace,
        },
    };

    use super::*;

    #[test]
    fn canonical_key_is_equal_for_equivalent_trace_steps() {
        let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
        let registry = standard_tetromino_registry();
        let piece = registry.get(PieceKind::O).expect("O");
        let placement =
            PlacementMask::new(layout, piece, RotationState::Zero, 0, 0).expect("placement");
        let board = Board64State::empty(layout);
        let step = PlacementStep::new(
            0,
            PieceDecision::new(PieceKind::O, 0, 1, None, None, HoldDecision::None),
            placement,
            board,
            BoardAfterStep::new(board, board),
            LineClearEvent::new(0),
        );
        let first = SolutionTrace::new(vec![step]);
        let second = SolutionTrace::new(vec![step]);

        assert_eq!(
            TraceCanonicalKey::from_trace(&first),
            TraceCanonicalKey::from_trace(&second)
        );
        assert_eq!(
            TraceCanonicalKey::from_trace(&first).stable_key(),
            "trk1:aOi0o1ihnoneohnonednonepOr0x0y0m0000000000000c03"
        );
    }
}
