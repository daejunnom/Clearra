// SRP: recognize the existential placement language of actual compact input
// storage, without enumerating reveal ordinals or assigning probability mass.
use core::{
    fmt,
    hash::{Hash, Hasher},
    num::NonZeroUsize,
};
use std::{collections::HashMap, sync::Arc};

use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    hold::hold_policy::HoldPolicy, QueueObservationPolicy, SupplyBranchKind,
    SupplyExecutionAutomaton, SupplyExecutionError, SupplyExecutionState,
};

use super::materialized_pattern_universe::{
    MaterializedPatternUniverse, UniformCompactPatternSource,
};

#[cfg(test)]
#[path = "compact_pattern_union_tests.rs"]
mod tests;

/// These are preparation/frontier work limits, not permission to truncate the
/// accepted language. Crossing a limit returns an error and no partial state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactPatternUnionLimits {
    source_pieces: NonZeroUsize,
    frontier_states: NonZeroUsize,
    transition_attempts: NonZeroUsize,
}

impl CompactPatternUnionLimits {
    pub const fn new(
        source_pieces: NonZeroUsize,
        frontier_states: NonZeroUsize,
        transition_attempts: NonZeroUsize,
    ) -> Self {
        Self {
            source_pieces,
            frontier_states,
            transition_attempts,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompactPatternUnionError {
    Cancelled,
    IncompleteSource,
    InconsistentSource,
    UnsupportedInitialState,
    UnsupportedObservation,
    SourcePieceLimit { limit: usize, attempted: usize },
    FrontierStateLimit { limit: usize, attempted: usize },
    TransitionLimit { limit: usize, attempted: usize },
    ForeignFrontier,
    PlacementDepthMismatch,
    CounterOverflow,
    AllocationFailed,
    Supply(SupplyExecutionError),
}

impl CompactPatternUnionError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::Cancelled => "compact_pattern_union_cancelled",
            Self::IncompleteSource => "compact_pattern_union_incomplete_source",
            Self::InconsistentSource => "compact_pattern_union_inconsistent_source",
            Self::UnsupportedInitialState => "compact_pattern_union_initial_state_unsupported",
            Self::UnsupportedObservation => "compact_pattern_union_observation_unsupported",
            Self::SourcePieceLimit { .. } => "compact_pattern_union_source_piece_limit",
            Self::FrontierStateLimit { .. } => "compact_pattern_union_frontier_state_limit",
            Self::TransitionLimit { .. } => "compact_pattern_union_transition_limit",
            Self::ForeignFrontier => "compact_pattern_union_foreign_frontier",
            Self::PlacementDepthMismatch => "compact_pattern_union_placement_depth_mismatch",
            Self::CounterOverflow => "compact_pattern_union_counter_overflow",
            Self::AllocationFailed => "compact_pattern_union_allocation_failed",
            Self::Supply(_) => "compact_pattern_union_supply_failed",
        }
    }
}

impl fmt::Display for CompactPatternUnionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.reason())
    }
}
impl std::error::Error for CompactPatternUnionError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DrawAtom {
    choices: u8,
    draws: u8,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct PlacementState {
    atom: u16,
    // `used` is in 0..=7 and hold has eight exact states (empty plus seven
    // tetrominoes). Packing both into one byte removes two alignment bytes
    // from every determinized supply state without quotienting histories.
    used_and_held: u8,
    remaining: u8,
    consumed: u16,
}

const PLACEMENT_USED_MASK: u8 = 0b0000_0111;
const PLACEMENT_HELD_SHIFT: u32 = 3;
const PLACEMENT_HELD_MASK: u8 = 0b0011_1000;
const _: [(); 6] = [(); core::mem::size_of::<PlacementState>()];

impl PlacementState {
    fn new(atom: u16, used: u8, remaining: u8, consumed: u16, held: Option<PieceKind>) -> Self {
        debug_assert!(used <= PLACEMENT_USED_MASK);
        Self {
            atom,
            used_and_held: used | (held_code(held) << PLACEMENT_HELD_SHIFT),
            remaining,
            consumed,
        }
    }

    const fn used(self) -> u8 {
        self.used_and_held & PLACEMENT_USED_MASK
    }

    fn set_used(&mut self, used: u8) {
        debug_assert!(used <= PLACEMENT_USED_MASK);
        self.used_and_held = (self.used_and_held & !PLACEMENT_USED_MASK) | used;
    }

    const fn held(self) -> Option<PieceKind> {
        held_from_code((self.used_and_held & PLACEMENT_HELD_MASK) >> PLACEMENT_HELD_SHIFT)
    }

    fn set_held(&mut self, held: Option<PieceKind>) {
        self.used_and_held =
            (self.used_and_held & !PLACEMENT_HELD_MASK) | (held_code(held) << PLACEMENT_HELD_SHIFT);
    }
}

#[derive(Debug)]
struct FrontierOwner {
    placed_pieces: usize,
}

/// A determinized set of supply states for one placement prefix, or an explicit
/// union of equal-depth prefixes whose future geometry the caller proved
/// equivalent. Distinct partial layouts/row frames remain distinct in that
/// caller. Clones are immutable inputs to independent transitions, so
/// cancellation cannot partially commit.
#[derive(Clone, Debug)]
pub struct CompactPatternUnionFrontier {
    // One language-owned token per depth. The pointer therefore carries both
    // the language identity and placement depth without retaining a separate
    // `usize` in every geometry frontier entry.
    owner: Arc<FrontierOwner>,
    // A completed frontier is immutable. Seal it to exact length so millions
    // of geometry entries do not retain Vec growth slack that can never be
    // used. The slice remains independently owned; only the depth/language
    // token is shared.
    states: Box<[PlacementState]>,
}

#[cfg(target_pointer_width = "64")]
const _: [(); 24] = [(); core::mem::size_of::<CompactPatternUnionFrontier>()];
#[cfg(target_pointer_width = "32")]
const _: [(); 12] = [(); core::mem::size_of::<CompactPatternUnionFrontier>()];

impl CompactPatternUnionFrontier {
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
    pub fn state_count(&self) -> usize {
        self.states.len()
    }
    pub fn placed_pieces(&self) -> usize {
        self.owner.placed_pieces
    }
    pub fn retained_state_capacity_bytes(&self) -> usize {
        self.states.len() * core::mem::size_of::<PlacementState>()
    }

    /// Returns the opaque owner token used by one exact placement-depth layer.
    /// The token does not expose or weaken the language/depth identity check;
    /// it only lets a graph layer retain that shared identity once instead of
    /// once per hash-table entry.
    pub fn layer_owner(&self) -> CompactPatternUnionLayerOwner {
        CompactPatternUnionLayerOwner {
            owner: Arc::clone(&self.owner),
        }
    }

    /// Fallible owned copy for callers that reserve the returned payload under
    /// a frontier-memory budget. A failed clone never changes the source.
    pub fn try_clone(&self) -> Result<Self, CompactPatternUnionError> {
        let mut states = Vec::new();
        states
            .try_reserve_exact(self.states.len())
            .map_err(|_| CompactPatternUnionError::AllocationFailed)?;
        states.extend_from_slice(&self.states);
        Ok(Self {
            owner: Arc::clone(&self.owner),
            states: states.into_boxed_slice(),
        })
    }
}

/// Opaque identity shared by every compact frontier at one language/depth.
///
/// This is deliberately not a constructor for frontiers. Payload ownership,
/// reattachment and foreign-owner rejection stay inside
/// [`CompactPatternUnionLayerShard`].
#[derive(Clone, Debug)]
pub struct CompactPatternUnionLayerOwner {
    owner: Arc<FrontierOwner>,
}

impl CompactPatternUnionLayerOwner {
    fn matches(&self, frontier: &CompactPatternUnionFrontier) -> bool {
        Arc::ptr_eq(&self.owner, &frontier.owner)
    }

    pub fn same_identity(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner)
    }
}

/// Borrowed exact supply frontier stored in a graph-layer shard.
///
/// Callers can inspect accounting and ask the owning language to merge it,
/// but cannot detach the payload from its owner or manufacture a frontier.
pub struct CompactPatternUnionLayerFrontierRef<'a> {
    owner: &'a Arc<FrontierOwner>,
    states: &'a [PlacementState],
}

impl CompactPatternUnionLayerFrontierRef<'_> {
    pub fn state_count(&self) -> usize {
        self.states.len()
    }

    pub fn retained_state_capacity_bytes(&self) -> usize {
        core::mem::size_of_val(self.states)
    }
}

/// One ownership shard for a breadth layer. The owner is retained once by the
/// surrounding layer while this table stores only exact state slices. On
/// 64-bit targets that reduces `(K, frontier)` by one pointer without changing
/// any accepted supply history, geometry key or merge rule.
pub struct CompactPatternUnionLayerShard<K> {
    entries: HashMap<K, Box<[PlacementState]>>,
}

impl<K> CompactPatternUnionLayerShard<K> {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub const fn entry_size() -> usize {
        core::mem::size_of::<(K, Box<[PlacementState]>)>()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn capacity(&self) -> usize {
        self.entries.capacity()
    }
}

impl<K: Eq + Hash> CompactPatternUnionLayerShard<K> {
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), CompactPatternUnionError> {
        self.entries
            .try_reserve(additional)
            .map_err(|_| CompactPatternUnionError::AllocationFailed)
    }

    pub fn get<'a>(
        &'a self,
        key: &K,
        owner: &'a CompactPatternUnionLayerOwner,
    ) -> Option<CompactPatternUnionLayerFrontierRef<'a>> {
        self.entries
            .get(key)
            .map(|states| CompactPatternUnionLayerFrontierRef {
                owner: &owner.owner,
                states,
            })
    }

    /// Inserts a frontier only when it belongs to the surrounding layer.
    /// Returns true when an existing key was replaced.
    pub fn insert(
        &mut self,
        key: K,
        frontier: CompactPatternUnionFrontier,
        owner: &CompactPatternUnionLayerOwner,
    ) -> Result<bool, CompactPatternUnionError> {
        if !owner.matches(&frontier) {
            return Err(CompactPatternUnionError::ForeignFrontier);
        }
        Ok(self.entries.insert(key, frontier.states).is_some())
    }

    pub fn into_iter(
        self,
        owner: CompactPatternUnionLayerOwner,
    ) -> CompactPatternUnionLayerIntoIter<K> {
        CompactPatternUnionLayerIntoIter {
            owner,
            entries: self.entries.into_iter(),
        }
    }
}

impl<K> Default for CompactPatternUnionLayerShard<K> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct CompactPatternUnionLayerIntoIter<K> {
    owner: CompactPatternUnionLayerOwner,
    entries: std::collections::hash_map::IntoIter<K, Box<[PlacementState]>>,
}

impl<K> Iterator for CompactPatternUnionLayerIntoIter<K> {
    type Item = (K, CompactPatternUnionFrontier);

    fn next(&mut self) -> Option<Self::Item> {
        self.entries.next().map(|(key, states)| {
            (
                key,
                CompactPatternUnionFrontier {
                    owner: Arc::clone(&self.owner.owner),
                    states,
                },
            )
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.entries.size_hint()
    }
}

impl<K> ExactSizeIterator for CompactPatternUnionLayerIntoIter<K> {}

// This key is only for an in-memory graph x supply memo in the same family.
// It is not a stable digest, wire identity, or a license to merge geometry
// histories. A source with equal numeric IDs still has a different owner.
impl PartialEq for CompactPatternUnionFrontier {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.owner, &other.owner) && self.states == other.states
    }
}
impl Eq for CompactPatternUnionFrontier {}
impl Hash for CompactPatternUnionFrontier {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::ptr::hash(Arc::as_ptr(&self.owner), state);
        self.states.hash(state);
    }
}

/// Input-language evidence ONLY. This accepts a placement prefix iff at least
/// one original visible queue and legal hold execution produces it. It does
/// not certify a PC, a complete graph traversal, coverage, replay provenance,
/// reveal weights or an online decision policy. Those retain their own owners.
///
/// Factorized atoms retain their actual boundaries/choice sets; e.g. P4P4 is
/// never approximated by a repeating seven-bag. Hidden suffix multiplicity is
/// irrelevant to existential union but remains in the original source owner.
#[derive(Clone, Debug)]
pub struct CompactPatternUnionLanguage {
    depth_owners: Arc<[Arc<FrontierOwner>]>,
    atoms: Arc<[DrawAtom]>,
    sequence_pieces: u16,
    source_pattern_count: usize,
    initial: SupplyExecutionState,
    limits: CompactPatternUnionLimits,
}

impl CompactPatternUnionLanguage {
    /// Conservative per-operation output reservation. A graph owner can keep
    /// its old payload live while advance/merge creates a replacement. It must
    /// still measure actual returned capacity before retaining that result.
    pub fn maximum_frontier_capacity_bytes(&self) -> Option<usize> {
        self.limits
            .frontier_states
            .get()
            .checked_mul(core::mem::size_of::<PlacementState>())
    }

    /// None means that the actual storage is not supported (explicit queues,
    /// observed bags or non-uniform weights), not an empty solution language.
    /// No expression text, descriptive structure label or numeric ID is used.
    pub fn prepare<G: Fn() -> bool>(
        universe: &MaterializedPatternUniverse,
        initial: SupplyExecutionState,
        limits: CompactPatternUnionLimits,
        cancelled: &G,
    ) -> Result<Option<(Self, CompactPatternUnionFrontier)>, CompactPatternUnionError> {
        check_cancelled(cancelled)?;
        if !universe.complete() || universe.truncation_reason().is_some() {
            return Err(CompactPatternUnionError::IncompleteSource);
        }
        if universe.pattern_count() == 0
            || universe.total_possible_pattern_count() != universe.pattern_count() as u128
        {
            return Err(CompactPatternUnionError::InconsistentSource);
        }
        if initial.observation.policy != QueueObservationPolicy::FullQueueOracle {
            return Err(CompactPatternUnionError::UnsupportedObservation);
        }
        if initial.cursor != 0
            || initial.hold_empty != initial.hold_piece.is_none()
            || (initial.hold_policy == HoldPolicy::Forbidden && initial.hold_piece.is_some())
        {
            return Err(CompactPatternUnionError::UnsupportedInitialState);
        }
        let Some(source) = universe.uniform_compact_source() else {
            return Ok(None);
        };
        let full_bag_key =
            crate::bag::bag_state::BagState::fresh_standard_7_bag().packed_remainder_key();
        if initial.bag_epoch != 0
            || (initial.bag_remainder_key != 0
                && (!matches!(source, UniformCompactPatternSource::Standard7Bag { .. })
                    || initial.bag_remainder_key != full_bag_key))
        {
            return Err(CompactPatternUnionError::UnsupportedInitialState);
        }
        let (atoms, visible, count) = compile_atoms(source, limits, cancelled)?;
        if count != universe.pattern_count() || visible == 0 || atoms.is_empty() {
            return Err(CompactPatternUnionError::InconsistentSource);
        }
        let sequence_pieces =
            u16::try_from(visible).map_err(|_| CompactPatternUnionError::CounterOverflow)?;
        let mut depth_owners = Vec::new();
        // One additional dead depth represents an attempted placement after
        // the visible source is exhausted. It is an exact empty language, not
        // a malformed frontier. Subsequent advances preserve that empty state.
        let owner_depths = visible
            .checked_add(2)
            .ok_or(CompactPatternUnionError::CounterOverflow)?;
        depth_owners
            .try_reserve_exact(owner_depths)
            .map_err(|_| CompactPatternUnionError::AllocationFailed)?;
        for placed_pieces in 0..owner_depths {
            depth_owners.push(Arc::new(FrontierOwner { placed_pieces }));
        }
        let depth_owners: Arc<[Arc<FrontierOwner>]> = depth_owners.into();
        let mut states = Vec::new();
        states
            .try_reserve_exact(1)
            .map_err(|_| CompactPatternUnionError::AllocationFailed)?;
        states.push(PlacementState::new(
            0,
            0,
            atoms[0].choices,
            0,
            initial.hold_piece,
        ));
        check_cancelled(cancelled)?;
        let frontier = CompactPatternUnionFrontier {
            owner: Arc::clone(&depth_owners[0]),
            states: states.into_boxed_slice(),
        };
        Ok(Some((
            Self {
                depth_owners,
                atoms: atoms.into(),
                sequence_pieces,
                source_pattern_count: count,
                initial,
                limits,
            },
            frontier,
        )))
    }

    pub const fn sequence_pieces(&self) -> usize {
        self.sequence_pieces as usize
    }
    pub const fn source_pattern_count(&self) -> usize {
        self.source_pattern_count
    }
    pub fn atom_count(&self) -> usize {
        self.atoms.len()
    }

    /// Existential union at one placement depth. Geometry owners may use this
    /// only after independently proving equality of their partial layout AND
    /// row frame. It does not combine probabilities or replay histories.
    pub fn merge<G: Fn() -> bool>(
        &self,
        left: &CompactPatternUnionFrontier,
        right: &CompactPatternUnionFrontier,
        cancelled: &G,
    ) -> Result<CompactPatternUnionFrontier, CompactPatternUnionError> {
        check_cancelled(cancelled)?;
        if left.placed_pieces() != right.placed_pieces() {
            return Err(CompactPatternUnionError::PlacementDepthMismatch);
        }
        if !Arc::ptr_eq(&left.owner, &right.owner)
            || self
                .depth_owners
                .get(left.placed_pieces())
                .is_none_or(|owner| !Arc::ptr_eq(owner, &left.owner))
        {
            return Err(CompactPatternUnionError::ForeignFrontier);
        }
        self.merge_states(&left.owner, &left.states, &right.states, cancelled)
    }

    /// Exact union against a frontier retained in a layer shard. This is the
    /// same operation as [`Self::merge`]; only the repeated owner pointer has
    /// moved from every hash entry to the surrounding layer.
    pub fn merge_layer<G: Fn() -> bool>(
        &self,
        left: CompactPatternUnionLayerFrontierRef<'_>,
        right: &CompactPatternUnionFrontier,
        cancelled: &G,
    ) -> Result<CompactPatternUnionFrontier, CompactPatternUnionError> {
        check_cancelled(cancelled)?;
        if left.owner.placed_pieces != right.placed_pieces() {
            return Err(CompactPatternUnionError::PlacementDepthMismatch);
        }
        if !Arc::ptr_eq(left.owner, &right.owner)
            || self
                .depth_owners
                .get(left.owner.placed_pieces)
                .is_none_or(|owner| !Arc::ptr_eq(owner, left.owner))
        {
            return Err(CompactPatternUnionError::ForeignFrontier);
        }
        self.merge_states(left.owner, left.states, &right.states, cancelled)
    }

    fn merge_states<G: Fn() -> bool>(
        &self,
        owner: &Arc<FrontierOwner>,
        left: &[PlacementState],
        right: &[PlacementState],
        cancelled: &G,
    ) -> Result<CompactPatternUnionFrontier, CompactPatternUnionError> {
        check_cancelled(cancelled)?;
        let mut states = Vec::new();
        let limit = self.limits.frontier_states.get();
        states
            .try_reserve_exact(left.len().saturating_add(right.len()).min(limit))
            .map_err(|_| CompactPatternUnionError::AllocationFailed)?;
        let (mut l, mut r) = (0, 0);
        while l < left.len() || r < right.len() {
            check_cancelled(cancelled)?;
            let item = match (left.get(l), right.get(r)) {
                (Some(a), Some(b)) if a == b => {
                    l += 1;
                    r += 1;
                    *a
                }
                (Some(a), Some(b)) if a < b => {
                    l += 1;
                    *a
                }
                (Some(_), Some(b)) | (None, Some(b)) => {
                    r += 1;
                    *b
                }
                (Some(a), None) => {
                    l += 1;
                    *a
                }
                (None, None) => unreachable!("loop has a remaining state"),
            };
            if states.len() == limit {
                return Err(CompactPatternUnionError::FrontierStateLimit {
                    limit,
                    attempted: limit.saturating_add(1),
                });
            }
            states.push(item);
        }
        check_cancelled(cancelled)?;
        Ok(CompactPatternUnionFrontier {
            owner: Arc::clone(owner),
            states: states.into_boxed_slice(),
        })
    }

    /// At most one next frontier is produced per desired graph-piece label.
    /// Ambiguous queue/hold histories are unioned, not emitted as more graph
    /// paths. Supply's existing transition function remains the hold authority.
    pub fn advance<G: Fn() -> bool>(
        &self,
        frontier: &CompactPatternUnionFrontier,
        desired: PieceKind,
        cancelled: &G,
    ) -> Result<CompactPatternUnionFrontier, CompactPatternUnionError> {
        check_cancelled(cancelled)?;
        if self
            .depth_owners
            .get(frontier.placed_pieces())
            .is_none_or(|owner| !Arc::ptr_eq(owner, &frontier.owner))
        {
            return Err(CompactPatternUnionError::ForeignFrontier);
        }
        if frontier.is_empty() {
            return frontier.try_clone();
        }
        let placed_pieces = frontier
            .placed_pieces()
            .checked_add(1)
            .ok_or(CompactPatternUnionError::CounterOverflow)?;
        let owner = self
            .depth_owners
            .get(placed_pieces)
            .ok_or(CompactPatternUnionError::PlacementDepthMismatch)?;
        let mut states = Vec::new();
        let mut attempts = 0usize;
        for state in frontier.states.iter() {
            check_cancelled(cancelled)?;
            let held = state.held();
            let execution = SupplyExecutionState {
                cursor: state.consumed,
                hold_piece: held,
                hold_empty: held.is_none(),
                ..self.initial
            };
            for current in PieceKind::STANDARD_TETROMINOES {
                check_cancelled(cancelled)?;
                let Some(after_current) = self.draw(*state, current) else {
                    continue;
                };
                if current == desired && execution.hold_policy != HoldPolicy::Required {
                    self.append_step(
                        &mut states,
                        &mut attempts,
                        execution,
                        SupplyBranchKind::Current,
                        current,
                        None,
                        after_current,
                    )?;
                }
                if execution.hold_policy == HoldPolicy::Forbidden {
                    continue;
                }
                if held == Some(desired) {
                    self.append_step(
                        &mut states,
                        &mut attempts,
                        execution,
                        SupplyBranchKind::SwapHeld,
                        current,
                        None,
                        after_current,
                    )?;
                } else if held.is_none() {
                    if let Some(after_next) = self.draw(after_current, desired) {
                        self.append_step(
                            &mut states,
                            &mut attempts,
                            execution,
                            SupplyBranchKind::StoreCurrent,
                            current,
                            Some(desired),
                            after_next,
                        )?;
                    }
                }
            }
        }
        check_cancelled(cancelled)?;
        Ok(CompactPatternUnionFrontier {
            owner: Arc::clone(owner),
            states: states.into_boxed_slice(),
        })
    }

    fn draw(&self, mut state: PlacementState, piece: PieceKind) -> Option<PlacementState> {
        if state.consumed >= self.sequence_pieces || state.remaining & piece_bit(piece) == 0 {
            return None;
        }
        let atom = self.atoms.get(usize::from(state.atom))?;
        state.remaining &= !piece_bit(piece);
        let used = state.used() + 1;
        state.set_used(used);
        state.consumed += 1;
        if used == atom.draws {
            state.atom += 1;
            state.set_used(0);
            state.remaining = self
                .atoms
                .get(usize::from(state.atom))
                .map_or(0, |next| next.choices);
        }
        Some(state)
    }

    #[allow(clippy::too_many_arguments)]
    fn append_step(
        &self,
        states: &mut Vec<PlacementState>,
        attempts: &mut usize,
        execution: SupplyExecutionState,
        kind: SupplyBranchKind,
        current: PieceKind,
        next: Option<PieceKind>,
        mut state: PlacementState,
    ) -> Result<(), CompactPatternUnionError> {
        *attempts = attempts
            .checked_add(1)
            .ok_or(CompactPatternUnionError::CounterOverflow)?;
        if *attempts > self.limits.transition_attempts.get() {
            return Err(CompactPatternUnionError::TransitionLimit {
                limit: self.limits.transition_attempts.get(),
                attempted: *attempts,
            });
        }
        let step = SupplyExecutionAutomaton::sequence()
            .transition(execution, kind, current, next)
            .map_err(CompactPatternUnionError::Supply)?;
        if step.next_state.cursor != state.consumed {
            return Err(CompactPatternUnionError::InconsistentSource);
        }
        state.set_held(step.next_state.hold_piece);
        let Err(index) = states.binary_search(&state) else {
            return Ok(());
        };
        let attempted = states.len() + 1;
        if attempted > self.limits.frontier_states.get() {
            return Err(CompactPatternUnionError::FrontierStateLimit {
                limit: self.limits.frontier_states.get(),
                attempted,
            });
        }
        if states.len() == states.capacity() {
            let capacity = states
                .capacity()
                .saturating_mul(2)
                .max(8)
                .min(self.limits.frontier_states.get());
            states
                .try_reserve_exact(capacity - states.len())
                .map_err(|_| CompactPatternUnionError::AllocationFailed)?;
        }
        states.insert(index, state);
        Ok(())
    }
}

fn compile_atoms<G: Fn() -> bool>(
    source: UniformCompactPatternSource<'_>,
    limits: CompactPatternUnionLimits,
    cancelled: &G,
) -> Result<(Vec<DrawAtom>, usize, usize), CompactPatternUnionError> {
    let (full, visible, count, atom_count) = match source {
        UniformCompactPatternSource::Standard7Bag {
            sequence_len,
            pattern_count,
        } => (
            sequence_len,
            sequence_len,
            pattern_count,
            sequence_len.div_ceil(7),
        ),
        UniformCompactPatternSource::FactorizedExpression(shape) => (
            shape.full_sequence_len(),
            shape.visible_sequence_len(),
            shape.pattern_count(),
            shape.atoms().len(),
        ),
    };
    let limit = limits.source_pieces.get().min(usize::from(u16::MAX));
    if full > limit {
        return Err(CompactPatternUnionError::SourcePieceLimit {
            limit,
            attempted: full,
        });
    }
    if full == 0 || visible == 0 || visible > full || atom_count == 0 || atom_count > full {
        return Err(CompactPatternUnionError::InconsistentSource);
    }
    let mut atoms = Vec::new();
    atoms
        .try_reserve_exact(atom_count)
        .map_err(|_| CompactPatternUnionError::AllocationFailed)?;
    match source {
        UniformCompactPatternSource::Standard7Bag { .. } => {
            let mut remaining = full;
            while remaining != 0 {
                check_cancelled(cancelled)?;
                let draws = remaining.min(7);
                atoms.push(DrawAtom {
                    choices: 0x7f,
                    draws: draws as u8,
                });
                remaining -= draws;
            }
        }
        UniformCompactPatternSource::FactorizedExpression(shape) => {
            for atom in shape.atoms() {
                check_cancelled(cancelled)?;
                let mut choices = 0u8;
                for piece in atom.choices() {
                    let bit = piece_bit(*piece);
                    if choices & bit != 0 {
                        return Err(CompactPatternUnionError::InconsistentSource);
                    }
                    choices |= bit;
                }
                let choice_count = choices.count_ones() as usize;
                let draws = atom.draw_count();
                if draws == 0
                    || draws > choice_count
                    || permutation_count(choice_count, draws)? != atom.variant_count()
                {
                    return Err(CompactPatternUnionError::InconsistentSource);
                }
                atoms.push(DrawAtom {
                    choices,
                    draws: draws as u8,
                });
            }
        }
    }
    let mut actual_count = 1usize;
    let mut actual_length = 0usize;
    for atom in &atoms {
        check_cancelled(cancelled)?;
        actual_count = actual_count
            .checked_mul(permutation_count(
                atom.choices.count_ones() as usize,
                usize::from(atom.draws),
            )?)
            .ok_or(CompactPatternUnionError::CounterOverflow)?;
        actual_length += usize::from(atom.draws);
    }
    if actual_count != count || actual_length != full {
        return Err(CompactPatternUnionError::InconsistentSource);
    }
    check_cancelled(cancelled)?;
    Ok((atoms, visible, count))
}

fn permutation_count(choices: usize, draws: usize) -> Result<usize, CompactPatternUnionError> {
    (0..draws).try_fold(1usize, |count, index| {
        count
            .checked_mul(choices - index)
            .ok_or(CompactPatternUnionError::CounterOverflow)
    })
}

fn check_cancelled<G: Fn() -> bool>(cancelled: &G) -> Result<(), CompactPatternUnionError> {
    if cancelled() {
        Err(CompactPatternUnionError::Cancelled)
    } else {
        Ok(())
    }
}

const fn piece_bit(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => 1,
        PieceKind::O => 2,
        PieceKind::T => 4,
        PieceKind::S => 8,
        PieceKind::Z => 16,
        PieceKind::J => 32,
        PieceKind::L => 64,
    }
}

const fn held_code(piece: Option<PieceKind>) -> u8 {
    match piece {
        None => 0,
        Some(PieceKind::I) => 1,
        Some(PieceKind::O) => 2,
        Some(PieceKind::T) => 3,
        Some(PieceKind::S) => 4,
        Some(PieceKind::Z) => 5,
        Some(PieceKind::J) => 6,
        Some(PieceKind::L) => 7,
    }
}

const fn held_from_code(code: u8) -> Option<PieceKind> {
    match code {
        0 => None,
        1 => Some(PieceKind::I),
        2 => Some(PieceKind::O),
        3 => Some(PieceKind::T),
        4 => Some(PieceKind::S),
        5 => Some(PieceKind::Z),
        6 => Some(PieceKind::J),
        7 => Some(PieceKind::L),
        _ => unreachable!(),
    }
}
