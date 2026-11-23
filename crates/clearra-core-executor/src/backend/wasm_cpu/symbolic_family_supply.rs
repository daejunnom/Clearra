//! Exact original-source relations for the direct partial-placement family.
//!
//! A source chooses one variable per original queue position. Variables below
//! `row_count` belong to placements and are never projected by this module.
//! Source guards retain the entire joint set, including hidden original draws.
//! Compact permutation segments are compiled by remaining-kind masks, without
//! visiting source ordinals. Unsupported source representations remain distinct
//! from an empty language.

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_pc_graph::request::PcCountPolicy;
use clearra_problem::SearchProblem;
use clearra_supply::bag::bag_state::BagState;
use clearra_supply::execution_automaton::{
    SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionError, SupplyExecutionState,
};
use clearra_supply::hold::hold_policy::HoldPolicy;
use clearra_supply::pattern_universe::materialized_pattern_universe::UniformCompactPatternSource;
use clearra_supply::pattern_universe::{
    FactorizedPatternLayout, FactorizedPatternSegment, PieceMultisetKey,
};
use clearra_supply::piece_source::{PieceSource, PieceSourceKind};
use clearra_supply::QueueObservationPolicy;

use super::exact_collections::ExactHashMap;
use super::geometry_partial_family::{ZddArena, ZddError, ZddId};
use super::terminal_hold_projection::finite_terminal_release_allowed;

const PIECE_COUNT: u32 = 7;
const FULL_BAG: u8 = 0x7f;
const MAX_TRANSITIONS: usize = 8;

type MultisetRoots = ExactHashMap<(PieceMultisetKey, SymbolicSupplyState), ZddId>;

#[derive(Debug)]
pub(super) enum SymbolicSupplyError {
    Unsupported(&'static str),
    InvalidSource(&'static str),
    Allocation,
    Zdd(ZddError),
    Execution(SupplyExecutionError),
}

impl From<ZddError> for SymbolicSupplyError {
    fn from(error: ZddError) -> Self {
        Self::Zdd(error)
    }
}

impl From<SupplyExecutionError> for SymbolicSupplyError {
    fn from(error: SupplyExecutionError) -> Self {
        Self::Execution(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct SymbolicSupplyState {
    cursor: u16,
    hold: Option<PieceKind>,
    terminal_projection_consumed: bool,
}

impl SymbolicSupplyState {
    pub(super) const fn cursor(self) -> u16 {
        self.cursor
    }

    pub(super) const fn hold_piece(self) -> Option<PieceKind> {
        self.hold
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SymbolicSupplyTransition {
    pub state: SymbolicSupplyState,
    pub root: ZddId,
}

#[derive(Clone, Copy)]
struct SupplyContract {
    initial: SupplyExecutionState,
    hold_enabled: bool,
    projects_unplaced_lookahead: bool,
    projects_standard_bag_lookahead: bool,
    visible_len: usize,
}

struct CompiledSource {
    root: ZddId,
    full_len: usize,
    visible_len: usize,
    standard_bag: bool,
    residual_states: usize,
}

pub(super) struct SymbolicFamilySupply {
    root: ZddId,
    row_count: u32,
    variable_count: u32,
    source_count: u128,
    residual_states: usize,
    contract: SupplyContract,
}

impl SymbolicFamilySupply {
    pub(super) fn compile(
        problem: &SearchProblem,
        row_count: u32,
        arena: &mut ZddArena,
    ) -> Result<Self, SymbolicSupplyError> {
        if problem.queue_observation_policy() != QueueObservationPolicy::FullQueueOracle {
            return Err(SymbolicSupplyError::Unsupported("queue_observation_policy"));
        }
        if problem.count_policy() != PcCountPolicy::CountUnique
            || problem.objective().score().requested()
            || problem.objective().execution_constraints().requested()
        {
            return Err(SymbolicSupplyError::Unsupported("non_boolean_execution"));
        }
        let supply = problem.supply();
        let contract = SupplyContract {
            initial: problem.initial_hold(),
            hold_enabled: supply.hold_enabled(),
            projects_unplaced_lookahead: supply.projects_unplaced_lookahead(),
            projects_standard_bag_lookahead: supply.projects_standard_bag_lookahead(),
            visible_len: supply.source_sequence_length(),
        };
        Self::compile_source(problem.piece_source(), contract, row_count, arena)
    }

    fn compile_source(
        source: &PieceSource,
        contract: SupplyContract,
        row_count: u32,
        arena: &mut ZddArena,
    ) -> Result<Self, SymbolicSupplyError> {
        validate_contract(source, contract)?;
        let universe = source
            .materialized_universe()
            .ok_or(SymbolicSupplyError::Unsupported("missing_source_universe"))?;
        if !universe.complete()
            || universe.truncation_reason().is_some()
            || universe.total_possible_pattern_count() != universe.pattern_count() as u128
        {
            return Err(SymbolicSupplyError::Unsupported("incomplete_source"));
        }
        let compiled = if let Some(layout) = universe.complete_factorized_layout() {
            compile_factorized(arena, row_count, layout)?
        } else if let Some(UniformCompactPatternSource::Standard7Bag {
            sequence_len,
            pattern_count,
        }) = universe.uniform_compact_source()
        {
            if pattern_count != universe.pattern_count() {
                return Err(SymbolicSupplyError::InvalidSource("standard_source_count"));
            }
            compile_standard_bag(arena, row_count, sequence_len)?
        } else if universe.pattern_count() == 1 {
            // One actual sequence is one input, not an ordinal enumeration.
            // This also admits a fixed explicit pattern after source projection.
            if universe.weight_at(0) != ProbabilityValue::ONE {
                return Err(SymbolicSupplyError::Unsupported("nonuniform_source"));
            }
            let sequence = universe
                .try_sequence_at(0)
                .ok_or(SymbolicSupplyError::InvalidSource("single_source_missing"))?;
            if source
                .fixed_sequence()
                .is_some_and(|fixed| fixed.pieces() != sequence.as_ref())
            {
                return Err(SymbolicSupplyError::InvalidSource("fixed_source_mismatch"));
            }
            compile_single(arena, row_count, sequence.as_ref())?
        } else {
            return Err(SymbolicSupplyError::Unsupported(
                "source_has_no_compact_unique_queue_relation",
            ));
        };
        if compiled.visible_len != contract.visible_len {
            return Err(SymbolicSupplyError::InvalidSource("visible_source_length"));
        }
        let expected_remainder = if compiled.standard_bag {
            BagState::fresh_standard_7_bag().packed_remainder_key()
        } else {
            0
        };
        if contract.initial.bag_remainder_key() != expected_remainder {
            return Err(SymbolicSupplyError::Unsupported("initial_bag_remainder"));
        }
        if contract.projects_standard_bag_lookahead && !compiled.standard_bag {
            return Err(SymbolicSupplyError::InvalidSource(
                "standard_lookahead_without_standard_source",
            ));
        }
        let source_count = universe.pattern_count() as u128;
        if arena.count(compiled.root)? != source_count {
            return Err(SymbolicSupplyError::InvalidSource("source_identity_count"));
        }
        Ok(Self {
            root: compiled.root,
            row_count,
            variable_count: checked_variable_count(row_count, compiled.full_len)?,
            source_count,
            residual_states: compiled.residual_states,
            contract,
        })
    }

    pub(super) const fn root(&self) -> ZddId {
        self.root
    }

    /// Replace this root only with its exact image from the arena's GC map.
    /// Joint layer roots and every other surviving root must be remapped too.
    pub(super) fn remap_root(&mut self, root: ZddId) {
        self.root = root;
    }

    pub(super) const fn variable_count(&self) -> u32 {
        self.variable_count
    }

    pub(super) const fn row_count(&self) -> u32 {
        self.row_count
    }

    pub(super) const fn visible_length(&self) -> usize {
        self.contract.visible_len
    }

    pub(super) fn source_variables(&self) -> core::ops::Range<u32> {
        self.row_count..self.variable_count
    }

    pub(super) const fn source_count(&self) -> u128 {
        self.source_count
    }

    pub(super) const fn compiled_residual_states(&self) -> usize {
        self.residual_states
    }

    pub(super) fn initial_state(&self) -> SymbolicSupplyState {
        SymbolicSupplyState {
            cursor: self.contract.initial.cursor(),
            hold: self.contract.initial.hold_piece(),
            terminal_projection_consumed: false,
        }
    }

    pub(super) fn is_accepting(&self, state: SymbolicSupplyState) -> bool {
        let cursor = usize::from(state.cursor);
        !self.contract.projects_unplaced_lookahead
            || (cursor == self.contract.visible_len && state.hold.is_none())
            || (Some(cursor) == self.contract.visible_len.checked_add(1) && state.hold.is_some())
    }

    /// Computes exact attainable inventories without reading source ordinals.
    ///
    /// This preparation owns all live arena roots except `self.root`: callers
    /// must invoke it before retaining geometry or layer roots. Each rank GC
    /// preserves the source plus the next residual inventory/state roots; the
    /// final GC preserves only the source. On error `self.root` remains valid,
    /// including after any previously completed compactions.
    pub(super) fn reachable_multisets(
        &mut self,
        arena: &mut ZddArena,
        placed_piece_count: usize,
    ) -> Result<Vec<PieceMultisetKey>, SymbolicSupplyError> {
        if placed_piece_count > usize::from(u8::MAX) {
            return Err(SymbolicSupplyError::Unsupported("multiset_rank_range"));
        }
        let mut current = MultisetRoots::default();
        current
            .try_reserve(1)
            .map_err(|_| SymbolicSupplyError::Allocation)?;
        current.insert(
            (PieceMultisetKey::default(), self.initial_state()),
            self.root,
        );
        for rank in 0..placed_piece_count {
            let mut next = MultisetRoots::default();
            for (&(key, state), &root) in &current {
                for (piece_index, desired) in
                    PieceKind::STANDARD_TETROMINOES.into_iter().enumerate()
                {
                    arena.checkpoint()?;
                    let transitions = self.successors(
                        arena,
                        root,
                        state,
                        desired,
                        rank + 1 == placed_piece_count,
                    )?;
                    if transitions.is_empty() {
                        continue;
                    }
                    let mut counts = key.counts();
                    counts[piece_index] = counts[piece_index].checked_add(1).ok_or(
                        SymbolicSupplyError::InvalidSource("multiset_count_overflow"),
                    )?;
                    // The rank check bounds the total too, so from_counts cannot
                    // exercise its saturating representation on this path.
                    let next_key = PieceMultisetKey::from_counts(counts);
                    for transition in transitions {
                        let entry = (next_key, transition.state);
                        if let Some(existing) = next.get_mut(&entry) {
                            *existing = arena.union(*existing, transition.root)?;
                        } else {
                            next.try_reserve(1)
                                .map_err(|_| SymbolicSupplyError::Allocation)?;
                            next.insert(entry, transition.root);
                        }
                    }
                }
            }
            drop(current);
            self.compact_multiset_roots(arena, &mut next)?;
            current = next;
            if current.is_empty() {
                break;
            }
        }

        let mut keys = Vec::new();
        keys.try_reserve_exact(current.len())
            .map_err(|_| SymbolicSupplyError::Allocation)?;
        for (&(key, state), &root) in &current {
            arena.checkpoint()?;
            if root != ZddId::EMPTY
                && usize::from(key.total_count()) == placed_piece_count
                && self.is_accepting(state)
            {
                keys.push(key);
            }
        }
        keys.sort_unstable();
        keys.dedup();
        drop(current);
        let mut roots = [self.root];
        arena.compact(&mut roots)?;
        self.root = roots[0];
        Ok(keys)
    }

    fn compact_multiset_roots(
        &mut self,
        arena: &mut ZddArena,
        next: &mut MultisetRoots,
    ) -> Result<(), SymbolicSupplyError> {
        let capacity = next
            .len()
            .checked_add(1)
            .ok_or(SymbolicSupplyError::Allocation)?;
        let mut roots = Vec::new();
        roots
            .try_reserve_exact(capacity)
            .map_err(|_| SymbolicSupplyError::Allocation)?;
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(next.len())
            .map_err(|_| SymbolicSupplyError::Allocation)?;
        roots.push(self.root);
        for root in next.values_mut() {
            roots.push(*root);
            slots.push(root);
        }
        arena.compact(&mut roots)?;
        self.root = roots[0];
        for (slot, root) in slots.into_iter().zip(roots.into_iter().skip(1)) {
            *slot = root;
        }
        Ok(())
    }

    /// `joint_root` must descend from this source root in the same arena. Every
    /// result retains both the original queue variables and placement variables.
    pub(super) fn successors(
        &self,
        arena: &mut ZddArena,
        joint_root: ZddId,
        state: SymbolicSupplyState,
        desired: PieceKind,
        terminal_step: bool,
    ) -> Result<Vec<SymbolicSupplyTransition>, SymbolicSupplyError> {
        let mut transitions = Vec::new();
        if joint_root == ZddId::EMPTY || state.terminal_projection_consumed {
            return Ok(transitions);
        }
        transitions
            .try_reserve_exact(MAX_TRANSITIONS)
            .map_err(|_| SymbolicSupplyError::Allocation)?;
        let cursor = usize::from(state.cursor);
        if state.hold == Some(desired)
            && !self.has_projected_current()
            && finite_terminal_release_allowed(
                self.contract.visible_len,
                cursor,
                self.contract.hold_enabled,
                self.contract.projects_unplaced_lookahead,
                state.terminal_projection_consumed,
                state.hold.is_some(),
                terminal_step,
            )
        {
            transitions.push(SymbolicSupplyTransition {
                state: SymbolicSupplyState {
                    cursor: state.cursor,
                    hold: None,
                    terminal_projection_consumed: true,
                },
                root: joint_root,
            });
            return Ok(transitions);
        }

        let current_root = self.select_piece(arena, joint_root, cursor, desired)?;
        if current_root != ZddId::EMPTY {
            let next = self.sequence_step(state, SupplyBranchKind::Current, desired, None)?;
            merge_transition(arena, &mut transitions, next, current_root)?;
        }
        if !self.contract.hold_enabled {
            return Ok(transitions);
        }
        if state.hold == Some(desired) {
            for current in PieceKind::STANDARD_TETROMINOES {
                // Current already reaches this exact Boolean state when equal.
                if current == desired {
                    continue;
                }
                let root = self.select_piece(arena, joint_root, cursor, current)?;
                if root == ZddId::EMPTY {
                    continue;
                }
                let next = self.sequence_step(state, SupplyBranchKind::SwapHeld, current, None)?;
                merge_transition(arena, &mut transitions, next, root)?;
            }
        } else if state.hold.is_none() {
            let next_cursor = cursor
                .checked_add(1)
                .ok_or(SymbolicSupplyError::InvalidSource("source_cursor_overflow"))?;
            let next_root = self.select_piece(arena, joint_root, next_cursor, desired)?;
            for current in PieceKind::STANDARD_TETROMINOES {
                let root = self.select_piece(arena, next_root, cursor, current)?;
                if root == ZddId::EMPTY {
                    continue;
                }
                let next = self.sequence_step(
                    state,
                    SupplyBranchKind::StoreCurrent,
                    current,
                    Some(desired),
                )?;
                merge_transition(arena, &mut transitions, next, root)?;
            }
        }
        Ok(transitions)
    }

    fn sequence_step(
        &self,
        state: SymbolicSupplyState,
        branch: SupplyBranchKind,
        current: PieceKind,
        next: Option<PieceKind>,
    ) -> Result<SymbolicSupplyState, SymbolicSupplyError> {
        // The queue relation owns bag/segment constraints. The shared sequence
        // automaton owns Current/Swap/Store semantics, not a fresh bag model.
        let input = SupplyExecutionState {
            cursor: state.cursor,
            hold_piece: state.hold,
            hold_empty: state.hold.is_none(),
            ..self.contract.initial
        };
        let step = SupplyExecutionAutomaton::sequence().transition(input, branch, current, next)?;
        Ok(SymbolicSupplyState {
            cursor: step.next_state.cursor(),
            hold: step.next_state.hold_piece(),
            terminal_projection_consumed: state.terminal_projection_consumed,
        })
    }

    fn has_projected_current(&self) -> bool {
        self.contract.hold_enabled
            && self.contract.projects_unplaced_lookahead
            && self.contract.projects_standard_bag_lookahead
            && self.contract.visible_len % PIECE_COUNT as usize == PIECE_COUNT as usize - 1
    }

    fn select_piece(
        &self,
        arena: &mut ZddArena,
        mut root: ZddId,
        position: usize,
        piece: PieceKind,
    ) -> Result<ZddId, SymbolicSupplyError> {
        if root == ZddId::EMPTY {
            return Ok(root);
        }
        if position < self.contract.visible_len {
            return Ok(arena.select(
                root,
                queue_variable(self.row_count, position, piece)?,
                true,
            )?);
        }
        if position != self.contract.visible_len || !self.has_projected_current() {
            return Ok(ZddId::EMPTY);
        }
        // The seventh piece is a function of the six actual original draws.
        // It adds no original variable or ordinal, and cannot start a fresh bag.
        for prior in position - (PIECE_COUNT as usize - 1)..position {
            root = arena.select(root, queue_variable(self.row_count, prior, piece)?, false)?;
        }
        Ok(root)
    }
}

fn validate_contract(
    source: &PieceSource,
    contract: SupplyContract,
) -> Result<(), SymbolicSupplyError> {
    let initial = contract.initial;
    if source.kind() == PieceSourceKind::ObservedWindow {
        return Err(SymbolicSupplyError::Unsupported("observed_source"));
    }
    if initial.piece_source_id() != source.id()
        || initial.source_kind() != source.kind()
        || initial.provenance().0 != source.provenance().supply_provenance_id()
    {
        return Err(SymbolicSupplyError::InvalidSource("source_owner_mismatch"));
    }
    if initial.observation().policy != QueueObservationPolicy::FullQueueOracle {
        return Err(SymbolicSupplyError::Unsupported(
            "source_observation_policy",
        ));
    }
    let expected_policy = if contract.hold_enabled {
        HoldPolicy::Allowed
    } else {
        HoldPolicy::Forbidden
    };
    if initial.cursor() != 0
        || initial.bag_epoch() != 0
        || initial.hold_empty() != initial.hold_piece().is_none()
        || initial.hold_policy() != expected_policy
        || (!contract.hold_enabled && initial.hold_piece().is_some())
    {
        return Err(SymbolicSupplyError::Unsupported("initial_supply_state"));
    }
    if contract.visible_len >= usize::from(u16::MAX) {
        return Err(SymbolicSupplyError::Unsupported("source_cursor_range"));
    }
    Ok(())
}

fn compile_single(
    arena: &mut ZddArena,
    row_count: u32,
    sequence: &[PieceKind],
) -> Result<CompiledSource, SymbolicSupplyError> {
    checked_variable_count(row_count, sequence.len())?;
    let mut root = ZddId::UNIT;
    for (position, &piece) in sequence.iter().enumerate().rev() {
        root = arena.add(root, queue_variable(row_count, position, piece)?)?;
    }
    Ok(CompiledSource {
        root,
        full_len: sequence.len(),
        visible_len: sequence.len(),
        standard_bag: false,
        residual_states: sequence.len(),
    })
}

fn compile_factorized(
    arena: &mut ZddArena,
    row_count: u32,
    layout: FactorizedPatternLayout<'_>,
) -> Result<CompiledSource, SymbolicSupplyError> {
    checked_variable_count(row_count, layout.full_sequence_len())?;
    let mut segments: Vec<FactorizedPatternSegment> = Vec::new();
    segments
        .try_reserve_exact(layout.segment_count())
        .map_err(|_| SymbolicSupplyError::Allocation)?;
    segments.extend(layout.segments());
    let mut root = ZddId::UNIT;
    let mut residual_states = 0;
    for segment in segments.into_iter().rev() {
        root = SegmentCompiler::new(
            arena,
            row_count,
            segment.draw_start(),
            segment.draw_count(),
            segment.choice_mask(),
            root,
            &mut residual_states,
        )
        .compile(segment.choice_mask())?;
    }
    Ok(CompiledSource {
        root,
        full_len: layout.full_sequence_len(),
        visible_len: layout.visible_sequence_len(),
        standard_bag: false,
        residual_states,
    })
}

fn compile_standard_bag(
    arena: &mut ZddArena,
    row_count: u32,
    sequence_len: usize,
) -> Result<CompiledSource, SymbolicSupplyError> {
    checked_variable_count(row_count, sequence_len)?;
    let mut root = ZddId::UNIT;
    let mut end = sequence_len;
    let mut residual_states = 0;
    while end != 0 {
        let start = (end - 1) / PIECE_COUNT as usize * PIECE_COUNT as usize;
        root = SegmentCompiler::new(
            arena,
            row_count,
            start,
            end - start,
            FULL_BAG,
            root,
            &mut residual_states,
        )
        .compile(FULL_BAG)?;
        end = start;
    }
    Ok(CompiledSource {
        root,
        full_len: sequence_len,
        visible_len: sequence_len,
        standard_bag: true,
        residual_states,
    })
}

struct SegmentCompiler<'a> {
    arena: &'a mut ZddArena,
    row_count: u32,
    start: usize,
    draws: usize,
    choice_count: u32,
    tail: ZddId,
    memo: [Option<ZddId>; 128],
    residual_states: &'a mut usize,
}

impl<'a> SegmentCompiler<'a> {
    fn new(
        arena: &'a mut ZddArena,
        row_count: u32,
        start: usize,
        draws: usize,
        choices: u8,
        tail: ZddId,
        residual_states: &'a mut usize,
    ) -> Self {
        Self {
            arena,
            row_count,
            start,
            draws,
            choice_count: choices.count_ones(),
            tail,
            memo: [None; 128],
            residual_states,
        }
    }

    fn compile(&mut self, remaining: u8) -> Result<ZddId, SymbolicSupplyError> {
        let used = (self.choice_count - remaining.count_ones()) as usize;
        if used == self.draws {
            return Ok(self.tail);
        }
        if let Some(root) = self.memo[usize::from(remaining)] {
            return Ok(root);
        }
        *self.residual_states =
            self.residual_states
                .checked_add(1)
                .ok_or(SymbolicSupplyError::InvalidSource(
                    "source_residual_overflow",
                ))?;
        let mut result = ZddId::EMPTY;
        let mut choices = remaining;
        while choices != 0 {
            let index = choices.trailing_zeros() as usize;
            let bit = 1_u8 << index;
            choices &= !bit;
            let suffix = self.compile(remaining & !bit)?;
            let piece = PieceKind::STANDARD_TETROMINOES[index];
            let variable = queue_variable(self.row_count, self.start + used, piece)?;
            let branch = self.arena.add(suffix, variable)?;
            result = self.arena.union(result, branch)?;
        }
        self.memo[usize::from(remaining)] = Some(result);
        Ok(result)
    }
}

fn checked_variable_count(row_count: u32, source_len: usize) -> Result<u32, SymbolicSupplyError> {
    u32::try_from(source_len)
        .ok()
        .and_then(|length| length.checked_mul(PIECE_COUNT))
        .and_then(|count| row_count.checked_add(count))
        .ok_or(SymbolicSupplyError::Unsupported("source_variable_range"))
}

fn queue_variable(
    row_count: u32,
    position: usize,
    piece: PieceKind,
) -> Result<u32, SymbolicSupplyError> {
    let code = match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    };
    checked_variable_count(row_count, position)?
        .checked_add(code)
        .ok_or(SymbolicSupplyError::Unsupported("source_variable_range"))
}

fn merge_transition(
    arena: &mut ZddArena,
    transitions: &mut Vec<SymbolicSupplyTransition>,
    state: SymbolicSupplyState,
    root: ZddId,
) -> Result<(), SymbolicSupplyError> {
    if let Some(existing) = transitions.iter_mut().find(|entry| entry.state == state) {
        existing.root = arena.union(existing.root, root)?;
    } else {
        if transitions.len() == transitions.capacity() {
            transitions
                .try_reserve_exact(1)
                .map_err(|_| SymbolicSupplyError::Allocation)?;
        }
        transitions.push(SymbolicSupplyTransition { state, root });
    }
    Ok(())
}

#[cfg(test)]
#[path = "symbolic_family_supply_tests.rs"]
mod tests;
