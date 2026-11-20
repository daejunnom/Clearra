//! Physical BuildUp for one candidate in a generic Board64 frame. The
//! product planner still uses its specialized compact and wide solvers.

use std::collections::{HashMap, HashSet};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
    resource::ResourceTruncationReason,
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_replay::ScoringLockEvidence;
use clearra_rules::kicks::KickTableProfileId;
use clearra_supply::{
    hold::hold_policy::HoldPolicy, piece_source::PieceSourceKind, QueueObservationPolicy,
    SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionState, SupplyHoldState,
};

use super::{compact_target_board, merge_deleted_rows, place_and_clear};
use crate::backend::{generic_geometry::GeometryCover, DocumentReachabilityEngine};

const MAX_CANDIDATE_PLACEMENTS: usize = 15;
const MAX_ORIGINAL_ROWS: usize = u16::BITS as usize;
const MAX_SUPPLY_BRANCHES: usize = 1 + PieceKind::STANDARD_TETROMINOES.len();

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericBuildUpPlacement {
    pub piece: PieceKind,
    /// Coordinates before line clears, as in the generic geometry stream.
    pub target_cells: u64,
    /// When absent, all grounded rotations matching the geometry are legal
    /// alternatives. An authoritative exact operation must supply its pose.
    pub exact_pose: Option<GenericBuildUpOriginalPose>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericBuildUpOriginalPose {
    pub rotation: RotationState,
    pub x: u8,
    pub y: u8,
    /// Exact original deleted-row state, not a minimum required set.
    pub required_deleted_rows: u16,
}

#[derive(Clone, Copy, Debug)]
pub enum GenericBuildUpSource<'a> {
    /// The caller owns one complete, concrete pattern, even when it originated
    /// in an observed or materialized universe. This computes no probability.
    Concrete {
        queue: &'a [PieceKind],
        initial: SupplyExecutionState,
        release_held_at_terminal: bool,
    },
    StandardBag {
        initial: SupplyExecutionState,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericBuildUpLimits {
    /// Limits expanded candidate/supply states, not individual movement poses.
    pub max_states: usize,
    /// Bounds the exact suffix memo. After the cap, traversal continues.
    pub max_memo_entries: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenericBuildUpSupplyAction {
    Queue(SupplyBranchKind),
    ReleaseHeldAtTerminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericBuildUpStep {
    pub placement_index: usize,
    pub rotation: RotationState,
    pub x: u8,
    pub y: u8,
    pub physical_cells: u64,
    pub cleared_original_rows: u16,
    pub supply_action: GenericBuildUpSupplyAction,
    pub supply_after: SupplyExecutionState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericBuildUpOutcome {
    Buildable { steps: Vec<GenericBuildUpStep> },
    NotBuildable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenericBuildUpReport {
    pub outcome: GenericBuildUpOutcome,
    pub expanded_states: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericBuildUpCountReport {
    /// Concrete lock rotation, placement order, and hold history all contribute
    /// to this multiplicity. Movement paths to a lock do not.
    pub variants: u64,
    pub expanded_states: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GenericBuildUpTSpinEvidence {
    pub step_index: usize,
    pub rotation: ScoringLockEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenericBuildUpTSpinReport {
    pub build: GenericBuildUpReport,
    /// Only T locks where an exact final-rotation witness exists.
    pub t_spin_evidence: Vec<GenericBuildUpTSpinEvidence>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GenericBuildUpError {
    InvalidRequest,
    UnsupportedScope,
    Cancelled,
    Resource(ResourceTruncationReason),
}

/// Reachability templates are layout/profile-owned and reused across
/// independently supplied geometry covers; search state/memo is per request.
pub struct GenericBoard64BuildUpWorkspace {
    layout: Board64Layout,
    harddrop_only: bool,
    reachability: DocumentReachabilityEngine,
}

impl GenericBoard64BuildUpWorkspace {
    pub fn new(
        layout: Board64Layout,
        kick_profile: KickTableProfileId,
    ) -> Result<Self, GenericBuildUpError> {
        if usize::from(layout.height()) > MAX_ORIGINAL_ROWS {
            return Err(GenericBuildUpError::UnsupportedScope);
        }
        Ok(Self {
            layout,
            harddrop_only: kick_profile == KickTableProfileId::NoKick,
            reachability: DocumentReachabilityEngine::new(
                layout.width() as u8,
                layout.height() as u8,
                kick_profile,
            )
            .ok_or(GenericBuildUpError::UnsupportedScope)?,
        })
    }

    /// Verifies exact existential BuildUp. Resource exhaustion is never an
    /// exact negative.
    pub fn verify(
        &mut self,
        initial_board: u64,
        placements: &[GenericBuildUpPlacement],
        source: GenericBuildUpSource<'_>,
        limits: GenericBuildUpLimits,
        control: &ExecutionControl,
    ) -> Result<GenericBuildUpReport, GenericBuildUpError> {
        let (mut search, root) = self.prepare_search(
            initial_board,
            placements,
            source,
            limits,
            control,
            SearchMemo::Failed(HashSet::new()),
        )?;
        search
            .path
            .try_reserve_exact(placements.len())
            .map_err(|_| GenericBuildUpError::Resource(ResourceTruncationReason::MemoryExceeded))?;
        let found = search.visit::<false>(root)? != 0;
        if control.is_cancelled() {
            return Err(GenericBuildUpError::Cancelled);
        }
        Ok(GenericBuildUpReport {
            outcome: if found {
                GenericBuildUpOutcome::Buildable { steps: search.path }
            } else {
                GenericBuildUpOutcome::NotBuildable
            },
            expanded_states: search.expanded,
        })
    }

    /// Counts all C-style BuildUp histories for one candidate. It does not
    /// retain, score, or rank variants. The memo cap only bounds storage; a
    /// state or variant limit returns typed incompleteness instead of a count.
    pub fn count_all(
        &mut self,
        initial_board: u64,
        placements: &[GenericBuildUpPlacement],
        source: GenericBuildUpSource<'_>,
        limits: GenericBuildUpLimits,
        max_variants: Option<u64>,
        control: &ExecutionControl,
    ) -> Result<GenericBuildUpCountReport, GenericBuildUpError> {
        let (mut search, root) = self.prepare_search(
            initial_board,
            placements,
            source,
            limits,
            control,
            SearchMemo::Completions {
                counts: HashMap::new(),
                variant_limit: max_variants.unwrap_or(u64::MAX),
            },
        )?;
        let variants = search.visit::<true>(root)?;
        if control.is_cancelled() {
            return Err(GenericBuildUpError::Cancelled);
        }
        Ok(GenericBuildUpCountReport {
            variants,
            expanded_states: search.expanded,
        })
    }

    /// Annotates the chosen build witness with the strongest *tier* of
    /// verified final T rotation. This is not a score or alternate build-order
    /// optimizer, and does not inherit C's traversal order inside a tier.
    pub fn verify_with_t_spin_evidence(
        &mut self,
        initial_board: u64,
        placements: &[GenericBuildUpPlacement],
        source: GenericBuildUpSource<'_>,
        limits: GenericBuildUpLimits,
        control: &ExecutionControl,
    ) -> Result<GenericBuildUpTSpinReport, GenericBuildUpError> {
        let build = self.verify(initial_board, placements, source, limits, control)?;
        let mut t_spin_evidence = Vec::new();
        if let GenericBuildUpOutcome::Buildable { steps } = &build.outcome {
            if !self.harddrop_only {
                let mut board = initial_board;
                let width = self.layout.width() as u8;
                let height = self.layout.height() as u8;
                for (step_index, step) in steps.iter().enumerate() {
                    if control.is_cancelled() {
                        return Err(GenericBuildUpError::Cancelled);
                    }
                    if placements[step.placement_index].piece == PieceKind::T {
                        if let Some(rotation) = self.preferred_t_spin_rotation_before_clear(
                            board,
                            step.rotation,
                            step.x,
                            step.y,
                        )? {
                            t_spin_evidence.try_reserve(1).map_err(|_| {
                                GenericBuildUpError::Resource(
                                    ResourceTruncationReason::MemoryExceeded,
                                )
                            })?;
                            t_spin_evidence.push(GenericBuildUpTSpinEvidence {
                                step_index,
                                rotation,
                            });
                        }
                    }
                    board = place_and_clear(width, height, board | step.physical_cells).0;
                }
            }
        }
        if control.is_cancelled() {
            return Err(GenericBuildUpError::Cancelled);
        }
        Ok(GenericBuildUpTSpinReport {
            build,
            t_spin_evidence,
        })
    }

    /// Uses the legacy tier order for a concrete pre-clear T lock: first
    /// success at kick index four, then any other reachable final rotation.
    /// The returned evidence proves the rotation, not a spin classification.
    pub fn preferred_t_spin_rotation_before_clear(
        &mut self,
        board: u64,
        rotation: RotationState,
        x: u8,
        y: u8,
    ) -> Result<Option<ScoringLockEvidence>, GenericBuildUpError> {
        if board & !self.layout.all_cells_mask() != 0 {
            return Err(GenericBuildUpError::InvalidRequest);
        }
        if self.harddrop_only
            || u16::from(x) >= self.layout.width()
            || u16::from(y) >= self.layout.height()
        {
            return Ok(None);
        }
        Ok(self
            .reachability
            .preferred_t_spin_rotation(board, rotation, x as i8, y as i8))
    }

    fn prepare_search<'workspace, 'request, 'queue>(
        &'workspace mut self,
        initial_board: u64,
        placements: &'request [GenericBuildUpPlacement],
        source: GenericBuildUpSource<'queue>,
        limits: GenericBuildUpLimits,
        control: &'request ExecutionControl,
        memo: SearchMemo,
    ) -> Result<(Search<'workspace, 'request, 'queue>, SearchState), GenericBuildUpError> {
        if control.is_cancelled() {
            return Err(GenericBuildUpError::Cancelled);
        }
        if placements.is_empty() {
            return Err(GenericBuildUpError::InvalidRequest);
        }
        if control.partition().count() != 1 || placements.len() > MAX_CANDIDATE_PLACEMENTS {
            return Err(GenericBuildUpError::UnsupportedScope);
        }
        self.validate_placements(initial_board, placements)?;
        let (supply, initial_supply) = Supply::prepare(source, placements.len())?;
        Ok((
            Search {
                workspace: self,
                supply,
                control,
                placements,
                limits,
                expanded: 0,
                memo,
                path: Vec::new(),
            },
            SearchState {
                board: initial_board,
                deleted_rows: 0,
                remaining: (1_u16 << placements.len()) - 1,
                supply: initial_supply,
            },
        ))
    }

    /// The geometry stream's preferred rotation is not an exact physical
    /// witness after a clear. Keep the whole shape domain at this boundary.
    pub fn verify_geometry_cover(
        &mut self,
        initial_board: u64,
        cover: &GeometryCover<'_>,
        source: GenericBuildUpSource<'_>,
        limits: GenericBuildUpLimits,
        control: &ExecutionControl,
    ) -> Result<GenericBuildUpReport, GenericBuildUpError> {
        let (placements, count) = cover_placements(cover)?;
        self.verify(initial_board, &placements[..count], source, limits, control)
    }

    pub fn count_all_geometry_cover(
        &mut self,
        initial_board: u64,
        cover: &GeometryCover<'_>,
        source: GenericBuildUpSource<'_>,
        limits: GenericBuildUpLimits,
        max_variants: Option<u64>,
        control: &ExecutionControl,
    ) -> Result<GenericBuildUpCountReport, GenericBuildUpError> {
        let (placements, count) = cover_placements(cover)?;
        self.count_all(
            initial_board,
            &placements[..count],
            source,
            limits,
            max_variants,
            control,
        )
    }

    fn validate_placements(
        &self,
        initial_board: u64,
        placements: &[GenericBuildUpPlacement],
    ) -> Result<(), GenericBuildUpError> {
        let layout_mask = self.layout.all_cells_mask();
        let mut occupied = initial_board;
        if occupied & !layout_mask != 0 {
            return Err(GenericBuildUpError::InvalidRequest);
        }
        for placement in placements {
            let cells = placement.target_cells;
            if cells.count_ones() != 4 || cells & (occupied | !layout_mask) != 0 {
                return Err(GenericBuildUpError::InvalidRequest);
            }
            if let Some(pose) = placement.exact_pose {
                if u16::from(pose.x) >= self.layout.width()
                    || u16::from(pose.y) >= self.layout.height()
                    || u32::from(pose.required_deleted_rows) >> self.layout.height() != 0
                {
                    return Err(GenericBuildUpError::InvalidRequest);
                }
            }
            occupied |= cells;
        }
        Ok(())
    }

    fn lock_reaches(
        &mut self,
        board: u64,
        piece: PieceKind,
        rotation: RotationState,
        x: u8,
        y: u8,
        cells: u64,
    ) -> bool {
        if cells & board != 0 {
            return false;
        }
        if self.harddrop_only {
            return has_harddrop_path(self.layout.width(), board, cells, y);
        }
        let reached = self
            .reachability
            .check_lock(board, piece, rotation, x as i8, y as i8);
        reached.reachable && reached.lock_mask == cells
    }
}

fn cover_placements(
    cover: &GeometryCover<'_>,
) -> Result<([GenericBuildUpPlacement; MAX_CANDIDATE_PLACEMENTS], usize), GenericBuildUpError> {
    let representatives = cover.representatives();
    let count = representatives.len();
    if count > MAX_CANDIDATE_PLACEMENTS {
        return Err(GenericBuildUpError::UnsupportedScope);
    }
    let mut placements = [GenericBuildUpPlacement {
        piece: PieceKind::I,
        target_cells: 0,
        exact_pose: None,
    }; MAX_CANDIDATE_PLACEMENTS];
    for (placement, representative) in placements.iter_mut().zip(representatives) {
        placement.piece = representative.piece;
        placement.target_cells = representative.target_cells;
    }
    Ok((placements, count))
}

struct Supply<'a> {
    source: GenericBuildUpSource<'a>,
    automaton: SupplyExecutionAutomaton,
}

#[derive(Clone, Copy)]
struct SupplyBranch {
    action: GenericBuildUpSupplyAction,
    next: SupplyExecutionState,
}

impl<'a> Supply<'a> {
    fn prepare(
        source: GenericBuildUpSource<'a>,
        placements: usize,
    ) -> Result<(Self, SupplyExecutionState), GenericBuildUpError> {
        let invalid = |_| GenericBuildUpError::InvalidRequest;
        let (automaton, state) = match source {
            GenericBuildUpSource::Concrete {
                queue,
                initial,
                release_held_at_terminal,
            } => {
                if usize::from(initial.cursor) > queue.len()
                    || queue.len() > usize::from(u16::MAX)
                    || initial.hold_empty != initial.hold_piece.is_none()
                    || initial.hold_policy == HoldPolicy::Forbidden
                        && (!initial.hold_empty || release_held_at_terminal)
                {
                    return Err(GenericBuildUpError::InvalidRequest);
                }
                if initial.source_kind == PieceSourceKind::BagUniverse {
                    return Err(GenericBuildUpError::UnsupportedScope);
                }
                (SupplyExecutionAutomaton::sequence(), initial)
            }
            GenericBuildUpSource::StandardBag { initial } => {
                let automaton = SupplyExecutionAutomaton::for_bag(&PieceKind::STANDARD_TETROMINOES)
                    .map_err(invalid)?;
                let initial = automaton
                    .project_bag_initial_state(
                        initial,
                        initial.hold_policy != HoldPolicy::Forbidden,
                        placements,
                    )
                    .map_err(invalid)?;
                (automaton, initial)
            }
        };
        if state.observation.policy != QueueObservationPolicy::FullQueueOracle {
            return Err(GenericBuildUpError::UnsupportedScope);
        }
        Ok((Self { source, automaton }, state))
    }

    fn branches(
        &self,
        state: SupplyExecutionState,
        piece: PieceKind,
        terminal: bool,
    ) -> Result<[Option<SupplyBranch>; MAX_SUPPLY_BRANCHES], GenericBuildUpError> {
        let mut branches = [None; MAX_SUPPLY_BRANCHES];
        let mut index = 0;
        let mut overflow = false;
        let mut push = |action, next| {
            if let Some(slot) = branches.get_mut(index) {
                *slot = Some(SupplyBranch { action, next });
                index += 1;
            } else {
                overflow = true;
            }
        };
        match self.source {
            GenericBuildUpSource::Concrete {
                queue,
                release_held_at_terminal,
                ..
            } => {
                let cursor = usize::from(state.cursor);
                if let Some(&current) = queue.get(cursor) {
                    let next = queue.get(cursor + 1).copied();
                    for kind in [
                        SupplyBranchKind::Current,
                        SupplyBranchKind::SwapHeld,
                        SupplyBranchKind::StoreCurrent,
                    ] {
                        let permitted = match kind {
                            SupplyBranchKind::Current => {
                                state.hold_policy != HoldPolicy::Required && current == piece
                            }
                            SupplyBranchKind::SwapHeld => {
                                state.hold_state() == SupplyHoldState::Occupied(piece)
                            }
                            SupplyBranchKind::StoreCurrent => {
                                state.hold_state() == SupplyHoldState::Empty && next == Some(piece)
                            }
                        };
                        if permitted {
                            let step = self
                                .automaton
                                .transition(state, kind, current, next)
                                .map_err(|_| GenericBuildUpError::InvalidRequest)?;
                            push(GenericBuildUpSupplyAction::Queue(kind), step.next_state);
                        }
                    }
                } else if release_held_at_terminal && terminal && state.hold_piece == Some(piece) {
                    let mut released = state;
                    released.hold_piece = None;
                    released.hold_empty = true;
                    push(GenericBuildUpSupplyAction::ReleaseHeldAtTerminal, released);
                }
            }
            GenericBuildUpSource::StandardBag { .. } => {
                self.automaton
                    .for_each_matching_bag_step(state, piece, |step| {
                        push(
                            GenericBuildUpSupplyAction::Queue(step.evidence.branch_kind),
                            step.next_state,
                        );
                    })
                    .map_err(|_| GenericBuildUpError::InvalidRequest)?;
            }
        }
        if overflow {
            return Err(GenericBuildUpError::Resource(
                ResourceTruncationReason::FrontierBudgetExceeded,
            ));
        }
        Ok(branches)
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct SearchState {
    board: u64,
    deleted_rows: u16,
    remaining: u16,
    supply: SupplyExecutionState,
}

enum SearchMemo {
    Failed(HashSet<SearchState>),
    Completions {
        counts: HashMap<SearchState, u64>,
        variant_limit: u64,
    },
}

impl SearchMemo {
    fn lookup(&self, state: &SearchState) -> Option<u64> {
        match self {
            Self::Failed(states) => states.contains(state).then_some(0),
            Self::Completions { counts, .. } => counts.get(state).copied(),
        }
    }

    fn accept_count(&self, count: Option<u64>) -> Result<u64, GenericBuildUpError> {
        let limit = match self {
            Self::Failed(_) => u64::MAX,
            Self::Completions { variant_limit, .. } => *variant_limit,
        };
        count
            .filter(|&value| value <= limit)
            .ok_or(GenericBuildUpError::Resource(
                ResourceTruncationReason::CandidateBudgetExceeded,
            ))
    }

    fn remember(
        &mut self,
        state: SearchState,
        count: u64,
        limit: usize,
    ) -> Result<(), GenericBuildUpError> {
        let exhausted = |_| GenericBuildUpError::Resource(ResourceTruncationReason::MemoryExceeded);
        match self {
            Self::Failed(states) if count == 0 && states.len() < limit => {
                states.try_reserve(1).map_err(exhausted)?;
                states.insert(state);
            }
            Self::Completions { counts, .. } if counts.len() < limit => {
                counts.try_reserve(1).map_err(exhausted)?;
                counts.insert(state, count);
            }
            _ => {}
        }
        Ok(())
    }
}

struct Search<'workspace, 'request, 'queue> {
    workspace: &'workspace mut GenericBoard64BuildUpWorkspace,
    placements: &'request [GenericBuildUpPlacement],
    control: &'request ExecutionControl,
    supply: Supply<'queue>,
    limits: GenericBuildUpLimits,
    expanded: usize,
    memo: SearchMemo,
    path: Vec<GenericBuildUpStep>,
}

impl Search<'_, '_, '_> {
    fn visit<const COUNT_ALL: bool>(
        &mut self,
        state: SearchState,
    ) -> Result<u64, GenericBuildUpError> {
        if self.control.is_cancelled() {
            return Err(GenericBuildUpError::Cancelled);
        }
        if state.remaining == 0 {
            let count = u64::from(state.board == 0);
            return if COUNT_ALL {
                self.memo.accept_count(Some(count))
            } else {
                Ok(count)
            };
        }
        if let Some(count) = self.memo.lookup(&state) {
            return Ok(count);
        }
        if self.expanded == self.limits.max_states {
            return Err(GenericBuildUpError::Resource(
                ResourceTruncationReason::FrontierBudgetExceeded,
            ));
        }
        self.expanded += 1;
        let width = self.workspace.layout.width() as u8;
        let height = self.workspace.layout.height() as u8;
        let mut count = 0_u64;
        let mut remaining = state.remaining;
        while remaining != 0 {
            let placement_index = remaining.trailing_zeros() as usize;
            remaining &= remaining - 1;
            let placement = self.placements[placement_index];
            let next_remaining = state.remaining & !(1_u16 << placement_index);
            let branches =
                self.supply
                    .branches(state.supply, placement.piece, next_remaining == 0)?;
            if branches[0].is_none() {
                continue;
            }
            let Some(cells) =
                project_cells(width, height, placement.target_cells, state.deleted_rows)
            else {
                continue;
            };
            let (x, y) = normalized_anchor(width, cells);
            let definition = standard_tetromino_registry()
                .get(placement.piece)
                .expect("PieceKind contains only standard pieces");
            let mut first_reachable_rotation = None;
            let mut reachable_rotations = 0;
            for rotation in RotationState::ALL {
                if placement.exact_pose.is_some_and(|pose| {
                    rotation != pose.rotation
                        || x != pose.x
                        || y != pose.y.saturating_sub(
                            (state.deleted_rows & ((1_u16 << pose.y) - 1)).count_ones() as u8,
                        )
                        || pose.required_deleted_rows != state.deleted_rows
                        || state.deleted_rows & (1_u16 << pose.y) != 0
                }) {
                    continue;
                }
                let shape = definition.shape(rotation);
                let cell_mask = shape.cells().iter().try_fold(0_u64, |mask, cell| {
                    let column = i16::from(x) + i16::from(cell.x());
                    let row = i16::from(y) + i16::from(cell.y());
                    (0..i16::from(width))
                        .contains(&column)
                        .then_some(())
                        .filter(|_| (0..i16::from(height)).contains(&row))
                        .map(|_| {
                            mask | (1_u64 << (row as usize * usize::from(width) + column as usize))
                        })
                });
                if cell_mask == Some(cells)
                    && self.workspace.lock_reaches(
                        state.board,
                        placement.piece,
                        rotation,
                        x,
                        y,
                        cells,
                    )
                {
                    // Equal occupied cells yield the same physical child.
                    // The exact count nevertheless includes each valid true
                    // rotation, matching C's geometry-variant multiplicity.
                    first_reachable_rotation.get_or_insert(rotation);
                    reachable_rotations += 1;
                    if !COUNT_ALL {
                        break;
                    }
                }
            }
            let Some(rotation) = first_reachable_rotation else {
                continue;
            };
            let (board, cleared_current_rows, _) =
                place_and_clear(width, height, state.board | cells);
            let deleted_rows = merge_deleted_rows(height, state.deleted_rows, cleared_current_rows)
                .ok_or(GenericBuildUpError::InvalidRequest)?;
            for branch in branches.into_iter().flatten() {
                let next = SearchState {
                    board,
                    deleted_rows,
                    remaining: next_remaining,
                    supply: branch.next,
                };
                if COUNT_ALL {
                    let child = self.visit::<true>(next)?;
                    let total = child
                        .checked_mul(reachable_rotations)
                        .and_then(|contribution| count.checked_add(contribution));
                    count = self.memo.accept_count(total)?;
                } else {
                    self.path.push(GenericBuildUpStep {
                        placement_index,
                        rotation,
                        x,
                        y,
                        physical_cells: cells,
                        cleared_original_rows: deleted_rows & !state.deleted_rows,
                        supply_action: branch.action,
                        supply_after: branch.next,
                    });
                    if self.visit::<false>(next)? != 0 {
                        return Ok(1);
                    }
                    self.path.pop();
                }
            }
        }
        self.memo
            .remember(state, count, self.limits.max_memo_entries)?;
        Ok(count)
    }
}

fn project_cells(width: u8, height: u8, cells: u64, deleted_rows: u16) -> Option<u64> {
    let mut remaining = cells;
    while remaining != 0 {
        let index = remaining.trailing_zeros();
        if deleted_rows & (1_u16 << (index / u32::from(width))) != 0 {
            return None;
        }
        remaining &= remaining - 1;
    }
    Some(compact_target_board(width, height, cells, deleted_rows))
}

fn normalized_anchor(width: u8, cells: u64) -> (u8, u8) {
    let width = u32::from(width);
    let y = cells.trailing_zeros() / width;
    let mut x = width;
    let mut remaining = cells;
    while remaining != 0 {
        x = x.min(remaining.trailing_zeros() % width);
        remaining &= remaining - 1;
    }
    (x as u8, y as u8)
}

fn has_harddrop_path(width: u16, board: u64, cells: u64, y: u8) -> bool {
    let width = u32::from(width);
    let grounded = y == 0 || (cells.checked_shr(width).unwrap_or(0) & board) != 0;
    if !grounded {
        return false;
    }
    let mut raised = cells.checked_shl(width).unwrap_or(0);
    while raised != 0 {
        if raised & board != 0 {
            return false;
        }
        raised = raised.checked_shl(width).unwrap_or(0);
    }
    true
}

#[cfg(test)]
#[path = "generic_board64_search_tests.rs"]
mod tests;
