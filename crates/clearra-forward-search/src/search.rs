use std::{
    collections::{BTreeMap, HashSet},
    ops::{Index, IndexMut},
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_replay::ScoringExecutionEdge;
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::{
    b2b_preservation::BackToBackPreservationPolicy,
    damage::{TetrioDamageAction, TetrioDamageProfile, TetrioDamageState},
    event::{SpinDetector, SpinEvent},
    profile::{SpinProfile, SpinProfileId},
};

use crate::{
    board::{place_and_clear, ForwardBoard, ForwardBoardCatalog},
    query::{
        ForwardLineClearPolicy, ForwardSearchMode, ForwardSearchQuery, ForwardSpinCategory,
        ForwardSpinLineRequirement, ForwardSpinTarget,
    },
    reachability::ReachabilityWorkspace,
    result::{ForwardPathStep, ForwardSearchOutcome, ForwardSearchReport, ForwardSpinGroup},
    t_spin_acceleration::TSpinAcceleration,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForwardSearchError {
    EmptyQueue,
    InvalidHeight,
    BoardOutsideField,
    PatternRequiresSpinFinder,
    SpinProfileDisabled,
    UnsupportedRuleProfile(&'static str),
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForwardSearchAdvance {
    Pending,
    Completed(ForwardSearchReport),
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct StateKey {
    pub(crate) board: ForwardBoard,
    pub(crate) active: PieceKind,
    pub(crate) cursor: u16,
    pub(crate) hold: Option<PieceKind>,
    pub(crate) combo: Option<u16>,
    pub(crate) back_to_back: Option<u16>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct StateIndexKey {
    state: StoredStateKey,
    total_damage: u32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct StoredStateKey {
    board_id: u32,
    active: PieceKind,
    cursor: u16,
    hold: Option<PieceKind>,
    combo: Option<u16>,
    back_to_back: Option<u16>,
}

#[derive(Clone, Debug)]
pub(crate) struct SearchNode {
    pub(crate) key: StoredStateKey,
    total_damage: u32,
    pub(crate) traces: TraceChain,
}

impl SearchNode {
    pub(crate) fn damage_state(&self) -> TetrioDamageState {
        TetrioDamageState::from_parts(self.key.combo, self.key.back_to_back, self.total_damage)
    }

    fn index_key(&self, mode: ForwardSearchMode) -> StateIndexKey {
        StateIndexKey {
            state: self.key,
            total_damage: if matches!(mode, ForwardSearchMode::DamageAtLeast(_)) {
                self.total_damage
            } else {
                0
            },
        }
    }
}

const TERMINAL_FUSION_MIN_PARENT_STATES: usize = 1 << 18;
const NODE_CHUNK_SHIFT: usize = 15;
const NODE_CHUNK_LEN: usize = 1 << NODE_CHUNK_SHIFT;
const NODE_CHUNK_MASK: usize = NODE_CHUNK_LEN - 1;

pub(crate) struct SearchNodeArena {
    chunks: Vec<Vec<SearchNode>>,
    len: usize,
}

impl SearchNodeArena {
    fn from_node(node: SearchNode) -> Self {
        let mut arena = Self::default();
        arena.push(node);
        arena
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn push(&mut self, node: SearchNode) {
        let chunk_index = self.len >> NODE_CHUNK_SHIFT;
        if chunk_index == self.chunks.len() {
            self.chunks.push(Vec::with_capacity(NODE_CHUNK_LEN));
        }
        self.chunks[chunk_index].push(node);
        self.len += 1;
    }

    pub(crate) fn clear(&mut self) {
        for chunk in &mut self.chunks {
            chunk.clear();
        }
        self.len = 0;
    }

    fn iter(&self) -> impl Iterator<Item = &SearchNode> {
        self.chunks.iter().flat_map(|chunk| chunk.iter())
    }
}

impl Default for SearchNodeArena {
    fn default() -> Self {
        Self {
            chunks: Vec::new(),
            len: 0,
        }
    }
}

impl Index<usize> for SearchNodeArena {
    type Output = SearchNode;

    fn index(&self, index: usize) -> &Self::Output {
        &self.chunks[index >> NODE_CHUNK_SHIFT][index & NODE_CHUNK_MASK]
    }
}

impl IndexMut<usize> for SearchNodeArena {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.chunks[index >> NODE_CHUNK_SHIFT][index & NODE_CHUNK_MASK]
    }
}

pub(crate) struct StateIndexTable {
    slots: Vec<u32>,
    len: usize,
    mode: ForwardSearchMode,
}

impl StateIndexTable {
    fn new(mode: ForwardSearchMode) -> Self {
        Self {
            slots: vec![0; 16],
            len: 0,
            mode,
        }
    }

    fn get(&self, key: &StateIndexKey, nodes: &SearchNodeArena) -> Option<usize> {
        let mut slot = state_index_hash(*key) as usize & (self.slots.len() - 1);
        loop {
            let encoded = self.slots[slot];
            if encoded == 0 {
                return None;
            }
            let index = (encoded - 1) as usize;
            if nodes[index].index_key(self.mode) == *key {
                return Some(index);
            }
            slot = (slot + 1) & (self.slots.len() - 1);
        }
    }

    fn reserve_for_insert(&mut self, nodes: &SearchNodeArena) {
        if (self.len + 1) * 10 <= self.slots.len() * 7 {
            return;
        }
        self.slots = vec![0; self.slots.len() * 2];
        self.len = 0;
        for (index, node) in nodes.iter().enumerate() {
            self.insert_without_grow(node.index_key(self.mode), index);
        }
    }

    fn insert(&mut self, key: StateIndexKey, index: usize) {
        debug_assert!(index < u32::MAX as usize);
        self.insert_without_grow(key, index);
    }

    fn insert_without_grow(&mut self, key: StateIndexKey, index: usize) {
        let mut slot = state_index_hash(key) as usize & (self.slots.len() - 1);
        while self.slots[slot] != 0 {
            slot = (slot + 1) & (self.slots.len() - 1);
        }
        self.slots[slot] = index as u32 + 1;
        self.len += 1;
    }

    pub(crate) fn clear(&mut self) {
        self.slots.fill(0);
        self.len = 0;
    }
}

fn state_index_hash(key: StateIndexKey) -> u64 {
    let state = key.state;
    let mut hash = 0x9e37_79b9_7f4a_7c15;
    hash = mix_state_index_word(hash, u64::from(state.board_id));
    hash = mix_state_index_word(
        hash,
        u64::from(state.active.as_ascii() as u32)
            | (u64::from(state.cursor) << 8)
            | (u64::from(state.hold.map_or(0, |piece| piece.as_ascii() as u32)) << 24),
    );
    hash = mix_state_index_word(hash, option_u16_hash_word(state.combo));
    hash = mix_state_index_word(hash, option_u16_hash_word(state.back_to_back));
    mix_state_index_word(hash, u64::from(key.total_damage))
}

fn option_u16_hash_word(value: Option<u16>) -> u64 {
    value.map_or(1_u64 << 16, u64::from)
}

fn mix_state_index_word(hash: u64, value: u64) -> u64 {
    (hash ^ value.wrapping_add(0x9e37_79b9_7f4a_7c15))
        .rotate_left(27)
        .wrapping_mul(0x94d0_49bb_1331_11eb)
}

const NO_TRACE: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TraceChain {
    head: u32,
    tail: u32,
}

impl TraceChain {
    const EMPTY: Self = Self {
        head: NO_TRACE,
        tail: NO_TRACE,
    };

    const fn singleton(trace: u32) -> Self {
        Self {
            head: trace,
            tail: trace,
        }
    }

    const fn is_empty(self) -> bool {
        self.head == NO_TRACE
    }
}

#[derive(Clone, Copy, Debug)]
enum TraceHoldDecision {
    None,
    Store,
    Swap,
}

impl TraceHoldDecision {
    fn from_label(label: &'static str) -> Self {
        match label {
            "none" => Self::None,
            "store" => Self::Store,
            "swap" => Self::Swap,
            _ => panic!("unknown forward-search hold decision"),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Store => "store",
            Self::Swap => "swap",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TraceStep {
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
    hold: TraceHoldDecision,
    cleared_lines: u8,
    spin: Option<(PieceKind, bool)>,
    damage: u32,
}

impl TraceStep {
    fn compact(step: ForwardPathStep) -> Self {
        Self {
            piece: step.piece(),
            rotation: step.placement_rotation(),
            x: step.x(),
            y: step.y(),
            hold: TraceHoldDecision::from_label(step.hold_decision()),
            cleared_lines: step.cleared_lines(),
            spin: step.spin().map(|(piece, mini)| {
                (
                    PieceKind::from_ascii(piece).expect("standard forward-search spin piece"),
                    mini,
                )
            }),
            damage: step.damage(),
        }
    }
}

#[derive(Clone, Debug)]
struct TraceNode {
    parent_head: u32,
    next: u32,
    step: TraceStep,
}

const TRACE_CHUNK_SHIFT: usize = 15;
const TRACE_CHUNK_LEN: usize = 1 << TRACE_CHUNK_SHIFT;
const TRACE_CHUNK_MASK: usize = TRACE_CHUNK_LEN - 1;

#[derive(Default)]
struct TraceArena {
    chunks: Vec<Vec<TraceNode>>,
    len: u32,
}

impl TraceArena {
    fn push(&mut self, node: TraceNode) -> u32 {
        let id = self.len;
        assert!(id != u32::MAX, "forward trace arena exceeds u32");
        let chunk_index = id as usize >> TRACE_CHUNK_SHIFT;
        if chunk_index == self.chunks.len() {
            self.chunks.push(Vec::with_capacity(TRACE_CHUNK_LEN));
        }
        self.chunks[chunk_index].push(node);
        self.len += 1;
        id
    }

    fn get(&self, id: u32) -> &TraceNode {
        &self.chunks[id as usize >> TRACE_CHUNK_SHIFT][id as usize & TRACE_CHUNK_MASK]
    }

    fn get_mut(&mut self, id: u32) -> &mut TraceNode {
        &mut self.chunks[id as usize >> TRACE_CHUNK_SHIFT][id as usize & TRACE_CHUNK_MASK]
    }
}

fn trace_placement(piece: PieceKind, rotation: RotationState, x: i8, y: i8) -> ForwardBoard {
    let shape = standard_tetromino_registry()
        .get(piece)
        .expect("standard forward-search piece")
        .shape(rotation);
    let mut mask = ForwardBoard::EMPTY;
    for cell in shape.cells() {
        let cell_x = i16::from(x) + i16::from(cell.x());
        let cell_y = i16::from(y) + i16::from(cell.y());
        debug_assert!((0..10).contains(&cell_x));
        debug_assert!(cell_y >= 0);
        let inserted = mask.insert(cell_y as u16 * 10 + cell_x as u16);
        debug_assert!(inserted);
    }
    mask
}

#[derive(Clone, Copy)]
struct PlacementSource {
    piece: PieceKind,
    cursor_after_selection: u16,
    hold_after_selection: Option<PieceKind>,
    hold_decision: &'static str,
}

pub(crate) enum ExpandedAction {
    Child {
        key: StateKey,
        damage_state: TetrioDamageState,
        step: ForwardPathStep,
    },
    DamageTerminal {
        board: ForwardBoard,
        total_damage: u32,
        step: ForwardPathStep,
    },
    Spin {
        board: ForwardBoard,
        rotation: RotationState,
        x: i8,
        y: i8,
        piece: PieceKind,
        mini: bool,
        lines: u8,
        total_damage: u32,
        step: ForwardPathStep,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SpinOutcomeKey {
    board: ForwardBoard,
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
    lines: u8,
    mini: bool,
    group: ForwardSpinGroup,
}

#[derive(Clone, Copy)]
struct PendingOutcome {
    key: SpinOutcomeKey,
    trace: u32,
    total_damage: u32,
}

#[derive(Clone)]
struct DamageOutcome {
    board: ForwardBoard,
    traces: Vec<u32>,
    total_damage: u32,
}

#[derive(Eq, Hash, PartialEq)]
struct DamagePathKey {
    board: ForwardBoard,
    steps: Vec<(
        PieceKind,
        RotationState,
        i8,
        i8,
        &'static str,
        u8,
        Option<(char, bool)>,
        u32,
    )>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct CanonicalLockOutcomeKey {
    placement_mask: [u64; 4],
    spin: Option<(char, bool)>,
}

#[derive(Clone, Copy)]
pub(crate) struct ForwardSearchConfig {
    pub(crate) board: ForwardBoard,
    pub(crate) height: u8,
    pub(crate) hold_enabled: bool,
    pub(crate) rule_profile: RuleProfileId,
    pub(crate) spin_profile: SpinProfileId,
    pub(crate) initial_combo: Option<u16>,
    pub(crate) initial_back_to_back: Option<u16>,
    pub(crate) line_clear_policy: ForwardLineClearPolicy,
    pub(crate) mode: ForwardSearchMode,
}

impl ForwardSearchConfig {
    pub(crate) fn from_query(query: &ForwardSearchQuery) -> Self {
        Self {
            board: ForwardBoard::from_mask(query.board()),
            height: query.height(),
            hold_enabled: query.hold_enabled(),
            rule_profile: query.rule_profile(),
            spin_profile: query.spin_profile(),
            initial_combo: query.initial_combo(),
            initial_back_to_back: query.initial_back_to_back(),
            line_clear_policy: query.line_clear_policy(),
            mode: query.mode(),
        }
    }
}

pub struct ForwardSearchSession {
    query: ForwardSearchQuery,
    config: ForwardSearchConfig,
    reachability: ReachabilityWorkspace,
    pattern_index: usize,
    active: ForwardQueueSession,
    outcomes: Vec<ForwardSearchOutcome>,
    pub(crate) visited_states: u64,
    pub(crate) generated_locks: u64,
    pub(crate) peak_frontier: usize,
    pub(crate) completed: bool,
}

impl ForwardSearchSession {
    pub fn new(query: ForwardSearchQuery) -> Result<Self, ForwardSearchError> {
        validate_query(&query)?;
        let reachability = ReachabilityWorkspace::new(query.height(), query.rule_profile())
            .map_err(ForwardSearchError::UnsupportedRuleProfile)?;
        let config = ForwardSearchConfig::from_query(&query);
        let queue = query.piece_source().sequence_at(0);
        let active = ForwardQueueSession::new(config, queue, 0);
        Ok(Self {
            query,
            config,
            reachability,
            pattern_index: 0,
            active,
            outcomes: Vec::new(),
            visited_states: 0,
            generated_locks: 0,
            peak_frontier: 1,
            completed: false,
        })
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
    ) -> Result<ForwardSearchAdvance, ForwardSearchError> {
        if self.completed {
            return Ok(ForwardSearchAdvance::Completed(self.build_report()));
        }
        match self
            .active
            .advance(work_budget, control, &mut self.reachability)?
        {
            ForwardSearchAdvance::Pending => Ok(ForwardSearchAdvance::Pending),
            ForwardSearchAdvance::Cancelled => Ok(ForwardSearchAdvance::Cancelled),
            ForwardSearchAdvance::Completed(report) => {
                self.visited_states = self.visited_states.saturating_add(report.visited_states());
                self.generated_locks = self
                    .generated_locks
                    .saturating_add(report.generated_locks());
                self.peak_frontier = self.peak_frontier.max(report.peak_frontier());
                for mut outcome in report.outcomes().iter().cloned() {
                    outcome.assign_id(self.outcomes.len() as u64 + 1);
                    self.outcomes.push(outcome);
                }

                self.pattern_index += 1;
                let pattern_count = self.query.piece_source().pattern_count();
                control.report_progress(
                    "forward-search-patterns",
                    self.pattern_index as u64,
                    Some(pattern_count as u64),
                );
                if self.pattern_index >= pattern_count {
                    self.completed = true;
                    Ok(ForwardSearchAdvance::Completed(self.build_report()))
                } else {
                    let queue = self.query.piece_source().sequence_at(self.pattern_index);
                    self.active = ForwardQueueSession::new(self.config, queue, self.pattern_index);
                    Ok(ForwardSearchAdvance::Pending)
                }
            }
        }
    }

    pub fn run_to_completion(
        mut self,
        control: &ExecutionControl,
    ) -> Result<ForwardSearchReport, ForwardSearchError> {
        loop {
            match self.advance(256, control)? {
                ForwardSearchAdvance::Pending => {}
                ForwardSearchAdvance::Completed(report) => return Ok(report),
                ForwardSearchAdvance::Cancelled => return Err(ForwardSearchError::Cancelled),
            }
        }
    }

    pub(crate) fn build_report(&self) -> ForwardSearchReport {
        ForwardSearchReport::new(
            true,
            self.config.board.words(),
            1,
            self.visited_states,
            self.generated_locks,
            self.peak_frontier,
            self.outcomes.clone(),
        )
    }
}

pub(crate) struct ForwardQueueSession {
    pub(crate) config: ForwardSearchConfig,
    pub(crate) queue: Vec<PieceKind>,
    pub(crate) source_pattern_index: u32,
    pub(crate) current: SearchNodeArena,
    pub(crate) current_cursor: usize,
    pub(crate) next: SearchNodeArena,
    pub(crate) next_index: StateIndexTable,
    boards: ForwardBoardCatalog,
    trace: TraceArena,
    spin_outcomes: BTreeMap<SpinOutcomeKey, PendingOutcome>,
    damage_outcomes: BTreeMap<(u32, ForwardBoard), DamageOutcome>,
    maximum_damage: Option<u32>,
    pub(crate) visited_states: u64,
    pub(crate) generated_locks: u64,
    pub(crate) peak_frontier: usize,
    pub(crate) completed: bool,
    expansion_actions: Vec<ExpandedAction>,
    terminal_actions: Vec<ExpandedAction>,
    seen_lock_outcomes: HashSet<CanonicalLockOutcomeKey>,
}

impl ForwardQueueSession {
    pub(crate) fn new(
        config: ForwardSearchConfig,
        queue: Vec<PieceKind>,
        pattern_index: usize,
    ) -> Self {
        let active = queue[0];
        let initial_damage =
            TetrioDamageState::new(config.initial_combo, config.initial_back_to_back);
        let mut boards = ForwardBoardCatalog::new(config.height);
        let initial_board_id = boards.intern(config.board);
        let initial = SearchNode {
            key: StoredStateKey {
                board_id: initial_board_id,
                active,
                cursor: 1,
                hold: None,
                combo: initial_damage.combo(),
                back_to_back: initial_damage.back_to_back(),
            },
            total_damage: initial_damage.total_damage(),
            traces: TraceChain::EMPTY,
        };
        Self {
            config,
            queue,
            source_pattern_index: u32::try_from(pattern_index).unwrap_or(u32::MAX),
            current: SearchNodeArena::from_node(initial),
            current_cursor: 0,
            next: SearchNodeArena::default(),
            next_index: StateIndexTable::new(config.mode),
            boards,
            trace: TraceArena::default(),
            spin_outcomes: BTreeMap::new(),
            damage_outcomes: BTreeMap::new(),
            maximum_damage: None,
            visited_states: 0,
            generated_locks: 0,
            peak_frontier: 1,
            completed: false,
            expansion_actions: Vec::new(),
            terminal_actions: Vec::new(),
            seen_lock_outcomes: HashSet::new(),
        }
    }

    pub(crate) fn advance(
        &mut self,
        work_budget: usize,
        control: &ExecutionControl,
        reachability: &mut ReachabilityWorkspace,
    ) -> Result<ForwardSearchAdvance, ForwardSearchError> {
        if self.completed {
            return Ok(ForwardSearchAdvance::Completed(self.build_report()));
        }
        let budget = work_budget.max(1);
        let mut processed = 0_usize;
        while processed < budget {
            if control.is_cancelled() {
                return Ok(ForwardSearchAdvance::Cancelled);
            }
            if self.current_cursor >= self.current.len() {
                if self.next.is_empty() {
                    self.completed = true;
                    return Ok(ForwardSearchAdvance::Completed(self.build_report()));
                }
                std::mem::swap(&mut self.current, &mut self.next);
                self.next.clear();
                self.current_cursor = 0;
                self.next_index.clear();
                self.peak_frontier = self.peak_frontier.max(self.current.len());
            }
            let node = self.current[self.current_cursor].clone();
            self.current_cursor += 1;
            self.visited_states = self.visited_states.saturating_add(1);
            let generated_locks = expand_search_node(
                self.config,
                &self.queue,
                self.materialize_state_key(node.key),
                node.damage_state(),
                control,
                reachability,
                &mut self.expansion_actions,
                &mut self.seen_lock_outcomes,
            )?;
            self.generated_locks = self.generated_locks.saturating_add(generated_locks);
            let mut actions = std::mem::take(&mut self.expansion_actions);
            for action in actions.drain(..) {
                self.absorb_expanded_action(node.traces, action, control, reachability)?;
            }
            self.expansion_actions = actions;
            processed += 1;
        }
        control.report_progress("forward-search", self.visited_states, None);
        Ok(ForwardSearchAdvance::Pending)
    }

    pub(crate) fn terminal_fusion_active(&self) -> bool {
        self.current.len() >= TERMINAL_FUSION_MIN_PARENT_STATES
            && self.current_cursor < self.current.len()
            && usize::from(self.current[self.current_cursor].key.cursor) >= self.queue.len()
    }

    pub(crate) fn absorb_expanded_action(
        &mut self,
        parents: TraceChain,
        action: ExpandedAction,
        control: &ExecutionControl,
        reachability: &mut ReachabilityWorkspace,
    ) -> Result<(), ForwardSearchError> {
        match action {
            ExpandedAction::Child {
                key,
                damage_state,
                step,
            } if self.current.len() >= TERMINAL_FUSION_MIN_PARENT_STATES
                && usize::from(key.cursor) >= self.queue.len() =>
            {
                self.expand_terminal_child(
                    parents,
                    key,
                    damage_state,
                    step,
                    control,
                    reachability,
                )?;
            }
            action => self.absorb_action(parents, action),
        }
        Ok(())
    }

    fn expand_terminal_child(
        &mut self,
        parents: TraceChain,
        key: StateKey,
        damage_state: TetrioDamageState,
        step: ForwardPathStep,
        control: &ExecutionControl,
        reachability: &mut ReachabilityWorkspace,
    ) -> Result<(), ForwardSearchError> {
        let mut terminal_actions = std::mem::take(&mut self.terminal_actions);
        let generated_locks = expand_search_node(
            self.config,
            &self.queue,
            key,
            damage_state,
            control,
            reachability,
            &mut terminal_actions,
            &mut self.seen_lock_outcomes,
        )?;
        self.visited_states = self.visited_states.saturating_add(1);
        self.generated_locks = self.generated_locks.saturating_add(generated_locks);

        let record_prefix = matches!(self.config.mode, ForwardSearchMode::MaximumDamage)
            && (parents.is_empty() || step.damage() > 0)
            && self
                .maximum_damage
                .is_none_or(|best| damage_state.total_damage() >= best);
        let retain_terminal = terminal_actions
            .iter()
            .any(|action| self.terminal_action_is_retainable(action));
        if record_prefix || retain_terminal {
            let trace = self.push_trace(parents, step);
            let child_parents = TraceChain::singleton(trace);
            if record_prefix {
                self.record_damage_candidate(key.board, trace, damage_state.total_damage());
            }
            for action in terminal_actions.drain(..) {
                if self.terminal_action_is_retainable(&action) {
                    self.absorb_action(child_parents, action);
                }
            }
        } else {
            terminal_actions.clear();
        }
        self.terminal_actions = terminal_actions;
        Ok(())
    }

    fn terminal_action_is_retainable(&self, action: &ExpandedAction) -> bool {
        match action {
            ExpandedAction::DamageTerminal {
                total_damage, step, ..
            } => match self.config.mode {
                ForwardSearchMode::MaximumDamage => {
                    step.damage() > 0
                        && self.maximum_damage.is_none_or(|best| *total_damage >= best)
                }
                ForwardSearchMode::DamageAtLeast(minimum) => *total_damage >= minimum,
                ForwardSearchMode::SpinFinder(_) => false,
            },
            ExpandedAction::Spin { .. } | ExpandedAction::Child { .. } => true,
        }
    }

    pub(crate) fn absorb_action(&mut self, parents: TraceChain, action: ExpandedAction) {
        match action {
            ExpandedAction::Child {
                key,
                damage_state,
                step,
            } => self.insert_child(key, damage_state, parents, step),
            ExpandedAction::DamageTerminal {
                board,
                total_damage,
                step,
            } => self.record_damage_terminal(board, parents, step, total_damage),
            ExpandedAction::Spin {
                board,
                rotation,
                x,
                y,
                piece,
                mini,
                lines,
                total_damage,
                step,
            } => {
                let trace = self.push_trace(parents, step);
                self.record_spin_outcome(
                    board,
                    rotation,
                    x,
                    y,
                    piece,
                    mini,
                    lines,
                    trace,
                    total_damage,
                );
            }
        }
    }

    fn push_trace(&mut self, parents: TraceChain, step: ForwardPathStep) -> u32 {
        self.trace.push(TraceNode {
            parent_head: parents.head,
            next: NO_TRACE,
            step: TraceStep::compact(step),
        })
    }

    fn append_trace(&mut self, chain: &mut TraceChain, trace: u32) {
        if chain.is_empty() {
            *chain = TraceChain::singleton(trace);
            return;
        }
        self.trace.get_mut(chain.tail).next = trace;
        chain.tail = trace;
    }

    fn insert_child(
        &mut self,
        key: StateKey,
        damage_state: TetrioDamageState,
        parents: TraceChain,
        step: ForwardPathStep,
    ) {
        let board = key.board;
        let record_maximum_prefix = matches!(self.config.mode, ForwardSearchMode::MaximumDamage)
            && (parents.is_empty() || step.damage() > 0);
        let stored_key = self.store_state_key(key);
        let index_key = state_index_key(self.config.mode, stored_key, damage_state);
        let trace = if let Some(index) = self.next_index.get(&index_key, &self.next) {
            let candidate_damage = damage_state.total_damage();
            let existing_damage = self.next[index].total_damage;
            if candidate_damage < existing_damage {
                return;
            }
            if candidate_damage > existing_damage {
                let trace = self.push_trace(parents, step);
                self.next[index] = SearchNode {
                    key: stored_key,
                    total_damage: candidate_damage,
                    traces: TraceChain::singleton(trace),
                };
                trace
            } else if self.config.mode.is_damage() {
                let trace = self.push_trace(parents, step);
                let mut traces = self.next[index].traces;
                self.append_trace(&mut traces, trace);
                self.next[index].traces = traces;
                trace
            } else {
                return;
            }
        } else {
            self.next_index.reserve_for_insert(&self.next);
            let index = self.next.len();
            let trace = self.push_trace(parents, step);
            self.next.push(SearchNode {
                key: stored_key,
                total_damage: damage_state.total_damage(),
                traces: TraceChain::singleton(trace),
            });
            self.next_index.insert(index_key, index);
            trace
        };
        if record_maximum_prefix {
            self.record_damage_candidate(board, trace, damage_state.total_damage());
        }
    }

    pub(crate) fn materialize_state_key(&self, key: StoredStateKey) -> StateKey {
        StateKey {
            board: self.boards.get(key.board_id),
            active: key.active,
            cursor: key.cursor,
            hold: key.hold,
            combo: key.combo,
            back_to_back: key.back_to_back,
        }
    }

    fn store_state_key(&mut self, key: StateKey) -> StoredStateKey {
        StoredStateKey {
            board_id: self.boards.intern(key.board),
            active: key.active,
            cursor: key.cursor,
            hold: key.hold,
            combo: key.combo,
            back_to_back: key.back_to_back,
        }
    }

    fn record_damage_terminal(
        &mut self,
        board: ForwardBoard,
        parents: TraceChain,
        step: ForwardPathStep,
        total_damage: u32,
    ) {
        match self.config.mode {
            ForwardSearchMode::MaximumDamage => {
                if (!parents.is_empty() && step.damage() == 0)
                    || self.maximum_damage.is_some_and(|best| total_damage < best)
                {
                    return;
                }
            }
            ForwardSearchMode::DamageAtLeast(minimum) => {
                if total_damage < minimum {
                    return;
                }
            }
            ForwardSearchMode::SpinFinder(_) => return,
        }
        let trace = self.push_trace(parents, step);
        self.record_damage_candidate(board, trace, total_damage);
    }

    fn record_damage_candidate(&mut self, board: ForwardBoard, trace: u32, total_damage: u32) {
        match self.config.mode {
            ForwardSearchMode::MaximumDamage => {
                if self.maximum_damage.is_some_and(|best| total_damage < best) {
                    return;
                }
                if self.maximum_damage.is_none_or(|best| total_damage > best) {
                    self.maximum_damage = Some(total_damage);
                    self.damage_outcomes.clear();
                }
            }
            ForwardSearchMode::DamageAtLeast(_) => {
                self.maximum_damage = Some(
                    self.maximum_damage
                        .map_or(total_damage, |best| best.max(total_damage)),
                );
            }
            ForwardSearchMode::SpinFinder(_) => return,
        }
        self.damage_outcomes
            .entry((total_damage, board))
            .or_insert_with(|| DamageOutcome {
                board,
                traces: Vec::new(),
                total_damage,
            })
            .traces
            .push(trace);
    }

    fn record_spin_outcome(
        &mut self,
        board: ForwardBoard,
        rotation: RotationState,
        x: i8,
        y: i8,
        piece: PieceKind,
        mini: bool,
        lines: u8,
        trace: u32,
        total_damage: u32,
    ) {
        let group = spin_group(self.config.spin_profile, piece.as_ascii());
        let key = SpinOutcomeKey {
            board,
            piece,
            rotation: canonical_result_rotation(piece, rotation),
            x,
            y,
            lines,
            mini,
            group,
        };
        self.spin_outcomes.entry(key).or_insert(PendingOutcome {
            key,
            trace,
            total_damage,
        });
    }

    pub(crate) fn build_report(&self) -> ForwardSearchReport {
        let outcomes = match self.config.mode {
            ForwardSearchMode::MaximumDamage | ForwardSearchMode::DamageAtLeast(_) => {
                let mut outcomes = Vec::new();
                let mut seen = HashSet::new();
                for best in self.damage_outcomes.values() {
                    for trace in &best.traces {
                        for path in self.reconstruct_paths(*trace) {
                            let key = DamagePathKey {
                                board: best.board,
                                steps: path
                                    .iter()
                                    .map(|step| {
                                        (
                                            step.piece(),
                                            step.rotation(),
                                            step.x(),
                                            step.y(),
                                            step.hold_decision(),
                                            step.cleared_lines(),
                                            step.spin(),
                                            step.damage(),
                                        )
                                    })
                                    .collect(),
                            };
                            if !seen.insert(key) {
                                continue;
                            }
                            outcomes.push(ForwardSearchOutcome::new(
                                outcomes.len() as u64 + 1,
                                self.source_pattern_index,
                                self.queue.clone(),
                                None,
                                best.board.words(),
                                None,
                                false,
                                0,
                                best.total_damage,
                                path,
                            ));
                        }
                    }
                }
                if matches!(self.config.mode, ForwardSearchMode::MaximumDamage) {
                    outcomes.sort_by_key(|outcome| outcome.path().len());
                }
                outcomes
            }
            ForwardSearchMode::SpinFinder(_) => self
                .spin_outcomes
                .values()
                .enumerate()
                .map(|(index, pending)| {
                    ForwardSearchOutcome::new(
                        index as u64 + 1,
                        self.source_pattern_index,
                        self.queue.clone(),
                        Some(pending.key.group),
                        pending.key.board.words(),
                        Some(pending.key.piece),
                        pending.key.mini,
                        pending.key.lines,
                        pending.total_damage,
                        self.reconstruct_first_path(pending.trace),
                    )
                })
                .collect(),
        };
        ForwardSearchReport::new(
            true,
            self.config.board.words(),
            1,
            self.visited_states,
            self.generated_locks,
            self.peak_frontier,
            outcomes,
        )
    }

    fn reconstruct_first_path(&self, mut trace: u32) -> Vec<ForwardPathStep> {
        let mut path = Vec::new();
        loop {
            let node = self.trace.get(trace);
            path.push(node.step);
            if node.parent_head == NO_TRACE {
                break;
            }
            trace = node.parent_head;
        }
        path.reverse();
        self.materialize_path(path)
    }

    fn reconstruct_paths(&self, trace: u32) -> Vec<Vec<ForwardPathStep>> {
        self.reconstruct_compact_paths(trace)
            .into_iter()
            .map(|path| self.materialize_path(path))
            .collect()
    }

    fn reconstruct_compact_paths(&self, trace: u32) -> Vec<Vec<TraceStep>> {
        let node = self.trace.get(trace);
        if node.parent_head == NO_TRACE {
            return vec![vec![node.step]];
        }
        let mut paths = Vec::new();
        let mut parent = node.parent_head;
        while parent != NO_TRACE {
            for mut path in self.reconstruct_compact_paths(parent) {
                path.push(node.step);
                paths.push(path);
            }
            parent = self.trace.get(parent).next;
        }
        paths
    }

    fn materialize_path(&self, compact: Vec<TraceStep>) -> Vec<ForwardPathStep> {
        let mut board = self.config.board;
        let mut total_damage = 0_u32;
        compact
            .into_iter()
            .map(|step| {
                let placement = trace_placement(step.piece, step.rotation, step.x, step.y);
                let placed = board.union_for_height(placement, self.config.height);
                let (board_after, cleared_row_mask, cleared_lines) =
                    place_and_clear(10, self.config.height, placed);
                debug_assert_eq!(cleared_lines, step.cleared_lines);
                total_damage = total_damage.saturating_add(step.damage);
                board = board_after;
                ForwardPathStep::new(
                    step.piece,
                    canonical_result_rotation(step.piece, step.rotation),
                    canonical_result_rotation(step.piece, step.rotation),
                    step.x,
                    step.y,
                    step.hold.label(),
                    step.cleared_lines,
                    step.spin.map(|(piece, mini)| (piece.as_ascii(), mini)),
                    step.damage,
                    total_damage,
                    placement.words(),
                    cleared_row_mask,
                    board_after.words(),
                )
            })
            .collect()
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn expand_search_node(
    config: ForwardSearchConfig,
    queue: &[PieceKind],
    key: StateKey,
    damage_state: TetrioDamageState,
    control: &ExecutionControl,
    reachability: &mut ReachabilityWorkspace,
    actions: &mut Vec<ExpandedAction>,
    seen_lock_outcomes: &mut HashSet<CanonicalLockOutcomeKey>,
) -> Result<u64, ForwardSearchError> {
    actions.clear();
    let mut generated_locks = 0_u64;
    let spin_profile = SpinProfile::builtin(config.spin_profile);
    let t_spin_acceleration = TSpinAcceleration::for_search(config.mode, config.spin_profile);
    let retain_matching_continuations = matches!(
        config.mode,
        ForwardSearchMode::SpinFinder(target)
            if matches!(
                target.line_requirement(),
                ForwardSpinLineRequirement::AtLeast(_)
            )
    );
    if t_spin_acceleration
        .is_some_and(|acceleration| !acceleration.state_can_reach_target(queue, key))
    {
        return Ok(0);
    }
    for source in placement_sources(config.hold_enabled, queue, key) {
        if control.is_cancelled() {
            return Err(ForwardSearchError::Cancelled);
        }
        seen_lock_outcomes.clear();
        let retain_rotation_evidence =
            rotation_evidence_affects_result(config.spin_profile, source.piece);
        let measure_immobility = immobility_affects_result(config.spin_profile, source.piece);
        let locks = reachability.reachable_locks(
            key.board,
            source.piece,
            retain_rotation_evidence,
            measure_immobility,
        );
        generated_locks = generated_locks.saturating_add(locks.len() as u64);
        for lock in locks.iter().copied() {
            let placed = key.board.union_for_height(lock.mask, config.height);
            let needs_t_corners = t_spin_acceleration.map_or_else(
                || t_corners_affect_result(config.spin_profile, source.piece),
                |acceleration| acceleration.needs_corner_counts(source.piece, lock.evidence),
            );
            let (blocked_corners, blocked_front) = if needs_t_corners {
                t_corner_counts(
                    key.board,
                    config.height,
                    source.piece,
                    lock.rotation,
                    lock.x,
                    lock.y,
                )
            } else {
                (0, 0)
            };
            let (board_after, cleared_rows, cleared_lines) =
                place_and_clear(10, config.height, placed);
            let perfect_clear = board_after.is_empty() && cleared_lines > 0;
            let edge = ScoringExecutionEdge::new(
                0,
                0,
                source.piece,
                lock.rotation,
                lock.x,
                lock.y,
                cleared_lines,
                blocked_corners,
                blocked_front,
                lock.evidence.scoring(lock.rotation, lock.immobile),
            )
            .with_perfect_clear(perfect_clear);
            let preservation_requires_spin = matches!(
                config.line_clear_policy,
                ForwardLineClearPolicy::PreserveBackToBack
            )
                && BackToBackPreservationPolicy::requires_recognized_spin(edge);
            let needs_exact_spin_confirmation = t_spin_acceleration.map_or(true, |acceleration| {
                acceleration.needs_exact_confirmation(
                    source.piece,
                    lock.evidence,
                    lock.immobile,
                    blocked_corners,
                )
            }) || preservation_requires_spin;
            let spin = needs_exact_spin_confirmation
                .then(|| SpinDetector::detect_scoring_edge_with_profile(edge, spin_profile))
                .flatten();
            if preservation_requires_spin && spin.is_none() {
                continue;
            }
            let matching_spin = match config.mode {
                ForwardSearchMode::SpinFinder(target) => {
                    spin.filter(|event| spin_matches(target, *event))
                }
                ForwardSearchMode::MaximumDamage | ForwardSearchMode::DamageAtLeast(_) => None,
            };
            let (next_piece, next_cursor, next_hold) = next_active(
                queue,
                source.cursor_after_selection,
                source.hold_after_selection,
            );
            let continuation_can_reach_target = next_piece.is_some_and(|active| {
                t_spin_acceleration.is_none_or(|acceleration| {
                    acceleration.supply_can_reach_target(queue, active, next_cursor, next_hold)
                })
            });
            if matches!(config.mode, ForwardSearchMode::SpinFinder(_))
                && matching_spin.is_none()
                && !continuation_can_reach_target
            {
                continue;
            }
            let damage = TetrioDamageProfile.evaluate(
                damage_state,
                TetrioDamageAction::from_clear(cleared_lines, spin),
                perfect_clear,
            );
            if !seen_lock_outcomes.insert(CanonicalLockOutcomeKey {
                placement_mask: lock.mask.words(),
                spin: spin.map(|event| (event.piece(), event.is_mini())),
            }) {
                continue;
            }
            let step = ForwardPathStep::new(
                source.piece,
                canonical_result_rotation(source.piece, lock.rotation),
                lock.rotation,
                lock.x,
                lock.y,
                source.hold_decision,
                cleared_lines,
                spin.map(|event| (event.piece(), event.is_mini())),
                damage.damage(),
                damage.state().total_damage(),
                lock.mask.words(),
                cleared_rows,
                board_after.words(),
            );
            if let Some(spin) = matching_spin {
                let spin_action = |step| ExpandedAction::Spin {
                    board: board_after,
                    rotation: lock.rotation,
                    x: lock.x,
                    y: lock.y,
                    piece: PieceKind::from_ascii(spin.piece()).expect("standard spin piece"),
                    mini: spin.is_mini(),
                    lines: spin.lines(),
                    total_damage: damage.state().total_damage(),
                    step,
                };
                if retain_matching_continuations {
                    actions.push(spin_action(step.clone()));
                } else {
                    actions.push(spin_action(step));
                    continue;
                }
            }

            if let Some(active) = next_piece {
                let child_key = StateKey {
                    board: board_after,
                    active,
                    cursor: next_cursor,
                    hold: next_hold,
                    combo: damage.state().combo(),
                    back_to_back: damage.state().back_to_back(),
                };
                if t_spin_acceleration.is_none_or(|acceleration| {
                    acceleration.state_can_reach_target(queue, child_key)
                }) {
                    actions.push(ExpandedAction::Child {
                        key: child_key,
                        damage_state: damage.state(),
                        step,
                    });
                }
            } else if config.mode.is_damage() {
                actions.push(ExpandedAction::DamageTerminal {
                    board: board_after,
                    total_damage: damage.state().total_damage(),
                    step,
                });
            }
        }
    }
    Ok(generated_locks)
}

fn state_index_key(
    mode: ForwardSearchMode,
    state: StoredStateKey,
    damage: TetrioDamageState,
) -> StateIndexKey {
    StateIndexKey {
        state,
        total_damage: if matches!(mode, ForwardSearchMode::DamageAtLeast(_)) {
            damage.total_damage()
        } else {
            0
        },
    }
}

fn rotation_evidence_affects_result(profile: SpinProfileId, piece: PieceKind) -> bool {
    if piece == PieceKind::T {
        matches!(
            profile,
            SpinProfileId::TSpins
                | SpinProfileId::TSpinsPlus
                | SpinProfileId::AllSpin
                | SpinProfileId::AllSpinPlus
                | SpinProfileId::AllMini
                | SpinProfileId::AllMiniPlus
        )
    } else {
        profile.recognizes_non_t_immobile_spins()
    }
}

fn immobility_affects_result(profile: SpinProfileId, piece: PieceKind) -> bool {
    if piece == PieceKind::T {
        profile.allows_immobile_t_fallback()
    } else {
        profile.recognizes_non_t_immobile_spins()
    }
}

fn t_corners_affect_result(profile: SpinProfileId, piece: PieceKind) -> bool {
    piece == PieceKind::T
        && !matches!(
            profile,
            SpinProfileId::Disabled | SpinProfileId::TSpinSimple
        )
}

pub(crate) fn validate_query(query: &ForwardSearchQuery) -> Result<(), ForwardSearchError> {
    if query.piece_source().sequence_len() == 0 {
        return Err(ForwardSearchError::EmptyQueue);
    }
    if query.piece_source().is_pattern() {
        if !matches!(query.mode(), ForwardSearchMode::SpinFinder(_)) {
            return Err(ForwardSearchError::PatternRequiresSpinFinder);
        }
    }
    if !(1..=24).contains(&query.height()) {
        return Err(ForwardSearchError::InvalidHeight);
    }
    if !query
        .board()
        .fits_cell_count(u16::from(query.height()) * 10)
        .unwrap_or(false)
    {
        return Err(ForwardSearchError::BoardOutsideField);
    }
    if matches!(query.mode(), ForwardSearchMode::SpinFinder(_))
        && query.spin_profile() == SpinProfileId::Disabled
    {
        return Err(ForwardSearchError::SpinProfileDisabled);
    }
    Ok(())
}

fn placement_sources(
    hold_enabled: bool,
    queue: &[PieceKind],
    key: StateKey,
) -> impl Iterator<Item = PlacementSource> {
    let mut sources = [None, None];
    sources[0] = Some(PlacementSource {
        piece: key.active,
        cursor_after_selection: key.cursor,
        hold_after_selection: key.hold,
        hold_decision: "none",
    });
    sources[1] = if !hold_enabled {
        None
    } else {
        match key.hold {
            Some(held) if held != key.active => Some(PlacementSource {
                piece: held,
                cursor_after_selection: key.cursor,
                hold_after_selection: Some(key.active),
                hold_decision: "swap",
            }),
            Some(_) => None,
            None if usize::from(key.cursor) < queue.len() => Some(PlacementSource {
                piece: queue[usize::from(key.cursor)],
                cursor_after_selection: key.cursor.saturating_add(1),
                hold_after_selection: Some(key.active),
                hold_decision: "store",
            }),
            None => None,
        }
    };
    sources.into_iter().flatten()
}

fn next_active(
    queue: &[PieceKind],
    cursor: u16,
    hold: Option<PieceKind>,
) -> (Option<PieceKind>, u16, Option<PieceKind>) {
    if let Some(piece) = queue.get(usize::from(cursor)).copied() {
        return (Some(piece), cursor.saturating_add(1), hold);
    }
    hold.map_or((None, cursor, None), |piece| (Some(piece), cursor, None))
}

fn spin_matches(target: ForwardSpinTarget, spin: SpinEvent) -> bool {
    if !target.matches_lines(spin.lines()) {
        return false;
    }
    match target.category() {
        ForwardSpinCategory::Any => true,
        ForwardSpinCategory::T => spin.piece() == 'T',
        ForwardSpinCategory::Other => spin.piece() != 'T',
    }
}

fn spin_group(profile: SpinProfileId, piece: char) -> ForwardSpinGroup {
    match profile {
        SpinProfileId::AllMini | SpinProfileId::AllMiniPlus => {
            if piece == 'T' {
                ForwardSpinGroup::T
            } else {
                ForwardSpinGroup::Other
            }
        }
        SpinProfileId::AllSpin | SpinProfileId::AllSpinPlus => ForwardSpinGroup::Integrated,
        SpinProfileId::Disabled
        | SpinProfileId::TSpinSimple
        | SpinProfileId::TSpins
        | SpinProfileId::TSpinsPlus => ForwardSpinGroup::T,
    }
}

fn canonical_result_rotation(piece: PieceKind, rotation: RotationState) -> RotationState {
    match piece {
        PieceKind::O => RotationState::Zero,
        PieceKind::I | PieceKind::S | PieceKind::Z => match rotation {
            RotationState::Two => RotationState::Zero,
            RotationState::Left => RotationState::Right,
            other => other,
        },
        PieceKind::T | PieceKind::J | PieceKind::L => rotation,
    }
}

fn t_corner_counts(
    board: ForwardBoard,
    height: u8,
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
) -> (u8, u8) {
    if piece != PieceKind::T {
        return (0, 0);
    }
    let (center_x, center_y) = match rotation {
        RotationState::Zero => (i16::from(x) + 1, i16::from(y)),
        RotationState::Right => (i16::from(x), i16::from(y) + 1),
        RotationState::Two | RotationState::Left => (i16::from(x) + 1, i16::from(y) + 1),
    };
    let corners = [(-1, -1), (1, -1), (-1, 1), (1, 1)];
    let front = match rotation {
        RotationState::Zero => [(-1, 1), (1, 1)],
        RotationState::Right => [(1, -1), (1, 1)],
        RotationState::Two => [(-1, -1), (1, -1)],
        RotationState::Left => [(-1, -1), (-1, 1)],
    };
    let blocked = corners
        .into_iter()
        .filter(|(dx, dy)| corner_blocked(board, height, center_x + dx, center_y + dy))
        .count() as u8;
    let blocked_front = front
        .into_iter()
        .filter(|(dx, dy)| corner_blocked(board, height, center_x + dx, center_y + dy))
        .count() as u8;
    (blocked, blocked_front)
}

fn corner_blocked(board: ForwardBoard, height: u8, x: i16, y: i16) -> bool {
    if x < 0 || x >= 10 || y < 0 {
        return true;
    }
    if y >= i16::from(height) {
        return false;
    }
    board.contains(y as u16 * 10 + x as u16)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionCancellationToken,
    };
    use clearra_rules::profile::rule_profile::RuleProfileId;
    use clearra_scoring::profile::SpinProfileId;
    use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

    use super::*;
    use crate::query::ForwardPieceSource;

    fn run(query: ForwardSearchQuery) -> ForwardSearchReport {
        let token = ExecutionCancellationToken::new();
        let control = ExecutionControl::new(token);
        ForwardSearchSession::new(query)
            .expect("session")
            .run_to_completion(&control)
            .expect("search")
    }

    #[test]
    fn fixed_queue_forward_damage_search_returns_a_real_terminal_path() {
        let query = ForwardSearchQuery::new(
            Board256Mask::EMPTY,
            8,
            vec![PieceKind::I, PieceKind::O],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        );
        let report = run(query);
        assert_eq!(report.maximum_damage(), Some(0));
        assert!(report.complete());
        assert!(!report.outcomes().is_empty());
        assert!(report
            .outcomes()
            .iter()
            .all(|outcome| outcome.path().len() == 1));
    }

    #[test]
    fn damage_search_preserves_all_distinct_routes_tied_for_the_maximum() {
        let query = ForwardSearchQuery::new(
            Board256Mask::EMPTY,
            8,
            vec![PieceKind::O],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        );

        let report = run(query);
        assert_eq!(report.maximum_damage(), Some(0));
        assert!(report.outcomes().len() > 1);
        let routes = report
            .outcomes()
            .iter()
            .map(|outcome| {
                let step = &outcome.path()[0];
                (step.rotation(), step.x(), step.y())
            })
            .collect::<HashSet<_>>();
        assert_eq!(routes.len(), report.outcomes().len());
    }

    #[test]
    fn hold_store_consumes_the_next_queue_piece_without_reordering_source_state() {
        let query = ForwardSearchQuery::new(
            Board256Mask::EMPTY,
            8,
            vec![PieceKind::T, PieceKind::I],
            true,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        );
        let report = run(query);
        assert!(report
            .outcomes()
            .iter()
            .all(|outcome| outcome.path().len() == 1));
        assert!(report.outcomes()[0]
            .path()
            .iter()
            .all(|step| matches!(step.hold_decision(), "none" | "store" | "swap")));
    }

    #[test]
    fn spin_finder_rejects_disabled_spin_profile() {
        let query = ForwardSearchQuery::new(
            Board256Mask::EMPTY,
            8,
            vec![PieceKind::T],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::Disabled,
            None,
            None,
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::default()),
        );
        assert!(matches!(
            ForwardSearchSession::new(query),
            Err(ForwardSearchError::SpinProfileDisabled)
        ));
    }

    #[test]
    fn forward_damage_search_scores_a_real_double() {
        let row_without_left_cell = ((1_u64 << 10) - 1) & !1_u64;
        let board = row_without_left_cell | (row_without_left_cell << 10);
        let query = ForwardSearchQuery::new(
            Board256Mask::from_words([board, 0, 0, 0]),
            4,
            vec![PieceKind::I],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        );

        let report = run(query);
        assert_eq!(report.maximum_damage(), Some(1));
        assert_eq!(report.outcomes()[0].path()[0].cleared_lines(), 2);
    }

    #[test]
    fn damage_threshold_preserves_lower_scoring_terminal_routes() {
        let row_without_left_cell = ((1_u64 << 10) - 1) & !1_u64;
        let board = row_without_left_cell | (row_without_left_cell << 10);
        let report = run(ForwardSearchQuery::new(
            Board256Mask::from_words([board, 0, 0, 0]),
            4,
            vec![PieceKind::I],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::AllMiniPlus,
            None,
            None,
            ForwardSearchMode::DamageAtLeast(0),
        ));

        let damages = report
            .outcomes()
            .iter()
            .map(ForwardSearchOutcome::total_damage)
            .collect::<HashSet<_>>();
        assert_eq!(report.maximum_damage(), Some(1));
        assert!(damages.contains(&0));
        assert!(damages.contains(&1));
    }

    #[test]
    fn damage_search_finds_the_oljtt_eight_damage_fixture_without_hold() {
        let board = 0x38f_u64
            | (0x387_u64 << 10)
            | (0x303_u64 << 20)
            | (0x303_u64 << 30)
            | (0x300_u64 << 40);
        let query = ForwardSearchQuery::new(
            Board256Mask::from_words([board, 0, 0, 0]),
            8,
            vec![
                PieceKind::O,
                PieceKind::L,
                PieceKind::J,
                PieceKind::T,
                PieceKind::T,
            ],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        );

        let report = run(query);
        assert_eq!(report.maximum_damage(), Some(8));
        assert_eq!(report.outcomes().len(), 1);
        assert!(report
            .outcomes()
            .iter()
            .all(|outcome| outcome.total_damage() == 8));
        assert!(report
            .outcomes()
            .iter()
            .all(|outcome| outcome.path().len() == 5));
        assert_eq!(report.outcomes()[0].path()[0].piece(), PieceKind::O);
        assert_eq!(
            report.outcomes()[0].path()[0].rotation(),
            RotationState::Zero
        );
    }

    #[test]
    fn spin_finder_reports_a_reachable_zero_line_t_spin() {
        let blockers = (1_u64 << 0) | (1_u64 << 2) | (1_u64 << 20);
        let query = ForwardSearchQuery::new(
            Board256Mask::from_words([blockers, 0, 0, 0]),
            4,
            vec![PieceKind::T],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::default()),
        );

        let report = run(query);
        assert!(!report.outcomes().is_empty());
        assert!(report
            .outcomes()
            .iter()
            .all(|outcome| outcome.spin_piece() == Some(PieceKind::T)));
    }

    #[test]
    fn spin_result_grouping_follows_the_selected_profile() {
        assert_eq!(spin_group(SpinProfileId::AllMini, 'T'), ForwardSpinGroup::T);
        assert_eq!(
            spin_group(SpinProfileId::AllMiniPlus, 'J'),
            ForwardSpinGroup::Other
        );
        assert_eq!(
            spin_group(SpinProfileId::AllSpin, 'T'),
            ForwardSpinGroup::Integrated
        );
        assert_eq!(
            spin_group(SpinProfileId::AllSpinPlus, 'L'),
            ForwardSpinGroup::Integrated
        );
    }

    #[test]
    fn all_mini_rejects_open_roof_non_t_rotation() {
        let right_blockers = (1_u64 << 2) | (1_u64 << 12);
        let query = ForwardSearchQuery::new(
            Board256Mask::from_words([right_blockers, 0, 0, 0]),
            4,
            vec![PieceKind::O],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::AllMiniPlus,
            None,
            None,
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::new(
                Some(0),
                ForwardSpinCategory::Other,
            )),
        );

        let report = run(query);
        assert!(report.outcomes().is_empty());
    }

    #[test]
    fn spin_finder_runs_each_concrete_pattern_as_an_exact_queue() {
        let t_spin_blockers = (1_u64 << 0) | (1_u64 << 2) | (1_u64 << 20);
        let pattern = QueuePatternExpression::parse("[OT]", 8).expect("pattern");
        let query = ForwardSearchQuery::new_with_source(
            Board256Mask::from_words([t_spin_blockers, 0, 0, 0]),
            4,
            ForwardPieceSource::pattern(pattern),
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::AllMiniPlus,
            None,
            None,
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::new(
                Some(0),
                ForwardSpinCategory::Any,
            )),
        );

        let report = run(query);
        assert!(report.outcomes().iter().any(|outcome| {
            outcome.source_queue() == [PieceKind::T]
                && outcome.source_pattern_index() < 2
                && outcome.spin_piece() == Some(PieceKind::T)
        }));
    }

    #[test]
    fn pattern_source_accepts_more_than_the_gui_piece_limit_in_spin_finder_mode() {
        let long_pattern = QueuePatternExpression::parse("IOTSZLJIO", 8).expect("pattern");
        let long_query = ForwardSearchQuery::new_with_source(
            Board256Mask::EMPTY,
            8,
            ForwardPieceSource::pattern(long_pattern),
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::default()),
        );
        assert!(ForwardSearchSession::new(long_query).is_ok());

        let damage_pattern = QueuePatternExpression::parse("[TI]", 8).expect("pattern");
        let damage_query = ForwardSearchQuery::new_with_source(
            Board256Mask::EMPTY,
            8,
            ForwardPieceSource::pattern(damage_pattern),
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        );
        assert!(matches!(
            ForwardSearchSession::new(damage_query),
            Err(ForwardSearchError::PatternRequiresSpinFinder)
        ));
    }
}
// SRP rationale: this module has one behavior-level change reason: exact forward lock-search state expansion and canonical output.
