//! Exact, bounded two-stage search over one continuous queue and board.
//!
//! A stage-two token may be locked before stage one has cleared, but never
//! appears twice in the supply. Every lock is replayed on the same board with
//! the actual hold state. The search keeps stage provenance through line
//! clears, so a nonempty borrowed piece does not masquerade as a PC.

use std::collections::HashSet;

use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_replay::ScoringExecutionEdge;
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::{
    b2b_preservation::BackToBackPreservationPolicy,
    event::SpinDetector,
    profile::{SpinProfile, SpinProfileId},
};

use crate::{
    board::{place_and_clear, ForwardBoard},
    reachability::ReachabilityWorkspace,
    search::t_corner_counts,
};

// Five stage-one 7-bags plus the adjacent stage-two bag remain addressable.
// Search still has a finite state budget and reports exhaustion as incomplete.
const MAX_QUEUE_PIECES: usize = 42;
const MAX_SEARCH_STATES: usize = 1_000_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryRecoveryQuery {
    pub initial_board: Board256Mask,
    pub final_board: Board256Mask,
    pub height: u8,
    pub queue: Vec<PieceKind>,
    /// The first `stage_one_queue_len` supply tokens belong to stage one.
    pub stage_one_queue_len: usize,
    /// Number of locks needed to reach the declared second-stage target.
    /// Remaining queue tokens are lookahead, not silently consumed.
    pub required_placements: usize,
    /// Empty means unconstrained geometry. Otherwise each entry is one
    /// four-cell lock-time role independent of the source token that fills it.
    pub placement_role_masks: Vec<Board256Mask>,
    /// Empty uses the fixed reference queue as the role-piece catalog. Pattern
    /// adapters retain reference roles while permuting supply tokens.
    pub placement_role_pieces: Vec<PieceKind>,
    /// Zero runs only the normal connection proof; one permits the selected
    /// stage-two placement to be locked before stage-one cleanup.
    pub max_early_placements: u8,
    /// The selected stage-two role allowed before stage-one cleanup.
    pub borrow_role_index: usize,
    /// Exact lock-time geometry of that selected early placement. It may
    /// compact to different cells after the stage-one line clear.
    pub borrow_placement_mask: Board256Mask,
    pub hold_enabled: bool,
    pub rule_profile: RuleProfileId,
    pub spin_profile: SpinProfileId,
    /// Each bag independently enables continuous B2B preservation.
    pub preserve_b2b_by_stage: [bool; 2],
    /// One-based stage-local bag selections, represented as zero-based bits.
    /// A partial bag at a stage boundary starts a new bag in the next stage.
    /// This augments the legacy whole-stage switches without changing them.
    pub preserve_b2b_bag_mask: u64,
    pub initial_b2b: bool,
    pub max_states: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundaryRecoveryError {
    InvalidHeight,
    BoardOutsideField,
    InvalidStageBoundary,
    InvalidBorrowRole,
    InvalidPlacementRoles,
    InvalidEarlyPlacementLimit,
    QueueTooLong,
    InvalidStateLimit,
    InvalidBagPolicy,
    UnsupportedRuleProfile,
    Cancelled,
}

/// Rebinds the same diagram to permutations of complete seven-piece bags.
/// Source-token indices change with each permutation, while each placement
/// keeps its (bag, piece) identity and lock-time geometry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryRecoveryBagRolePlan {
    reference: BoundaryRecoveryQuery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundaryRecoveryBagRoleError {
    InvalidReference(BoundaryRecoveryError),
    RequiresCompleteSevenBags,
    RequiresExactRoles,
    AmbiguousReferenceBag,
}

impl BoundaryRecoveryBagRolePlan {
    pub fn new(reference: BoundaryRecoveryQuery) -> Result<Self, BoundaryRecoveryBagRoleError> {
        reference
            .validate()
            .map_err(BoundaryRecoveryBagRoleError::InvalidReference)?;
        if reference.queue.len() % 7 != 0
            || reference.stage_one_queue_len % 7 != 0
            || reference.required_placements != reference.queue.len()
        {
            return Err(BoundaryRecoveryBagRoleError::RequiresCompleteSevenBags);
        }
        if reference.placement_role_masks.len() != reference.required_placements {
            return Err(BoundaryRecoveryBagRoleError::RequiresExactRoles);
        }
        for bag in reference.queue.chunks_exact(7) {
            let mut seen = 0_u8;
            for piece in bag {
                let bit = 1_u8 << piece_index(*piece);
                if seen & bit != 0 {
                    return Err(BoundaryRecoveryBagRoleError::AmbiguousReferenceBag);
                }
                seen |= bit;
            }
        }
        Ok(Self { reference })
    }

    /// `None` means this sequence lacks a required bag role. It is a proven
    /// diagram mismatch, not an invalid universe identity or a search timeout.
    pub fn query_for_sequence(&self, queue: &[PieceKind]) -> Option<BoundaryRecoveryQuery> {
        if queue.len() != self.reference.queue.len() {
            return None;
        }
        let mut query = self.reference.clone();
        query.queue = queue.to_vec();
        query.placement_role_pieces = self.reference.queue.clone();
        for (bag_index, bag) in queue.chunks_exact(7).enumerate() {
            let reference = &self.reference.queue[bag_index * 7..][..7];
            let mut seen = 0_u8;
            for piece in bag {
                let bit = 1_u8 << piece_index(*piece);
                if seen & bit != 0 {
                    return None;
                }
                seen |= bit;
                reference.iter().position(|source| source == piece)?;
            }
        }
        Some(query)
    }
}

fn piece_index(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => 0,
        PieceKind::J => 1,
        PieceKind::L => 2,
        PieceKind::O => 3,
        PieceKind::S => 4,
        PieceKind::T => 5,
        PieceKind::Z => 6,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundaryRecoveryStatus {
    Normal,
    PcPreservingRecovery,
    NonPcRecovery,
    NoPath,
    Incomplete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryRecoveryStep {
    /// Zero-based index in the actual source queue, not a piece-kind alias.
    pub source_queue_index: usize,
    /// Zero-based required placement role filled by this source token.
    pub placement_role_index: usize,
    pub piece: PieceKind,
    pub rotation: RotationState,
    pub x: i8,
    pub y: i8,
    pub hold_decision: &'static str,
    pub placement_mask: [u64; 4],
    pub cleared_row_mask: u32,
    pub board_after: [u64; 4],
    pub cleared_lines: u8,
    pub recognized_spin: bool,
    pub b2b_active_after: bool,
    pub stage_one_complete_after: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryRecoveryReport {
    pub status: BoundaryRecoveryStatus,
    pub normal_states: usize,
    pub recovery_states: usize,
    pub stage_one_checkpoint_step: Option<usize>,
    pub checkpoint_is_pc: Option<bool>,
    pub borrowed_stage_two_count: usize,
    pub steps: Vec<BoundaryRecoveryStep>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Token {
    index: u8,
    piece: PieceKind,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct State {
    board: ForwardBoard,
    stage_one_board: ForwardBoard,
    stage_two_board: ForwardBoard,
    active: Option<Token>,
    hold: Option<Token>,
    next_queue_index: u8,
    placed_mask: u64,
    fulfilled_role_mask: u64,
    checkpoint_step: Option<u8>,
    checkpoint_is_pc: bool,
    borrowed_count: u8,
    b2b_active: bool,
}

#[derive(Clone, Copy)]
struct Choice {
    token: Token,
    hold_after: Option<Token>,
    next_queue_index: u8,
    decision: &'static str,
}

#[derive(Debug)]
enum PassResult {
    Found {
        steps: Vec<BoundaryRecoveryStep>,
        checkpoint_step: usize,
        checkpoint_is_pc: bool,
        borrowed_count: usize,
    },
    NoPath,
    Incomplete,
}

struct Pass<'a> {
    query: &'a BoundaryRecoveryQuery,
    control: &'a ExecutionControl,
    reachability: ReachabilityWorkspace,
    seen: HashSet<State>,
    max_borrowed: u8,
    incomplete: bool,
}

impl BoundaryRecoveryQuery {
    fn role_piece(&self, role_index: usize) -> PieceKind {
        self.placement_role_pieces
            .get(role_index)
            .copied()
            .unwrap_or(self.queue[role_index])
    }

    fn stage_one_bag_count(&self) -> usize {
        self.stage_one_queue_len.div_ceil(7)
    }

    fn bag_count(&self) -> usize {
        self.stage_one_bag_count()
            + (self.required_placements - self.stage_one_queue_len).div_ceil(7)
    }

    fn bag_index(&self, source_index: usize) -> usize {
        if source_index < self.stage_one_queue_len {
            source_index / 7
        } else {
            self.stage_one_bag_count() + (source_index - self.stage_one_queue_len) / 7
        }
    }

    fn bag_source_mask(&self, bag: usize) -> u64 {
        let first = self.stage_one_bag_count();
        let (start, end) = if bag < first {
            let start = bag * 7;
            (start, (start + 7).min(self.stage_one_queue_len))
        } else {
            let start = self.stage_one_queue_len + (bag - first) * 7;
            (start, (start + 7).min(self.required_placements))
        };
        ((1_u64 << end) - 1) ^ ((1_u64 << start) - 1)
    }

    fn preserves_b2b_in_bag(&self, bag: usize) -> bool {
        let stage = usize::from(bag >= self.stage_one_bag_count());
        self.preserve_b2b_by_stage[stage] || self.preserve_b2b_bag_mask & (1_u64 << bag) != 0
    }

    fn validate(&self) -> Result<(), BoundaryRecoveryError> {
        if self.height == 0 || self.height > 25 {
            return Err(BoundaryRecoveryError::InvalidHeight);
        }
        let cells = u16::from(self.height) * 10;
        if self.initial_board.fits_cell_count(cells) != Ok(true)
            || self.final_board.fits_cell_count(cells) != Ok(true)
        {
            return Err(BoundaryRecoveryError::BoardOutsideField);
        }
        if self.queue.len() > MAX_QUEUE_PIECES {
            return Err(BoundaryRecoveryError::QueueTooLong);
        }
        if self.stage_one_queue_len == 0
            || self.stage_one_queue_len >= self.required_placements
            || self.required_placements > self.queue.len()
        {
            return Err(BoundaryRecoveryError::InvalidStageBoundary);
        }
        if self.max_early_placements > 1 {
            return Err(BoundaryRecoveryError::InvalidEarlyPlacementLimit);
        }
        if !self.placement_role_masks.is_empty()
            && (self.placement_role_masks.len() != self.required_placements
                || self.placement_role_masks.iter().any(|mask| {
                    mask.fits_cell_count(cells) != Ok(true)
                        || mask
                            .words()
                            .iter()
                            .map(|word| word.count_ones())
                            .sum::<u32>()
                            != 4
                }))
        {
            return Err(BoundaryRecoveryError::InvalidPlacementRoles);
        }
        if !self.placement_role_pieces.is_empty() {
            if self.placement_role_masks.is_empty()
                || self.placement_role_pieces.len() != self.required_placements
            {
                return Err(BoundaryRecoveryError::InvalidPlacementRoles);
            }
            let mut counts = [0_i8; 7];
            for piece in self.queue.iter().take(self.required_placements) {
                counts[piece_index(*piece) as usize] += 1;
            }
            for piece in &self.placement_role_pieces {
                counts[piece_index(*piece) as usize] -= 1;
            }
            if counts.iter().any(|count| *count != 0) {
                return Err(BoundaryRecoveryError::InvalidPlacementRoles);
            }
        }
        if self.max_early_placements == 1
            && (self.borrow_role_index < self.stage_one_queue_len
                || self.borrow_role_index >= self.required_placements
                || self.borrow_placement_mask.fits_cell_count(cells) != Ok(true)
                || self
                    .borrow_placement_mask
                    .words()
                    .iter()
                    .map(|word| word.count_ones())
                    .sum::<u32>()
                    != 4
                || (!self.placement_role_masks.is_empty()
                    && self.placement_role_masks[self.borrow_role_index].words()
                        != self.borrow_placement_mask.words()))
        {
            return Err(BoundaryRecoveryError::InvalidBorrowRole);
        }
        if self.max_states == 0 || self.max_states > MAX_SEARCH_STATES {
            return Err(BoundaryRecoveryError::InvalidStateLimit);
        }
        if self.preserve_b2b_bag_mask >> self.bag_count() != 0 {
            return Err(BoundaryRecoveryError::InvalidBagPolicy);
        }
        ReachabilityWorkspace::new(self.height, self.rule_profile)
            .map_err(|_| BoundaryRecoveryError::UnsupportedRuleProfile)?;
        Ok(())
    }

    /// A normal PC connection is proven first. A recovery is only classified
    /// as additional if that proof was complete and found no normal path.
    pub fn search(
        &self,
        control: &ExecutionControl,
    ) -> Result<BoundaryRecoveryReport, BoundaryRecoveryError> {
        self.validate()?;
        let (normal, normal_states) = Pass::new(self, control, 0)?.run()?;
        if !matches!(normal, PassResult::NoPath) {
            return Ok(report(normal, normal_states, 0, false));
        }
        if self.max_early_placements == 0 {
            return Ok(report(normal, normal_states, 0, false));
        }
        // The declared state budget covers both passes together. Exhausting
        // it after proving normal failure cannot prove recovery failure.
        let remaining_states = self.max_states.saturating_sub(normal_states);
        if remaining_states == 0 {
            return Ok(report(PassResult::Incomplete, normal_states, 0, true));
        }
        let mut recovery_query = self.clone();
        recovery_query.max_states = remaining_states;
        let (recovery, recovery_states) = Pass::new(&recovery_query, control, 1)?.run()?;
        Ok(report(recovery, normal_states, recovery_states, true))
    }
}

fn report(
    result: PassResult,
    normal_states: usize,
    recovery_states: usize,
    recovery_phase: bool,
) -> BoundaryRecoveryReport {
    match result {
        PassResult::Found {
            steps,
            checkpoint_step,
            checkpoint_is_pc,
            borrowed_count,
        } => BoundaryRecoveryReport {
            status: if !recovery_phase {
                BoundaryRecoveryStatus::Normal
            } else if checkpoint_is_pc {
                BoundaryRecoveryStatus::PcPreservingRecovery
            } else {
                BoundaryRecoveryStatus::NonPcRecovery
            },
            normal_states,
            recovery_states,
            stage_one_checkpoint_step: Some(checkpoint_step),
            checkpoint_is_pc: Some(checkpoint_is_pc),
            borrowed_stage_two_count: borrowed_count,
            steps,
        },
        PassResult::NoPath | PassResult::Incomplete => BoundaryRecoveryReport {
            status: if matches!(result, PassResult::Incomplete) {
                BoundaryRecoveryStatus::Incomplete
            } else {
                BoundaryRecoveryStatus::NoPath
            },
            normal_states,
            recovery_states,
            stage_one_checkpoint_step: None,
            checkpoint_is_pc: None,
            borrowed_stage_two_count: 0,
            steps: Vec::new(),
        },
    }
}

impl<'a> Pass<'a> {
    fn requires_b2b_for_lock(&self, state: State, source_index: usize) -> bool {
        let selected_bag = self.query.bag_index(source_index);
        if self.query.preserves_b2b_in_bag(selected_bag) {
            return true;
        }
        // A borrowed next-bag token can lock while an earlier selected bag
        // remains active. Conversely, after borrowing from a selected bag,
        // an older held token can lock before that selected bag completes.
        // Neither cross-boundary lock may silently break the live B2B chain.
        let required_mask = (1_u64 << self.query.required_placements) - 1;
        let pending = required_mask & !state.placed_mask;
        if pending != 0
            && self
                .query
                .preserves_b2b_in_bag(self.query.bag_index(pending.trailing_zeros() as usize))
        {
            return true;
        }
        (0..self.query.bag_count()).any(|bag| {
            let mask = self.query.bag_source_mask(bag);
            let placed = state.placed_mask & mask;
            self.query.preserves_b2b_in_bag(bag) && placed != 0 && placed != mask
        })
    }

    fn new(
        query: &'a BoundaryRecoveryQuery,
        control: &'a ExecutionControl,
        max_borrowed: u8,
    ) -> Result<Self, BoundaryRecoveryError> {
        Ok(Self {
            query,
            control,
            reachability: ReachabilityWorkspace::new(query.height, query.rule_profile)
                .map_err(|_| BoundaryRecoveryError::UnsupportedRuleProfile)?,
            seen: HashSet::new(),
            max_borrowed,
            incomplete: false,
        })
    }

    fn run(mut self) -> Result<(PassResult, usize), BoundaryRecoveryError> {
        let (initial, _, _) = place_and_clear(
            10,
            self.query.height,
            ForwardBoard::from_mask(self.query.initial_board),
        );
        let state = State {
            board: initial,
            stage_one_board: initial,
            stage_two_board: ForwardBoard::EMPTY,
            active: Some(Token {
                index: 0,
                piece: self.query.queue[0],
            }),
            hold: None,
            next_queue_index: 1,
            placed_mask: 0,
            fulfilled_role_mask: 0,
            checkpoint_step: None,
            checkpoint_is_pc: false,
            borrowed_count: 0,
            b2b_active: self.query.initial_b2b,
        };
        let mut path = Vec::with_capacity(self.query.queue.len());
        let found = self.visit(state, &mut path)?;
        let result = found.map_or_else(
            || {
                if self.incomplete {
                    PassResult::Incomplete
                } else {
                    PassResult::NoPath
                }
            },
            |state| PassResult::Found {
                steps: path,
                checkpoint_step: usize::from(state.checkpoint_step.unwrap_or(0)),
                checkpoint_is_pc: state.checkpoint_is_pc,
                borrowed_count: usize::from(state.borrowed_count),
            },
        );
        Ok((result, self.seen.len()))
    }

    fn visit(
        &mut self,
        state: State,
        path: &mut Vec<BoundaryRecoveryStep>,
    ) -> Result<Option<State>, BoundaryRecoveryError> {
        if self.control.is_cancelled() {
            return Err(BoundaryRecoveryError::Cancelled);
        }
        if state.placed_mask.count_ones() as usize == self.query.required_placements {
            let required_mask = (1_u64 << self.query.required_placements) - 1;
            if state.checkpoint_step.is_some()
                && state.placed_mask == required_mask
                && state.fulfilled_role_mask == required_mask
                && state.board.words() == self.query.final_board.words()
                && (self.max_borrowed == 0 || state.borrowed_count > 0)
            {
                return Ok(Some(state));
            }
            return Ok(None);
        }
        if self.seen.contains(&state) {
            return Ok(None);
        }
        if self.seen.len() >= self.query.max_states {
            self.incomplete = true;
            return Ok(None);
        }
        self.seen.insert(state);
        let mut choices = Vec::with_capacity(3);
        supply_choices(self.query, state, &mut choices);
        for choice in choices {
            // The tail is lookahead for the one-slot hold, not another target
            // role that can replace one of the declared placement tokens.
            if usize::from(choice.token.index) >= self.query.required_placements {
                continue;
            }
            let preserve_b2b = self.requires_b2b_for_lock(state, usize::from(choice.token.index));
            let role_candidates: Vec<usize> = if self.query.placement_role_masks.is_empty() {
                vec![usize::from(choice.token.index)]
            } else {
                (0..self.query.required_placements)
                    .filter(|role| {
                        state.fulfilled_role_mask & (1_u64 << role) == 0
                            && self.query.role_piece(*role) == choice.token.piece
                    })
                    .collect()
            };
            let locks = self
                .reachability
                .reachable_locks(state.board, choice.token.piece, true, true)
                .to_vec();
            for lock in locks {
                let placed = state.board.union_for_height(lock.mask, self.query.height);
                let (board_after, cleared_rows, cleared_lines) =
                    place_and_clear(10, self.query.height, placed);
                let (blocked_corners, blocked_front) = t_corner_counts(
                    state.board,
                    self.query.height,
                    choice.token.piece,
                    lock.rotation,
                    lock.x,
                    lock.y,
                );
                let perfect_clear = board_after.is_empty() && cleared_lines > 0;
                let edge = ScoringExecutionEdge::new(
                    0,
                    0,
                    choice.token.piece,
                    lock.rotation,
                    lock.x,
                    lock.y,
                    cleared_lines,
                    blocked_corners,
                    blocked_front,
                    lock.evidence.scoring(lock.rotation, lock.immobile),
                )
                .with_perfect_clear(perfect_clear);
                let spin_profile = SpinProfile::builtin(self.query.spin_profile);
                let recognized_spin =
                    SpinDetector::detect_scoring_edge_with_profile(edge, spin_profile).is_some();
                let b2b_active = if cleared_lines == 0 {
                    state.b2b_active
                } else {
                    cleared_lines == 4 || perfect_clear || recognized_spin
                };
                if preserve_b2b
                    && (!state.b2b_active
                        || !b2b_active
                        || !BackToBackPreservationPolicy::new(spin_profile).allows(edge))
                {
                    continue;
                }
                for role_index in &role_candidates {
                    let role_index = *role_index;
                    if state.fulfilled_role_mask & (1_u64 << role_index) != 0 {
                        continue;
                    }
                    if !self.query.placement_role_masks.is_empty()
                        && lock.mask.words() != self.query.placement_role_masks[role_index].words()
                    {
                        continue;
                    }
                    let is_stage_two = role_index >= self.query.stage_one_queue_len;
                    if is_stage_two
                        && state.checkpoint_step.is_none()
                        && (state.borrowed_count >= self.max_borrowed
                            || role_index != self.query.borrow_role_index
                            || lock.mask.words() != self.query.borrow_placement_mask.words())
                    {
                        continue;
                    }
                    let stage_one_placed = if is_stage_two {
                        state.stage_one_board
                    } else {
                        state
                            .stage_one_board
                            .union_for_height(lock.mask, self.query.height)
                    };
                    let stage_two_placed = if is_stage_two {
                        state
                            .stage_two_board
                            .union_for_height(lock.mask, self.query.height)
                    } else {
                        state.stage_two_board
                    };
                    let stage_one_board =
                        compact_tag(stage_one_placed, cleared_rows, self.query.height);
                    let stage_two_board =
                        compact_tag(stage_two_placed, cleared_rows, self.query.height);
                    let placed_mask = state.placed_mask | (1_u64 << choice.token.index);
                    let fulfilled_role_mask = state.fulfilled_role_mask | (1_u64 << role_index);
                    let stage_one_mask = (1_u64 << self.query.stage_one_queue_len) - 1;
                    let checkpoint_reached = state.checkpoint_step.is_none()
                        && fulfilled_role_mask & stage_one_mask == stage_one_mask
                        && stage_one_board.is_empty();
                    let next_queue_index = choice.next_queue_index;
                    let active = if usize::from(next_queue_index) < self.query.queue.len() {
                        Some(Token {
                            index: next_queue_index,
                            piece: self.query.queue[usize::from(next_queue_index)],
                        })
                    } else {
                        None
                    };
                    let next = State {
                        board: board_after,
                        stage_one_board,
                        stage_two_board,
                        active,
                        hold: choice.hold_after,
                        next_queue_index: next_queue_index
                            .saturating_add(u8::from(active.is_some())),
                        placed_mask,
                        fulfilled_role_mask,
                        checkpoint_step: if checkpoint_reached {
                            Some((path.len() + 1) as u8)
                        } else {
                            state.checkpoint_step
                        },
                        checkpoint_is_pc: if checkpoint_reached {
                            board_after.is_empty()
                        } else {
                            state.checkpoint_is_pc
                        },
                        borrowed_count: state.borrowed_count
                            + u8::from(is_stage_two && state.checkpoint_step.is_none()),
                        b2b_active,
                    };
                    debug_assert_eq!(
                        next.board.words(),
                        next.stage_one_board.union(next.stage_two_board).words()
                    );
                    path.push(BoundaryRecoveryStep {
                        source_queue_index: usize::from(choice.token.index),
                        placement_role_index: role_index,
                        piece: choice.token.piece,
                        rotation: lock.rotation,
                        x: lock.x,
                        y: lock.y,
                        hold_decision: choice.decision,
                        placement_mask: lock.mask.words(),
                        cleared_row_mask: cleared_rows,
                        board_after: board_after.words(),
                        cleared_lines,
                        recognized_spin,
                        b2b_active_after: b2b_active,
                        stage_one_complete_after: next.checkpoint_step.is_some(),
                    });
                    if let Some(found) = self.visit(next, path)? {
                        return Ok(Some(found));
                    }
                    path.pop();
                }
            }
        }
        Ok(None)
    }
}

fn supply_choices(query: &BoundaryRecoveryQuery, state: State, output: &mut Vec<Choice>) {
    let Some(active) = state.active else { return };
    output.push(Choice {
        token: active,
        hold_after: state.hold,
        next_queue_index: state.next_queue_index,
        decision: "none",
    });
    if !query.hold_enabled {
        return;
    }
    if let Some(held) = state.hold {
        output.push(Choice {
            token: held,
            hold_after: Some(active),
            next_queue_index: state.next_queue_index,
            decision: "swap",
        });
    } else if usize::from(state.next_queue_index) < query.queue.len() {
        let next_index = state.next_queue_index;
        output.push(Choice {
            token: Token {
                index: next_index,
                piece: query.queue[usize::from(next_index)],
            },
            hold_after: Some(active),
            next_queue_index: next_index + 1,
            decision: "store",
        });
    }
}

fn compact_tag(board: ForwardBoard, cleared_rows: u32, height: u8) -> ForwardBoard {
    if cleared_rows == 0 {
        return board;
    }
    let mut compacted = ForwardBoard::EMPTY;
    let mut output_row = 0_u8;
    for row in 0..height {
        if cleared_rows & (1_u32 << row) != 0 {
            continue;
        }
        let bits = board.row_bits(10, row);
        for x in 0..10_u8 {
            if bits & (1_u16 << x) != 0 {
                compacted.insert(u16::from(output_row) * 10 + u16::from(x));
            }
        }
        output_row += 1;
    }
    compacted
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;

    use super::*;

    fn control() -> ExecutionControl {
        ExecutionControl::new(ExecutionCancellationToken::new())
    }

    fn two_stage_query() -> BoundaryRecoveryQuery {
        BoundaryRecoveryQuery {
            initial_board: Board256Mask::from_words([0x3f0, 0, 0, 0]),
            final_board: Board256Mask::from_words([0xc030, 0, 0, 0]),
            height: 4,
            queue: vec![PieceKind::I, PieceKind::O],
            stage_one_queue_len: 1,
            required_placements: 2,
            placement_role_masks: Vec::new(),
            placement_role_pieces: Vec::new(),
            max_early_placements: 1,
            borrow_role_index: 1,
            borrow_placement_mask: Board256Mask::from_words([0x300c000, 0, 0, 0]),
            hold_enabled: false,
            rule_profile: RuleProfileId::SrsPlus,
            spin_profile: SpinProfileId::AllSpinPlus,
            preserve_b2b_by_stage: [false, false],
            preserve_b2b_bag_mask: 0,
            initial_b2b: true,
            max_states: 10_000,
        }
    }

    #[test]
    fn normal_checkpoint_reuses_the_same_board_and_supply() {
        let report = two_stage_query().search(&control()).unwrap();
        assert_eq!(report.status, BoundaryRecoveryStatus::Normal);
        assert_eq!(report.stage_one_checkpoint_step, Some(1));
        assert_eq!(report.checkpoint_is_pc, Some(true));
        assert_eq!(report.borrowed_stage_two_count, 0);
        assert_eq!(report.steps.len(), 2);
        assert_eq!(report.steps[0].source_queue_index, 0);
        assert_eq!(report.steps[1].source_queue_index, 1);
        assert_eq!(report.steps[1].board_after, [0xc030, 0, 0, 0]);
    }

    #[test]
    fn borrowed_stage_two_token_is_not_consumed_twice() {
        let mut query = two_stage_query();
        query.queue.push(PieceKind::T);
        query.hold_enabled = true;
        let (result, states) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
        let PassResult::Found {
            steps,
            checkpoint_is_pc,
            borrowed_count,
            ..
        } = result
        else {
            panic!("the second-stage O should be borrowable before the I clear: {result:?}, states={states}");
        };
        assert!(!checkpoint_is_pc);
        assert_eq!(borrowed_count, 1);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].source_queue_index, 1);
        assert_eq!(steps[0].hold_decision, "store");
        assert_eq!(steps[1].source_queue_index, 0);
        assert_eq!(steps[1].hold_decision, "swap");
        assert_eq!(steps[1].board_after, [0xc030, 0, 0, 0]);
    }

    #[test]
    fn recovery_rejects_a_different_early_placement_for_the_same_piece() {
        let mut query = two_stage_query();
        query.queue.push(PieceKind::T);
        query.hold_enabled = true;
        query.borrow_placement_mask = Board256Mask::from_words([0x600c000, 0, 0, 0]);
        let (result, _) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
        assert!(matches!(result, PassResult::NoPath));
    }

    #[test]
    fn exhausted_state_budget_is_not_reported_as_impossibility() {
        let mut query = two_stage_query();
        query.max_states = 1;
        let report = query.search(&control()).unwrap();
        assert_eq!(report.status, BoundaryRecoveryStatus::Incomplete);
    }

    #[test]
    fn zero_early_placements_runs_only_the_normal_connection() {
        let mut query = two_stage_query();
        query.max_early_placements = 0;
        query.borrow_placement_mask = Board256Mask::EMPTY;
        let report = query.search(&control()).unwrap();
        assert_eq!(report.status, BoundaryRecoveryStatus::Normal);
        assert_eq!(report.recovery_states, 0);
    }

    #[test]
    fn lookahead_token_cannot_replace_a_required_second_stage_token() {
        let mut query = two_stage_query();
        query.queue.push(PieceKind::T);
        query.hold_enabled = true;
        query.max_early_placements = 0;
        // After I clears the initial row, T could make this board, but the
        // declared second-stage token is O and T is only queue lookahead.
        query.final_board = Board256Mask::from_words([0x807, 0, 0, 0]);
        let report = query.search(&control()).unwrap();
        assert_eq!(report.status, BoundaryRecoveryStatus::NoPath);

        query.max_early_placements = 1;
        query.borrow_role_index = 2;
        assert_eq!(
            query.search(&control()),
            Err(BoundaryRecoveryError::InvalidBorrowRole)
        );
    }

    #[test]
    fn exact_stage_roles_follow_source_tokens_through_the_same_search() {
        let mut query = two_stage_query();
        query.max_early_placements = 0;
        let ordinary = query.search(&control()).unwrap();
        query.placement_role_masks = ordinary
            .steps
            .iter()
            .map(|step| Board256Mask::from_words(step.placement_mask))
            .collect();
        assert_eq!(
            query.search(&control()).unwrap().status,
            BoundaryRecoveryStatus::Normal
        );

        query.placement_role_masks[1] = Board256Mask::from_words([0xf, 0, 0, 0]);
        assert_eq!(
            query.search(&control()).unwrap().status,
            BoundaryRecoveryStatus::NoPath
        );
        query.placement_role_masks.pop();
        assert_eq!(
            query.search(&control()),
            Err(BoundaryRecoveryError::InvalidPlacementRoles)
        );
    }

    #[test]
    fn b2b_preservation_never_accepts_reestablishing_a_broken_chain() {
        let mut query = two_stage_query();
        query.initial_b2b = false;
        query.preserve_b2b_by_stage = [true, false];
        let report = query.search(&control()).unwrap();
        assert_eq!(report.status, BoundaryRecoveryStatus::NoPath);
    }

    #[test]
    fn bag_policy_checks_the_locked_source_bag_independently() {
        let mut query = two_stage_query();
        query.initial_b2b = false;
        query.preserve_b2b_bag_mask = 1;
        assert_eq!(
            query.search(&control()).unwrap().status,
            BoundaryRecoveryStatus::NoPath
        );
        query.preserve_b2b_bag_mask = 2;
        assert_eq!(
            query.search(&control()).unwrap().status,
            BoundaryRecoveryStatus::Normal
        );
        query.preserve_b2b_bag_mask = 4;
        assert_eq!(
            query.search(&control()),
            Err(BoundaryRecoveryError::InvalidBagPolicy)
        );
    }

    #[test]
    fn borrowed_token_cannot_break_an_active_selected_bag() {
        let mut query = two_stage_query();
        query.queue.push(PieceKind::T);
        query.hold_enabled = true;
        query.initial_b2b = false;
        let (unrestricted, _) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
        assert!(matches!(unrestricted, PassResult::Found { .. }));
        query.preserve_b2b_bag_mask = 1;
        let (protected, _) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
        assert!(matches!(protected, PassResult::NoPath));
    }

    #[test]
    fn complete_and_partial_stages_keep_distinct_bag_indices() {
        let mut query = two_stage_query();
        query.stage_one_queue_len = 8;
        query.required_placements = 15;
        assert_eq!(query.bag_count(), 3);
        assert_eq!(query.bag_index(0), 0);
        assert_eq!(query.bag_index(7), 1);
        assert_eq!(query.bag_index(8), 2);
        assert_eq!(query.bag_index(14), 2);
    }

    #[test]
    fn five_stage_one_bags_and_one_adjacent_bag_fit_without_bitmask_wraparound() {
        let mut query = two_stage_query();
        query.queue = (0..42)
            .map(|index| {
                [
                    PieceKind::I,
                    PieceKind::J,
                    PieceKind::L,
                    PieceKind::O,
                    PieceKind::S,
                    PieceKind::T,
                    PieceKind::Z,
                ][index % 7]
            })
            .collect();
        query.stage_one_queue_len = 35;
        query.required_placements = 42;
        query.max_early_placements = 0;
        query.max_states = 1;
        assert_eq!(
            query.search(&control()).unwrap().status,
            BoundaryRecoveryStatus::Incomplete
        );

        query.queue.push(PieceKind::I);
        assert_eq!(
            query.search(&control()),
            Err(BoundaryRecoveryError::QueueTooLong)
        );
    }

    #[test]
    fn bag_roles_remain_fixed_while_supply_tokens_permute() {
        let mut reference = two_stage_query();
        reference.height = 8;
        reference.initial_board = Board256Mask::EMPTY;
        reference.final_board = Board256Mask::EMPTY;
        reference.queue = vec![
            PieceKind::I,
            PieceKind::J,
            PieceKind::L,
            PieceKind::O,
            PieceKind::S,
            PieceKind::T,
            PieceKind::Z,
            PieceKind::Z,
            PieceKind::T,
            PieceKind::S,
            PieceKind::O,
            PieceKind::L,
            PieceKind::J,
            PieceKind::I,
        ];
        reference.stage_one_queue_len = 7;
        reference.required_placements = 14;
        reference.borrow_role_index = 10;
        reference.placement_role_masks = (0..14)
            .map(|index| Board256Mask::from_words([0xf_u64 << (index * 4), 0, 0, 0]))
            .collect();
        reference.borrow_placement_mask = reference.placement_role_masks[10];
        let plan = BoundaryRecoveryBagRolePlan::new(reference.clone()).unwrap();
        let mut sequence = reference.queue.clone();
        sequence[..7].reverse();
        sequence[7..].rotate_left(3);
        let projected = plan.query_for_sequence(&sequence).unwrap();
        assert_eq!(projected.queue, sequence);
        assert_eq!(projected.placement_role_pieces, reference.queue);
        assert_eq!(
            projected.placement_role_masks,
            reference.placement_role_masks
        );
        assert_eq!(projected.borrow_role_index, reference.borrow_role_index);
        assert_eq!(
            projected.borrow_placement_mask,
            reference.borrow_placement_mask
        );

        sequence[7] = PieceKind::I;
        assert!(plan.query_for_sequence(&sequence).is_none());
    }

    #[test]
    fn identical_supply_pieces_can_fill_roles_across_the_stage_boundary() {
        let mut query = two_stage_query();
        query.initial_board = Board256Mask::from_words([0xff3fc, 0, 0, 0]);
        query.queue = vec![PieceKind::O, PieceKind::O];
        query.placement_role_masks = vec![
            Board256Mask::from_words([0xc03, 0, 0, 0]),
            Board256Mask::from_words([0xc03000000, 0, 0, 0]),
        ];
        query.placement_role_pieces = query.queue.clone();
        query.borrow_placement_mask = query.placement_role_masks[1];

        let (result, states) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
        let PassResult::Found {
            steps,
            checkpoint_is_pc,
            borrowed_count,
            ..
        } = result
        else {
            panic!("same-piece roles should cross the boundary: {result:?}, states={states}");
        };
        assert_eq!(steps.len(), 2);
        assert_eq!(borrowed_count, 1);
        assert!(!checkpoint_is_pc);
        assert_eq!(steps[0].source_queue_index, 0);
        assert_eq!(steps[0].placement_role_index, 1);
        assert_eq!(steps[1].source_queue_index, 1);
        assert_eq!(steps[1].placement_role_index, 0);
        assert_eq!(steps[1].board_after, query.final_board.words());
    }

    #[test]
    fn exact_early_roles_can_prove_recovery_only_after_normal_failure() {
        let mut query = two_stage_query();
        query.queue.push(PieceKind::T);
        query.hold_enabled = true;
        let (PassResult::Found { steps, .. }, _) =
            Pass::new(&query, &control(), 1).unwrap().run().unwrap()
        else {
            panic!("expected an early-placement witness");
        };
        query.placement_role_masks = vec![Board256Mask::EMPTY; query.required_placements];
        for step in steps {
            query.placement_role_masks[step.source_queue_index] =
                Board256Mask::from_words(step.placement_mask);
        }
        query.borrow_placement_mask = query.placement_role_masks[query.borrow_role_index];
        let report = query.search(&control()).unwrap();
        assert_eq!(report.status, BoundaryRecoveryStatus::NonPcRecovery);
        assert!(report.normal_states > 0);
        assert!(report.recovery_states > 0);

        query.max_states = report.normal_states;
        let bounded = query.search(&control()).unwrap();
        assert_eq!(bounded.status, BoundaryRecoveryStatus::Incomplete);
        assert_eq!(bounded.recovery_states, 0);
        assert!(bounded.normal_states <= query.max_states);
    }
}
