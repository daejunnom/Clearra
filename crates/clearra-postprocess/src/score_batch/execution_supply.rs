use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_replay::{ExactScoringExecutionBatch, HoldDecision, SpinCoverageExecutionBatch};
use clearra_supply::{
    execution_automaton::{
        SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionState,
        SupplyObservationIdentity, SupplyProvenanceId,
    },
    hold::hold_policy::HoldPolicy,
    piece_source::{PieceSourceId, PieceSourceKind},
    QueueObservationPolicy,
};

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
    let projected_terminal_cursor = sequence.len().checked_add(1);
    !batch.projects_unplaced_lookahead()
        || (usize::from(state.cursor) == sequence.len() && state.hold.is_none())
        || (projected_terminal_cursor == Some(usize::from(state.cursor)) && state.hold.is_some())
}

pub(super) fn for_each_supply_successor(
    batch: &impl ExecutionSupplyBatch,
    sequence: &[PieceKind],
    state: SupplyState,
    required_piece: PieceKind,
    mut visit: impl FnMut(HoldDecision, SupplyState) -> Result<(), ExactScoringExecutionCancelled>,
) -> Result<(), ExactScoringExecutionCancelled> {
    let cursor = usize::from(state.cursor);
    let Some(current) = available_piece(batch, sequence, cursor) else {
        return Ok(());
    };
    let execution_state = execution_state(batch, sequence, state);
    let automaton = SupplyExecutionAutomaton::sequence();

    if current == required_piece {
        emit_transition(
            &automaton,
            execution_state,
            state.node,
            SupplyBranchKind::Current,
            current,
            None,
            &mut visit,
        )?;
    }
    if !batch.hold_enabled() {
        return Ok(());
    }
    if state.hold == Some(required_piece) {
        emit_transition(
            &automaton,
            execution_state,
            state.node,
            SupplyBranchKind::SwapHeld,
            current,
            None,
            &mut visit,
        )?;
    }
    if state.hold.is_none() {
        if let Some(next) = cursor
            .checked_add(1)
            .and_then(|next_cursor| available_piece(batch, sequence, next_cursor))
        {
            if next == required_piece {
                emit_transition(
                    &automaton,
                    execution_state,
                    state.node,
                    SupplyBranchKind::StoreCurrent,
                    current,
                    Some(next),
                    &mut visit,
                )?;
            }
        }
    }
    Ok(())
}

fn emit_transition(
    automaton: &SupplyExecutionAutomaton,
    state: SupplyExecutionState,
    node: u32,
    branch_kind: SupplyBranchKind,
    current_piece: PieceKind,
    next_piece: Option<PieceKind>,
    visit: &mut impl FnMut(HoldDecision, SupplyState) -> Result<(), ExactScoringExecutionCancelled>,
) -> Result<(), ExactScoringExecutionCancelled> {
    let step = automaton
        .transition(state, branch_kind, current_piece, next_piece)
        .map_err(|_| ExactScoringExecutionCancelled)?;
    let hold_decision = match step.evidence.branch_kind {
        SupplyBranchKind::Current => HoldDecision::None,
        SupplyBranchKind::SwapHeld => HoldDecision::SwapWithHold {
            incoming_piece: current_piece,
            held_piece: step.used_piece,
        },
        SupplyBranchKind::StoreCurrent => HoldDecision::StoreIncoming {
            stored_piece: current_piece,
            drawn_piece: step.used_piece,
        },
    };
    visit(
        hold_decision,
        SupplyState {
            node,
            cursor: step.next_state.cursor,
            hold: step.next_state.hold_piece,
        },
    )
}

fn execution_state(
    batch: &impl ExecutionSupplyBatch,
    sequence: &[PieceKind],
    state: SupplyState,
) -> SupplyExecutionState {
    let source_identity = supply_identity(batch, sequence);
    let standard_bag = batch.projects_standard_bag_lookahead();
    let (bag_epoch, bag_remainder_key) = if standard_bag {
        standard_bag_state(batch, sequence, usize::from(state.cursor))
    } else {
        (0, 0)
    };
    SupplyExecutionState::with_contract(
        PieceSourceId::new(source_identity),
        if standard_bag {
            PieceSourceKind::BagUniverse
        } else {
            PieceSourceKind::FixedQueue
        },
        state.cursor,
        state.hold,
        if batch.hold_enabled() {
            HoldPolicy::Allowed
        } else {
            HoldPolicy::Forbidden
        },
        bag_epoch,
        bag_remainder_key,
        SupplyObservationIdentity::new(QueueObservationPolicy::FullQueueOracle, source_identity),
        SupplyProvenanceId(supply_provenance_identity(batch, source_identity)),
    )
}

fn available_piece(
    batch: &impl ExecutionSupplyBatch,
    sequence: &[PieceKind],
    index: usize,
) -> Option<PieceKind> {
    sequence.get(index).copied().or_else(|| {
        (batch.projects_unplaced_lookahead()
            && batch.projects_standard_bag_lookahead()
            && batch.hold_enabled()
            && index == sequence.len())
        .then(|| first_standard_bag_lookahead(sequence))
        .flatten()
    })
}

fn standard_bag_state(
    batch: &impl ExecutionSupplyBatch,
    sequence: &[PieceKind],
    cursor: usize,
) -> (u16, u64) {
    let bag_size = PieceKind::STANDARD_TETROMINOES.len();
    let (epoch, drawn_in_epoch) = if cursor == 0 {
        (0, 0)
    } else {
        ((cursor - 1) / bag_size, ((cursor - 1) % bag_size) + 1)
    };
    let epoch_start = cursor.saturating_sub(drawn_in_epoch);
    let mut counts = [1_u8; 7];
    for index in epoch_start..cursor {
        let Some(piece) = available_piece(batch, sequence, index) else {
            continue;
        };
        let slot = usize::from(piece_tag(piece) - 1);
        counts[slot] = counts[slot].saturating_sub(1);
    }
    let remainder_key = counts
        .into_iter()
        .enumerate()
        .fold(0_u64, |key, (index, count)| {
            key | (u64::from(count) << ((index + 1) * 4))
        });
    (u16::try_from(epoch).unwrap_or(u16::MAX), remainder_key)
}

fn supply_identity(batch: &impl ExecutionSupplyBatch, sequence: &[PieceKind]) -> u64 {
    let mut hash = fnv_seed();
    mix_u8(&mut hash, 0x51);
    mix_u8(&mut hash, batch.hold_enabled() as u8);
    mix_u8(&mut hash, batch.projects_unplaced_lookahead() as u8);
    mix_u8(&mut hash, batch.projects_standard_bag_lookahead() as u8);
    for piece in sequence.iter().copied() {
        mix_u8(&mut hash, piece_tag(piece));
    }
    if hash == 0 {
        1
    } else {
        hash
    }
}

fn supply_provenance_identity(batch: &impl ExecutionSupplyBatch, source_identity: u64) -> u64 {
    let mut hash = fnv_seed();
    mix_u8(&mut hash, 0xa7);
    for byte in source_identity.to_le_bytes() {
        mix_u8(&mut hash, byte);
    }
    mix_u8(&mut hash, batch.projects_unplaced_lookahead() as u8);
    mix_u8(&mut hash, batch.projects_standard_bag_lookahead() as u8);
    hash
}

pub(super) fn first_standard_bag_lookahead(sequence: &[PieceKind]) -> Option<PieceKind> {
    let used_in_current_bag = sequence.len() % PieceKind::STANDARD_TETROMINOES.len();
    if used_in_current_bag != PieceKind::STANDARD_TETROMINOES.len() - 1 {
        return None;
    }
    let current_bag_start = sequence.len() - used_in_current_bag;
    let mut missing = PieceKind::STANDARD_TETROMINOES.into_iter().filter(|piece| {
        !sequence[current_bag_start..]
            .iter()
            .any(|used| used == piece)
    });
    let piece = missing.next()?;
    missing.next().is_none().then_some(piece)
}

const fn piece_tag(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => 1,
        PieceKind::O => 2,
        PieceKind::T => 3,
        PieceKind::S => 4,
        PieceKind::Z => 5,
        PieceKind::J => 6,
        PieceKind::L => 7,
    }
}

const fn fnv_seed() -> u64 {
    14_695_981_039_346_656_037
}

fn mix_u8(hash: &mut u64, value: u8) {
    *hash ^= u64::from(value);
    *hash = hash.wrapping_mul(1_099_511_628_211);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct Batch {
        hold: bool,
        lookahead: bool,
        standard_bag: bool,
    }

    impl ExecutionSupplyBatch for Batch {
        fn hold_enabled(&self) -> bool {
            self.hold
        }

        fn projects_unplaced_lookahead(&self) -> bool {
            self.lookahead
        }

        fn projects_standard_bag_lookahead(&self) -> bool {
            self.standard_bag
        }
    }

    #[test]
    fn forward_successors_preserve_current_swap_and_store_behavior() {
        let batch = Batch {
            hold: true,
            lookahead: false,
            standard_bag: false,
        };
        let sequence = [PieceKind::I, PieceKind::O];

        let mut current = Vec::new();
        for_each_supply_successor(
            &batch,
            &sequence,
            SupplyState {
                node: 7,
                cursor: 0,
                hold: None,
            },
            PieceKind::I,
            |decision, next| {
                current.push((decision, next));
                Ok(())
            },
        )
        .expect("current branch");
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].0, HoldDecision::None);
        assert_eq!(current[0].1.cursor, 1);

        let mut swap = Vec::new();
        for_each_supply_successor(
            &batch,
            &sequence,
            SupplyState {
                node: 7,
                cursor: 0,
                hold: Some(PieceKind::T),
            },
            PieceKind::T,
            |decision, next| {
                swap.push((decision, next));
                Ok(())
            },
        )
        .expect("swap branch");
        assert_eq!(swap.len(), 1);
        assert_eq!(swap[0].1.hold, Some(PieceKind::I));

        let mut store = Vec::new();
        for_each_supply_successor(
            &batch,
            &sequence,
            SupplyState {
                node: 7,
                cursor: 0,
                hold: None,
            },
            PieceKind::O,
            |decision, next| {
                store.push((decision, next));
                Ok(())
            },
        )
        .expect("store branch");
        assert_eq!(store.len(), 1);
        assert_eq!(store[0].1.cursor, 2);
        assert_eq!(store[0].1.hold, Some(PieceKind::I));
    }

    #[test]
    fn forward_cursor_overflow_fails_closed() {
        let batch = Batch {
            hold: true,
            lookahead: false,
            standard_bag: false,
        };
        let result = emit_transition(
            &SupplyExecutionAutomaton::sequence(),
            SupplyExecutionState::with_contract(
                PieceSourceId::new(1),
                PieceSourceKind::FixedQueue,
                u16::MAX,
                None,
                HoldPolicy::Allowed,
                0,
                0,
                SupplyObservationIdentity::full_queue_oracle(),
                SupplyProvenanceId(1),
            ),
            0,
            SupplyBranchKind::Current,
            PieceKind::I,
            None,
            &mut |_, _| Ok(()),
        );
        assert_eq!(result, Err(ExactScoringExecutionCancelled));
        assert!(batch.hold_enabled());
    }
}
