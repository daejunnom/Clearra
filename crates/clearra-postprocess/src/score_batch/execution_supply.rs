use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_replay::{ExactScoringExecutionBatch, HoldDecision, SpinCoverageExecutionBatch};

use super::exact_scoring_execution_materializer::ExactScoringExecutionCancelled;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct SupplyState {
    pub node: u32,
    pub cursor: u16,
    pub hold: Option<PieceKind>,
}

pub(super) trait ExecutionSupplyBatch {
    fn hold_enabled(&self) -> bool;
    fn projects_unplaced_lookahead(&self) -> bool;
    fn projects_standard_bag_lookahead(&self) -> bool;
}

impl ExecutionSupplyBatch for ExactScoringExecutionBatch {
    fn hold_enabled(&self) -> bool {
        self.hold_enabled()
    }

    fn projects_unplaced_lookahead(&self) -> bool {
        self.projects_unplaced_lookahead()
    }

    fn projects_standard_bag_lookahead(&self) -> bool {
        self.projects_standard_bag_lookahead()
    }
}

impl ExecutionSupplyBatch for SpinCoverageExecutionBatch {
    fn hold_enabled(&self) -> bool {
        self.hold_enabled()
    }

    fn projects_unplaced_lookahead(&self) -> bool {
        self.projects_unplaced_lookahead()
    }

    fn projects_standard_bag_lookahead(&self) -> bool {
        self.projects_standard_bag_lookahead()
    }
}

pub(super) fn terminal_supply_state_is_accepted(
    batch: &impl ExecutionSupplyBatch,
    sequence: &[PieceKind],
    state: SupplyState,
) -> bool {
    !batch.projects_unplaced_lookahead()
        || (state.cursor as usize == sequence.len() && state.hold.is_none())
        || (state.cursor as usize == sequence.len().saturating_add(1) && state.hold.is_some())
}

pub(super) fn for_each_supply_successor(
    batch: &impl ExecutionSupplyBatch,
    sequence: &[PieceKind],
    state: SupplyState,
    required_piece: PieceKind,
    mut visit: impl FnMut(HoldDecision, SupplyState) -> Result<(), ExactScoringExecutionCancelled>,
) -> Result<(), ExactScoringExecutionCancelled> {
    let cursor = state.cursor as usize;
    let Some(current) = sequence.get(cursor).copied() else {
        if batch.projects_unplaced_lookahead()
            && batch.projects_standard_bag_lookahead()
            && batch.hold_enabled()
            && cursor == sequence.len()
        {
            if let Some(lookahead) = first_standard_bag_lookahead(sequence) {
                if lookahead == required_piece {
                    visit(
                        HoldDecision::None,
                        SupplyState {
                            cursor: state.cursor.saturating_add(1),
                            ..state
                        },
                    )?;
                }
                if state.hold == Some(required_piece) {
                    visit(
                        HoldDecision::SwapWithHold {
                            incoming_piece: lookahead,
                            held_piece: required_piece,
                        },
                        SupplyState {
                            cursor: state.cursor.saturating_add(1),
                            hold: Some(lookahead),
                            ..state
                        },
                    )?;
                }
            }
        }
        return Ok(());
    };
    if current == required_piece {
        visit(
            HoldDecision::None,
            SupplyState {
                cursor: state.cursor.saturating_add(1),
                ..state
            },
        )?;
    }
    if !batch.hold_enabled() {
        return Ok(());
    }
    if state.hold == Some(required_piece) {
        visit(
            HoldDecision::SwapWithHold {
                incoming_piece: current,
                held_piece: required_piece,
            },
            SupplyState {
                cursor: state.cursor.saturating_add(1),
                hold: Some(current),
                ..state
            },
        )?;
    }
    if state.hold.is_none() && sequence.get(cursor + 1).copied() == Some(required_piece) {
        visit(
            HoldDecision::StoreIncoming {
                stored_piece: current,
                drawn_piece: required_piece,
            },
            SupplyState {
                cursor: state.cursor.saturating_add(2),
                hold: Some(current),
                ..state
            },
        )?;
    }
    if state.hold.is_none()
        && batch.projects_unplaced_lookahead()
        && batch.projects_standard_bag_lookahead()
        && cursor + 1 == sequence.len()
        && first_standard_bag_lookahead(sequence) == Some(required_piece)
    {
        visit(
            HoldDecision::StoreIncoming {
                stored_piece: current,
                drawn_piece: required_piece,
            },
            SupplyState {
                cursor: state.cursor.saturating_add(2),
                hold: Some(current),
                ..state
            },
        )?;
    }
    Ok(())
}

pub(super) fn first_standard_bag_lookahead(sequence: &[PieceKind]) -> Option<PieceKind> {
    let used_in_current_bag = sequence.len() % PieceKind::STANDARD_TETROMINOES.len();
    if used_in_current_bag != PieceKind::STANDARD_TETROMINOES.len() - 1 {
        return None;
    }
    let current_bag_start = sequence.len().saturating_sub(used_in_current_bag);
    let mut missing = PieceKind::STANDARD_TETROMINOES.into_iter().filter(|piece| {
        !sequence[current_bag_start..]
            .iter()
            .any(|used| used == piece)
    });
    let piece = missing.next()?;
    missing.next().is_none().then_some(piece)
}
