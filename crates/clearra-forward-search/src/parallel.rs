use std::collections::{BTreeMap, HashMap, HashSet};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::{damage::TetrioDamageState, profile::SpinProfileId};

use crate::{
    board::ForwardBoard,
    query::{
        ForwardLineClearPolicy, ForwardSearchMode, ForwardSearchQuery, ForwardSpinCategory,
        ForwardSpinLineRequirement, ForwardSpinTarget,
    },
    reachability::ReachabilityWorkspace,
    result::{ForwardPathStep, ForwardSearchOutcome, ForwardSearchReport, ForwardSpinGroup},
    search::{
        expand_search_node, validate_query, CanonicalLockOutcomeKey, ExpandedAction,
        ForwardQueueSession, ForwardSearchConfig, ForwardSearchError, StateKey,
    },
};

const INIT_MAGIC: u32 = u32::from_le_bytes(*b"FWIN");
const TASK_MAGIC: u32 = u32::from_le_bytes(*b"FWTK");
const RESULT_MAGIC: u32 = u32::from_le_bytes(*b"FWRS");
const WIRE_VERSION: u32 = 10;
const MAX_WIRE_ITEMS: usize = 10_000_000;
const MAX_FIXED_TASKS_PER_BATCH: usize = 32;
const MAX_REORDER_BATCHES_PER_WORKER: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForwardParallelProduce {
    Pending,
    Batch,
    Completed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ForwardParallelProgress {
    pub visited_states: u64,
    pub generated_locks: u64,
    pub tasks_dispatched: u64,
    pub tasks_completed: u64,
    pub outstanding_tasks: usize,
    pub layer_index: usize,
    pub layer_count: usize,
    pub layer_done: usize,
    pub layer_total: usize,
    pub patterns_completed: usize,
    pub pattern_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForwardParallelError {
    Search(ForwardSearchError),
    InvalidWire(&'static str),
    InvalidState(&'static str),
}

impl ForwardParallelError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Search(ForwardSearchError::EmptyQueue) => "forward_search_empty_queue",
            Self::Search(ForwardSearchError::QueueTooLong) => "forward_search_queue_too_long",
            Self::Search(ForwardSearchError::InvalidHeight) => "forward_search_invalid_height",
            Self::Search(ForwardSearchError::BoardOutsideField) => {
                "forward_search_board_outside_field"
            }
            Self::Search(ForwardSearchError::PatternRequiresSpinFinder) => {
                "forward_search_pattern_requires_spin_finder"
            }
            Self::Search(ForwardSearchError::RenRequiresFixedQueue) => {
                "forward_ren_requires_fixed_queue"
            }
            Self::Search(ForwardSearchError::RenQueueTooLong) => "forward_ren_queue_too_long",
            Self::Search(ForwardSearchError::RenInitialComboUnsupported) => {
                "forward_ren_initial_combo_unsupported"
            }
            Self::Search(ForwardSearchError::RenInitialBackToBackUnsupported) => {
                "forward_ren_initial_back_to_back_unsupported"
            }
            Self::Search(ForwardSearchError::RenLineClearPolicyUnsupported) => {
                "forward_ren_line_clear_policy_unsupported"
            }
            Self::Search(ForwardSearchError::RenSpinProfileMustBeDisabled) => {
                "forward_ren_spin_profile_must_be_disabled"
            }
            Self::Search(ForwardSearchError::SpinProfileDisabled) => {
                "forward_search_spin_profile_disabled"
            }
            Self::Search(ForwardSearchError::UnsupportedRuleProfile(reason)) => reason,
            Self::Search(ForwardSearchError::Cancelled) => "forward_search_cancelled",
            Self::InvalidWire(reason) | Self::InvalidState(reason) => reason,
        }
    }
}

impl From<ForwardSearchError> for ForwardParallelError {
    fn from(value: ForwardSearchError) -> Self {
        Self::Search(value)
    }
}

pub struct ForwardParallelCoordinator {
    config: ForwardSearchConfig,
    state: CoordinatorState,
    tail_reachability: ReachabilityWorkspace,
    next_task_id: u64,
    next_absorb_task_id: u64,
    buffered_results: BTreeMap<u64, WireResult>,
    max_outstanding_tasks: usize,
    progress: ForwardParallelProgress,
}

enum CoordinatorState {
    Fixed(FixedCoordinator),
    Pattern(PatternCoordinator),
}

struct FixedCoordinator {
    session: ForwardQueueSession,
    pending: HashMap<u64, usize>,
}

struct PatternCoordinator {
    query: ForwardSearchQuery,
    layered: bool,
    next_pattern: usize,
    active_pattern: Option<usize>,
    active_session: Option<ForwardQueueSession>,
    pending: HashMap<u64, usize>,
    reports: BTreeMap<usize, ForwardSearchReport>,
}

enum WireTask {
    Expand {
        id: u64,
        key: StateKey,
        damage: TetrioDamageState,
    },
    Pattern {
        id: u64,
        pattern_index: usize,
        queue: Vec<PieceKind>,
    },
    ExpandPattern {
        id: u64,
        pattern_index: usize,
        key: StateKey,
        damage: TetrioDamageState,
    },
}

enum WireResult {
    Expansion {
        id: u64,
        generated_locks: u64,
        actions: Vec<ExpandedAction>,
    },
    Pattern {
        id: u64,
        report: ForwardSearchReport,
    },
}

impl WireResult {
    const fn id(&self) -> u64 {
        match self {
            Self::Expansion { id, .. } | Self::Pattern { id, .. } => *id,
        }
    }
}

pub struct ForwardParallelWorker {
    config: ForwardSearchConfig,
    fixed_queue: Option<Vec<PieceKind>>,
    reachability: ReachabilityWorkspace,
    pattern_queues: Vec<Vec<PieceKind>>,
    actions: Vec<ExpandedAction>,
    seen_lock_outcomes: HashSet<CanonicalLockOutcomeKey>,
    visited_states: u64,
    generated_locks: u64,
}

impl ForwardParallelCoordinator {
    pub fn is_worthwhile(query: &ForwardSearchQuery, workers: usize) -> bool {
        if workers < 2 {
            return false;
        }
        if query.piece_source().is_pattern() {
            query
                .piece_source()
                .pattern_count()
                .saturating_mul(query.piece_source().sequence_len())
                >= 8
        } else {
            query.piece_source().sequence_len() >= 4
        }
    }

    pub fn new(
        query: ForwardSearchQuery,
        worker_count: usize,
    ) -> Result<Self, ForwardParallelError> {
        validate_query(&query)?;
        let config = ForwardSearchConfig::from_query(&query);
        let tail_reachability = ReachabilityWorkspace::new(config.height, config.rule_profile)
            .map_err(|reason| {
                ForwardParallelError::Search(ForwardSearchError::UnsupportedRuleProfile(reason))
            })?;
        let state = if query.piece_source().is_pattern() {
            let layered = should_layer_pattern_search(
                query.piece_source().sequence_len(),
                query.piece_source().pattern_count(),
                worker_count,
            );
            let active_session = layered
                .then(|| ForwardQueueSession::new(config, query.piece_source().sequence_at(0), 0));
            CoordinatorState::Pattern(PatternCoordinator {
                query,
                layered,
                next_pattern: usize::from(layered),
                active_pattern: layered.then_some(0),
                active_session,
                pending: HashMap::new(),
                reports: BTreeMap::new(),
            })
        } else {
            let queue = query.piece_source().sequence_at(0);
            CoordinatorState::Fixed(FixedCoordinator {
                session: ForwardQueueSession::new(config, queue, 0),
                pending: HashMap::new(),
            })
        };
        let parallel_workers = worker_count.saturating_sub(1).max(1);
        let tasks_per_batch = match &state {
            CoordinatorState::Pattern(pattern) if !pattern.layered => 1,
            CoordinatorState::Fixed(_) | CoordinatorState::Pattern(_) => MAX_FIXED_TASKS_PER_BATCH,
        };
        let max_outstanding_tasks = parallel_workers
            .saturating_mul(tasks_per_batch)
            .saturating_mul(MAX_REORDER_BATCHES_PER_WORKER);
        Ok(Self {
            config,
            state,
            tail_reachability,
            next_task_id: 1,
            next_absorb_task_id: 1,
            buffered_results: BTreeMap::new(),
            max_outstanding_tasks,
            progress: ForwardParallelProgress::default(),
        })
    }

    pub fn worker_initialization(&self) -> Vec<u8> {
        let (fixed_queue, pattern_queues) = match &self.state {
            CoordinatorState::Fixed(fixed) => (Some(fixed.session.queue.as_slice()), Vec::new()),
            CoordinatorState::Pattern(pattern) if pattern.layered => (
                None,
                (0..pattern.query.piece_source().pattern_count())
                    .map(|index| pattern.query.piece_source().sequence_at(index))
                    .collect(),
            ),
            CoordinatorState::Pattern(_) => (None, Vec::new()),
        };
        encode_worker_initialization(self.config, fixed_queue, &pattern_queues)
    }

    pub fn progress(&self) -> ForwardParallelProgress {
        let mut progress = self.progress;
        match &self.state {
            CoordinatorState::Fixed(fixed) => {
                apply_session_progress(
                    &mut progress,
                    &fixed.session,
                    fixed.pending.len(),
                    usize::from(fixed.session.completed),
                    1,
                );
            }
            CoordinatorState::Pattern(pattern) if pattern.layered => {
                let pattern_count = pattern.query.piece_source().pattern_count();
                progress.patterns_completed = pattern.reports.len();
                progress.pattern_count = pattern_count;
                if let Some(session) = pattern.active_session.as_ref() {
                    apply_session_progress(
                        &mut progress,
                        session,
                        pattern.pending.len(),
                        pattern.reports.len(),
                        pattern_count,
                    );
                }
            }
            CoordinatorState::Pattern(pattern) => {
                progress.patterns_completed = pattern.reports.len();
                progress.pattern_count = pattern.query.piece_source().pattern_count();
                progress.layer_count = pattern.query.piece_source().sequence_len();
            }
        }
        progress
    }

    pub fn produce(
        &mut self,
        capacity: usize,
        control: &ExecutionControl,
    ) -> Result<(ForwardParallelProduce, Vec<u8>), ForwardParallelError> {
        if control.is_cancelled() {
            return Ok((ForwardParallelProduce::Cancelled, Vec::new()));
        }
        let available_capacity = self
            .max_outstanding_tasks
            .saturating_sub(self.outstanding_tasks());
        if available_capacity == 0 {
            return Ok((ForwardParallelProduce::Pending, Vec::new()));
        }
        let capacity = match &self.state {
            CoordinatorState::Fixed(_) => capacity.clamp(1, MAX_FIXED_TASKS_PER_BATCH),
            CoordinatorState::Pattern(pattern) if pattern.layered => {
                capacity.clamp(1, MAX_FIXED_TASKS_PER_BATCH)
            }
            CoordinatorState::Pattern(_) => 1,
        }
        .min(available_capacity);
        let mut tasks = Vec::with_capacity(capacity);
        match &mut self.state {
            CoordinatorState::Fixed(fixed) => {
                seal_fixed_layer(fixed);
                if fixed.session.completed {
                    return Ok((ForwardParallelProduce::Completed, Vec::new()));
                }
                while tasks.len() < capacity
                    && fixed.session.current_cursor < fixed.session.current.len()
                {
                    let index = fixed.session.current_cursor;
                    fixed.session.current_cursor += 1;
                    let (stored_key, damage) = {
                        let node = &fixed.session.current[index];
                        (node.key, node.damage_state())
                    };
                    let key = fixed.session.materialize_state_key(stored_key);
                    let id = self.next_task_id;
                    self.next_task_id = self.next_task_id.saturating_add(1);
                    fixed.pending.insert(id, index);
                    tasks.push(WireTask::Expand { id, key, damage });
                }
            }
            CoordinatorState::Pattern(pattern) => {
                if pattern.layered {
                    match seal_layered_pattern(pattern, self.config, control) {
                        Ok(()) => {}
                        Err(ForwardSearchError::Cancelled) => {
                            return Ok((ForwardParallelProduce::Cancelled, Vec::new()));
                        }
                        Err(error) => return Err(error.into()),
                    }
                    let Some(session) = pattern.active_session.as_mut() else {
                        return Ok((ForwardParallelProduce::Completed, Vec::new()));
                    };
                    let pattern_index =
                        pattern
                            .active_pattern
                            .ok_or(ForwardParallelError::InvalidState(
                                "forward_parallel_pattern_session_without_identity",
                            ))?;
                    while tasks.len() < capacity && session.current_cursor < session.current.len() {
                        let index = session.current_cursor;
                        session.current_cursor += 1;
                        let (stored_key, damage) = {
                            let node = &session.current[index];
                            (node.key, node.damage_state())
                        };
                        let key = session.materialize_state_key(stored_key);
                        let id = self.next_task_id;
                        self.next_task_id = self.next_task_id.saturating_add(1);
                        pattern.pending.insert(id, index);
                        tasks.push(WireTask::ExpandPattern {
                            id,
                            pattern_index,
                            key,
                            damage,
                        });
                    }
                } else {
                    let pattern_count = pattern.query.piece_source().pattern_count();
                    while tasks.len() < capacity && pattern.next_pattern < pattern_count {
                        let pattern_index = pattern.next_pattern;
                        pattern.next_pattern += 1;
                        let id = self.next_task_id;
                        self.next_task_id = self.next_task_id.saturating_add(1);
                        pattern.pending.insert(id, pattern_index);
                        tasks.push(WireTask::Pattern {
                            id,
                            pattern_index,
                            queue: pattern.query.piece_source().sequence_at(pattern_index),
                        });
                    }
                    if tasks.is_empty() && pattern.pending.is_empty() {
                        return Ok((ForwardParallelProduce::Completed, Vec::new()));
                    }
                }
            }
        }
        if tasks.is_empty() {
            return Ok((ForwardParallelProduce::Pending, Vec::new()));
        }
        self.progress.tasks_dispatched = self
            .progress
            .tasks_dispatched
            .saturating_add(tasks.len() as u64);
        self.progress.outstanding_tasks = self.outstanding_tasks();
        Ok((ForwardParallelProduce::Batch, encode_tasks(&tasks)))
    }

    pub fn absorb(
        &mut self,
        input: &[u8],
        control: &ExecutionControl,
    ) -> Result<usize, ForwardParallelError> {
        if control.is_cancelled() {
            return Err(ForwardSearchError::Cancelled.into());
        }
        let results = decode_results(input)?;
        let count = results.len();
        for result in results {
            let id = result.id();
            if id < self.next_absorb_task_id || !self.task_is_pending(id) {
                return Err(ForwardParallelError::InvalidState(
                    "forward_parallel_task_not_pending",
                ));
            }
            if self.buffered_results.insert(id, result).is_some() {
                return Err(ForwardParallelError::InvalidState(
                    "forward_parallel_task_result_duplicate",
                ));
            }
        }
        while self
            .buffered_results
            .contains_key(&self.next_absorb_task_id)
        {
            if control.is_cancelled() {
                return Err(ForwardSearchError::Cancelled.into());
            }
            let result = self
                .buffered_results
                .remove(&self.next_absorb_task_id)
                .expect("checked buffered forward result");
            self.absorb_ready_result(result, control)?;
            self.next_absorb_task_id = self.next_absorb_task_id.saturating_add(1);
            self.progress.tasks_completed = self.progress.tasks_completed.saturating_add(1);
        }
        self.progress.outstanding_tasks = self.outstanding_tasks();
        Ok(count)
    }

    fn task_is_pending(&self, id: u64) -> bool {
        match &self.state {
            CoordinatorState::Fixed(fixed) => fixed.pending.contains_key(&id),
            CoordinatorState::Pattern(pattern) => pattern.pending.contains_key(&id),
        }
    }

    fn absorb_ready_result(
        &mut self,
        result: WireResult,
        control: &ExecutionControl,
    ) -> Result<(), ForwardParallelError> {
        match (&mut self.state, result) {
            (
                CoordinatorState::Fixed(fixed),
                WireResult::Expansion {
                    id,
                    generated_locks,
                    actions,
                },
            ) => {
                let index = fixed
                    .pending
                    .remove(&id)
                    .ok_or(ForwardParallelError::InvalidState(
                        "forward_parallel_task_not_pending",
                    ))?;
                let parents = fixed.session.current[index].traces;
                fixed.session.visited_states = fixed.session.visited_states.saturating_add(1);
                fixed.session.generated_locks = fixed
                    .session
                    .generated_locks
                    .saturating_add(generated_locks);
                for action in actions {
                    fixed.session.absorb_expanded_action(
                        parents,
                        action,
                        control,
                        &mut self.tail_reachability,
                    )?;
                }
                self.progress.visited_states = fixed.session.visited_states;
                self.progress.generated_locks = fixed.session.generated_locks;
            }
            (
                CoordinatorState::Pattern(pattern),
                WireResult::Expansion {
                    id,
                    generated_locks,
                    actions,
                },
            ) if pattern.layered => {
                let index =
                    pattern
                        .pending
                        .remove(&id)
                        .ok_or(ForwardParallelError::InvalidState(
                            "forward_parallel_pattern_task_not_pending",
                        ))?;
                let session =
                    pattern
                        .active_session
                        .as_mut()
                        .ok_or(ForwardParallelError::InvalidState(
                            "forward_parallel_pattern_session_missing",
                        ))?;
                let parents = session.current[index].traces;
                session.visited_states = session.visited_states.saturating_add(1);
                session.generated_locks = session.generated_locks.saturating_add(generated_locks);
                for action in actions {
                    session.absorb_expanded_action(
                        parents,
                        action,
                        control,
                        &mut self.tail_reachability,
                    )?;
                }
                self.progress.visited_states = session.visited_states;
                self.progress.generated_locks = session.generated_locks;
            }
            (CoordinatorState::Pattern(pattern), WireResult::Pattern { id, report }) => {
                if pattern.layered {
                    return Err(ForwardParallelError::InvalidWire(
                        "forward_parallel_layered_pattern_result_kind_mismatch",
                    ));
                }
                let pattern_index =
                    pattern
                        .pending
                        .remove(&id)
                        .ok_or(ForwardParallelError::InvalidState(
                            "forward_parallel_pattern_not_pending",
                        ))?;
                if report
                    .outcomes()
                    .iter()
                    .any(|outcome| outcome.source_pattern_index() as usize != pattern_index)
                {
                    return Err(ForwardParallelError::InvalidWire(
                        "forward_parallel_pattern_identity_mismatch",
                    ));
                }
                self.progress.visited_states = self
                    .progress
                    .visited_states
                    .saturating_add(report.visited_states());
                self.progress.generated_locks = self
                    .progress
                    .generated_locks
                    .saturating_add(report.generated_locks());
                if pattern.reports.insert(pattern_index, report).is_some() {
                    return Err(ForwardParallelError::InvalidState(
                        "forward_parallel_pattern_duplicate",
                    ));
                }
            }
            _ => {
                return Err(ForwardParallelError::InvalidWire(
                    "forward_parallel_result_kind_mismatch",
                ));
            }
        }
        Ok(())
    }

    pub fn finish(self, workers_used: usize) -> Result<ForwardSearchReport, ForwardParallelError> {
        self.finish_with_control(workers_used, &ExecutionControl::default())
    }

    pub fn finish_with_control(
        mut self,
        workers_used: usize,
        control: &ExecutionControl,
    ) -> Result<ForwardSearchReport, ForwardParallelError> {
        if control.is_cancelled() {
            return Err(ForwardSearchError::Cancelled.into());
        }
        let workers_used = workers_used.max(1);
        if self.outstanding_tasks() != 0 {
            return Err(ForwardParallelError::InvalidState(
                "forward_parallel_finish_with_outstanding_tasks",
            ));
        }
        if !self.buffered_results.is_empty() {
            return Err(ForwardParallelError::InvalidState(
                "forward_parallel_finish_with_buffered_results",
            ));
        }
        match &mut self.state {
            CoordinatorState::Fixed(fixed) => {
                seal_fixed_layer(fixed);
                if !fixed.session.completed {
                    return Err(ForwardParallelError::InvalidState(
                        "forward_parallel_fixed_search_incomplete",
                    ));
                }
                let mut report = fixed.session.build_report(control)?;
                canonicalize_outcomes(report.outcomes_mut(), control)?;
                Ok(report.with_workers_used(workers_used))
            }
            CoordinatorState::Pattern(pattern) => {
                if pattern.layered {
                    seal_layered_pattern(pattern, self.config, control)?;
                    if pattern.active_session.is_some() {
                        return Err(ForwardParallelError::InvalidState(
                            "forward_parallel_layered_pattern_search_incomplete",
                        ));
                    }
                }
                if pattern.next_pattern != pattern.query.piece_source().pattern_count() {
                    return Err(ForwardParallelError::InvalidState(
                        "forward_parallel_pattern_search_incomplete",
                    ));
                }
                let mut outcomes = Vec::new();
                let mut visited_states = 0_u64;
                let mut generated_locks = 0_u64;
                let mut peak_frontier = 0_usize;
                for report in pattern.reports.values() {
                    if control.is_cancelled() {
                        return Err(ForwardSearchError::Cancelled.into());
                    }
                    visited_states = visited_states.saturating_add(report.visited_states());
                    generated_locks = generated_locks.saturating_add(report.generated_locks());
                    peak_frontier = peak_frontier.max(report.peak_frontier());
                    for outcome in report.outcomes() {
                        if control.is_cancelled() {
                            return Err(ForwardSearchError::Cancelled.into());
                        }
                        outcomes.push(outcome.clone());
                    }
                }
                canonicalize_outcomes(&mut outcomes, control)?;
                Ok(ForwardSearchReport::new(
                    true,
                    self.config.board.words(),
                    workers_used,
                    visited_states,
                    generated_locks,
                    peak_frontier,
                    outcomes,
                ))
            }
        }
    }

    fn outstanding_tasks(&self) -> usize {
        match &self.state {
            CoordinatorState::Fixed(fixed) => fixed.pending.len(),
            CoordinatorState::Pattern(pattern) => pattern.pending.len(),
        }
    }
}

fn should_layer_pattern_search(sequence_len: usize, pattern_count: usize, workers: usize) -> bool {
    sequence_len >= 6 && pattern_count > 0 && pattern_count.saturating_mul(4) <= workers.max(1)
}

impl ForwardParallelWorker {
    pub fn new(input: &[u8]) -> Result<Self, ForwardParallelError> {
        let (config, fixed_queue, pattern_queues) = decode_worker_initialization(input)?;
        let reachability =
            ReachabilityWorkspace::new(config.height, config.rule_profile).map_err(|reason| {
                ForwardParallelError::Search(ForwardSearchError::UnsupportedRuleProfile(reason))
            })?;
        Ok(Self {
            config,
            fixed_queue,
            reachability,
            pattern_queues,
            actions: Vec::new(),
            seen_lock_outcomes: HashSet::new(),
            visited_states: 0,
            generated_locks: 0,
        })
    }

    pub fn consume(
        &mut self,
        input: &[u8],
        control: &ExecutionControl,
    ) -> Result<(usize, Vec<u8>), ForwardParallelError> {
        let tasks = decode_tasks(input)?;
        let count = tasks.len();
        let mut results = Vec::with_capacity(count);
        for task in tasks {
            if control.is_cancelled() {
                return Err(ForwardParallelError::Search(ForwardSearchError::Cancelled));
            }
            match task {
                WireTask::Expand { id, key, damage } => {
                    let queue =
                        self.fixed_queue
                            .as_deref()
                            .ok_or(ForwardParallelError::InvalidState(
                                "forward_parallel_fixed_queue_not_initialized",
                            ))?;
                    let generated_locks = expand_search_node(
                        self.config,
                        queue,
                        key,
                        damage,
                        control,
                        &mut self.reachability,
                        &mut self.actions,
                        &mut self.seen_lock_outcomes,
                    )?;
                    self.visited_states = self.visited_states.saturating_add(1);
                    self.generated_locks = self.generated_locks.saturating_add(generated_locks);
                    results.push(WireResult::Expansion {
                        id,
                        generated_locks,
                        actions: std::mem::take(&mut self.actions),
                    });
                }
                WireTask::Pattern {
                    id,
                    pattern_index,
                    queue,
                } => {
                    let mut session = ForwardQueueSession::new(self.config, queue, pattern_index);
                    let report = loop {
                        match session.advance(256, control, &mut self.reachability)? {
                            crate::search::ForwardSearchAdvance::Pending => {}
                            crate::search::ForwardSearchAdvance::Completed(report) => break report,
                            crate::search::ForwardSearchAdvance::Cancelled => {
                                return Err(ForwardParallelError::Search(
                                    ForwardSearchError::Cancelled,
                                ));
                            }
                        }
                    };
                    self.visited_states =
                        self.visited_states.saturating_add(report.visited_states());
                    self.generated_locks = self
                        .generated_locks
                        .saturating_add(report.generated_locks());
                    results.push(WireResult::Pattern { id, report });
                }
                WireTask::ExpandPattern {
                    id,
                    pattern_index,
                    key,
                    damage,
                } => {
                    let queue = self.pattern_queues.get(pattern_index).ok_or(
                        ForwardParallelError::InvalidState(
                            "forward_parallel_pattern_queue_not_initialized",
                        ),
                    )?;
                    let generated_locks = expand_search_node(
                        self.config,
                        queue,
                        key,
                        damage,
                        control,
                        &mut self.reachability,
                        &mut self.actions,
                        &mut self.seen_lock_outcomes,
                    )?;
                    self.visited_states = self.visited_states.saturating_add(1);
                    self.generated_locks = self.generated_locks.saturating_add(generated_locks);
                    results.push(WireResult::Expansion {
                        id,
                        generated_locks,
                        actions: std::mem::take(&mut self.actions),
                    });
                }
            }
        }
        let output = encode_results(&results);
        for result in results {
            if let WireResult::Expansion { mut actions, .. } = result {
                actions.clear();
                if actions.capacity() > self.actions.capacity() {
                    self.actions = actions;
                }
            }
        }
        Ok((count, output))
    }

    pub fn progress(&self) -> ForwardParallelProgress {
        ForwardParallelProgress {
            visited_states: self.visited_states,
            generated_locks: self.generated_locks,
            tasks_dispatched: 0,
            tasks_completed: self.visited_states,
            outstanding_tasks: 0,
            layer_index: 0,
            layer_count: self.fixed_queue.as_ref().map_or(0, Vec::len),
            layer_done: 0,
            layer_total: 0,
            patterns_completed: 0,
            pattern_count: self.pattern_queues.len(),
        }
    }
}

fn apply_session_progress(
    progress: &mut ForwardParallelProgress,
    session: &ForwardQueueSession,
    pending: usize,
    patterns_completed: usize,
    pattern_count: usize,
) {
    progress.layer_index = session.layer_index;
    progress.layer_count = session.queue.len();
    progress.layer_total = session.current.len();
    progress.layer_done = session
        .current_cursor
        .saturating_sub(pending)
        .min(progress.layer_total);
    progress.patterns_completed = patterns_completed;
    progress.pattern_count = pattern_count;
}

fn seal_fixed_layer(fixed: &mut FixedCoordinator) {
    while fixed.session.current_cursor >= fixed.session.current.len() && fixed.pending.is_empty() {
        if fixed.session.next.is_empty() {
            fixed.session.completed = true;
            return;
        }
        std::mem::swap(&mut fixed.session.current, &mut fixed.session.next);
        fixed.session.next.clear();
        fixed.session.current_cursor = 0;
        fixed.session.layer_index = fixed.session.layer_index.saturating_add(1);
        fixed.session.next_index.clear();
        fixed.session.peak_frontier = fixed.session.peak_frontier.max(fixed.session.current.len());
    }
}

fn seal_layered_pattern(
    pattern: &mut PatternCoordinator,
    config: ForwardSearchConfig,
    control: &ExecutionControl,
) -> Result<(), ForwardSearchError> {
    loop {
        if control.is_cancelled() {
            return Err(ForwardSearchError::Cancelled);
        }
        if pattern.active_session.is_none() {
            let pattern_count = pattern.query.piece_source().pattern_count();
            if pattern.next_pattern >= pattern_count {
                return Ok(());
            }
            let pattern_index = pattern.next_pattern;
            pattern.next_pattern += 1;
            pattern.active_pattern = Some(pattern_index);
            pattern.active_session = Some(ForwardQueueSession::new(
                config,
                pattern.query.piece_source().sequence_at(pattern_index),
                pattern_index,
            ));
        }

        let session = pattern
            .active_session
            .as_mut()
            .expect("layered pattern session exists");
        while session.current_cursor >= session.current.len() && pattern.pending.is_empty() {
            if session.next.is_empty() {
                session.completed = true;
                break;
            }
            std::mem::swap(&mut session.current, &mut session.next);
            session.next.clear();
            session.current_cursor = 0;
            session.layer_index = session.layer_index.saturating_add(1);
            session.next_index.clear();
            session.peak_frontier = session.peak_frontier.max(session.current.len());
        }
        if !session.completed || !pattern.pending.is_empty() {
            return Ok(());
        }

        let pattern_index = pattern
            .active_pattern
            .take()
            .expect("layered pattern identity exists");
        let report = pattern
            .active_session
            .take()
            .expect("completed layered pattern session")
            .build_report(control)?;
        pattern.reports.insert(pattern_index, report);
    }
}

fn canonicalize_outcomes(
    outcomes: &mut Vec<ForwardSearchOutcome>,
    control: &ExecutionControl,
) -> Result<(), ForwardSearchError> {
    if control.is_cancelled() {
        return Err(ForwardSearchError::Cancelled);
    }
    outcomes.sort_by(|left, right| {
        (
            left.source_pattern_index(),
            left.source_queue(),
            left.group(),
            left.path().len(),
            left.final_board(),
            left.spin_piece(),
            left.spin_mini(),
            left.spin_lines(),
            left.total_damage(),
            left.evidence_path_count(),
            left.evidence_complete(),
            left.path(),
        )
            .cmp(&(
                right.source_pattern_index(),
                right.source_queue(),
                right.group(),
                right.path().len(),
                right.final_board(),
                right.spin_piece(),
                right.spin_mini(),
                right.spin_lines(),
                right.total_damage(),
                right.evidence_path_count(),
                right.evidence_complete(),
                right.path(),
            ))
    });
    if control.is_cancelled() {
        return Err(ForwardSearchError::Cancelled);
    }
    outcomes.dedup_by(|left, right| {
        left.source_pattern_index() == right.source_pattern_index()
            && left.source_queue() == right.source_queue()
            && left.group() == right.group()
            && left.final_board() == right.final_board()
            && left.spin_piece() == right.spin_piece()
            && left.spin_mini() == right.spin_mini()
            && left.spin_lines() == right.spin_lines()
            && left.total_damage() == right.total_damage()
            && left.evidence_path_count() == right.evidence_path_count()
            && left.evidence_complete() == right.evidence_complete()
            && left.path() == right.path()
    });
    if control.is_cancelled() {
        return Err(ForwardSearchError::Cancelled);
    }
    for (index, outcome) in outcomes.iter_mut().enumerate() {
        if control.is_cancelled() {
            return Err(ForwardSearchError::Cancelled);
        }
        outcome.assign_id(index as u64 + 1);
    }
    Ok(())
}

fn encode_worker_initialization(
    config: ForwardSearchConfig,
    fixed_queue: Option<&[PieceKind]>,
    pattern_queues: &[Vec<PieceKind>],
) -> Vec<u8> {
    let mut output = Vec::new();
    put_header(&mut output, INIT_MAGIC);
    encode_config(&mut output, config);
    encode_piece_slice(&mut output, fixed_queue.unwrap_or_default());
    put_u32(&mut output, pattern_queues.len() as u32);
    for queue in pattern_queues {
        encode_piece_slice(&mut output, queue);
    }
    output
}

fn decode_worker_initialization(
    input: &[u8],
) -> Result<
    (
        ForwardSearchConfig,
        Option<Vec<PieceKind>>,
        Vec<Vec<PieceKind>>,
    ),
    ForwardParallelError,
> {
    let mut reader = Reader::new(input);
    reader.require_header(INIT_MAGIC)?;
    let config = decode_config(&mut reader)?;
    let queue = decode_piece_vec(&mut reader)?;
    let pattern_count = reader.count()?;
    let mut pattern_queues = Vec::with_capacity(pattern_count);
    for _ in 0..pattern_count {
        pattern_queues.push(decode_piece_vec(&mut reader)?);
    }
    reader.finish()?;
    Ok((config, (!queue.is_empty()).then_some(queue), pattern_queues))
}

fn encode_tasks(tasks: &[WireTask]) -> Vec<u8> {
    let mut output = Vec::new();
    put_header(&mut output, TASK_MAGIC);
    put_u32(&mut output, tasks.len() as u32);
    for task in tasks {
        match task {
            WireTask::Expand { id, key, damage } => {
                output.push(0);
                put_u64(&mut output, *id);
                encode_state(&mut output, *key, *damage);
            }
            WireTask::Pattern {
                id,
                pattern_index,
                queue,
            } => {
                output.push(1);
                put_u64(&mut output, *id);
                put_u32(&mut output, *pattern_index as u32);
                encode_piece_slice(&mut output, queue);
            }
            WireTask::ExpandPattern {
                id,
                pattern_index,
                key,
                damage,
            } => {
                output.push(2);
                put_u64(&mut output, *id);
                put_u32(&mut output, *pattern_index as u32);
                encode_state(&mut output, *key, *damage);
            }
        }
    }
    output
}

fn decode_tasks(input: &[u8]) -> Result<Vec<WireTask>, ForwardParallelError> {
    let mut reader = Reader::new(input);
    reader.require_header(TASK_MAGIC)?;
    let count = reader.count()?;
    let mut tasks = Vec::with_capacity(count);
    for _ in 0..count {
        let tag = reader.u8()?;
        let id = reader.u64()?;
        tasks.push(match tag {
            0 => {
                let (key, damage) = decode_state(&mut reader)?;
                WireTask::Expand { id, key, damage }
            }
            1 => WireTask::Pattern {
                id,
                pattern_index: reader.count()?,
                queue: decode_piece_vec(&mut reader)?,
            },
            2 => {
                let pattern_index = reader.count()?;
                let (key, damage) = decode_state(&mut reader)?;
                WireTask::ExpandPattern {
                    id,
                    pattern_index,
                    key,
                    damage,
                }
            }
            _ => {
                return Err(ForwardParallelError::InvalidWire(
                    "forward_task_tag_invalid",
                ))
            }
        });
    }
    reader.finish()?;
    Ok(tasks)
}

fn encode_results(results: &[WireResult]) -> Vec<u8> {
    let mut output = Vec::new();
    put_header(&mut output, RESULT_MAGIC);
    put_u32(&mut output, results.len() as u32);
    for result in results {
        match result {
            WireResult::Expansion {
                id,
                generated_locks,
                actions,
            } => {
                output.push(0);
                put_u64(&mut output, *id);
                put_u64(&mut output, *generated_locks);
                put_u32(&mut output, actions.len() as u32);
                for action in actions {
                    encode_action(&mut output, action);
                }
            }
            WireResult::Pattern { id, report } => {
                output.push(1);
                put_u64(&mut output, *id);
                encode_report(&mut output, report);
            }
        }
    }
    output
}

fn decode_results(input: &[u8]) -> Result<Vec<WireResult>, ForwardParallelError> {
    let mut reader = Reader::new(input);
    reader.require_header(RESULT_MAGIC)?;
    let count = reader.count()?;
    let mut results = Vec::with_capacity(count);
    for _ in 0..count {
        let tag = reader.u8()?;
        let id = reader.u64()?;
        results.push(match tag {
            0 => {
                let generated_locks = reader.u64()?;
                let action_count = reader.count()?;
                let mut actions = Vec::with_capacity(action_count);
                for _ in 0..action_count {
                    actions.push(decode_action(&mut reader)?);
                }
                WireResult::Expansion {
                    id,
                    generated_locks,
                    actions,
                }
            }
            1 => WireResult::Pattern {
                id,
                report: decode_report(&mut reader)?,
            },
            _ => {
                return Err(ForwardParallelError::InvalidWire(
                    "forward_result_tag_invalid",
                ))
            }
        });
    }
    reader.finish()?;
    Ok(results)
}

fn encode_config(output: &mut Vec<u8>, config: ForwardSearchConfig) {
    encode_board(output, config.board);
    output.push(config.height);
    output.push(u8::from(config.hold_enabled));
    output.push(rule_code(config.rule_profile));
    output.push(spin_profile_code(config.spin_profile));
    output.push(match config.line_clear_policy {
        ForwardLineClearPolicy::Any => 0,
        ForwardLineClearPolicy::PreserveBackToBack => 1,
    });
    encode_option_u16(output, config.initial_combo);
    encode_option_u16(output, config.initial_back_to_back);
    match config.mode {
        ForwardSearchMode::MaximumDamage => output.push(0),
        ForwardSearchMode::DamageAtLeast(minimum) => {
            output.push(2);
            put_u32(output, minimum);
        }
        ForwardSearchMode::SpinFinder(target) => {
            output.push(1);
            output.push(match target.line_requirement() {
                ForwardSpinLineRequirement::Any => u8::MAX,
                ForwardSpinLineRequirement::Exact(lines) => lines,
                ForwardSpinLineRequirement::AtLeast(lines) => 0x80 | lines,
            });
            output.push(match target.category() {
                ForwardSpinCategory::Any => 0,
                ForwardSpinCategory::T => 1,
                ForwardSpinCategory::Other => 2,
            });
        }
        ForwardSearchMode::MaximumRen => output.push(3),
    }
}

fn decode_config(reader: &mut Reader<'_>) -> Result<ForwardSearchConfig, ForwardParallelError> {
    let board = decode_board(reader)?;
    let height = reader.u8()?;
    let hold_enabled = reader.bool()?;
    let rule_profile = rule_from_code(reader.u8()?)?;
    let spin_profile = spin_profile_from_code(reader.u8()?)?;
    let line_clear_policy = match reader.u8()? {
        0 => ForwardLineClearPolicy::Any,
        1 => ForwardLineClearPolicy::PreserveBackToBack,
        _ => {
            return Err(ForwardParallelError::InvalidWire(
                "forward_line_clear_policy_invalid",
            ))
        }
    };
    let initial_combo = reader.option_u16()?;
    let initial_back_to_back = reader.option_u16()?;
    let mode = match reader.u8()? {
        0 => ForwardSearchMode::MaximumDamage,
        2 => ForwardSearchMode::DamageAtLeast(reader.u32()?),
        1 => {
            let lines = match reader.u8()? {
                u8::MAX => ForwardSpinLineRequirement::Any,
                value @ 0..=4 => ForwardSpinLineRequirement::Exact(value),
                value @ 0x80..=0x84 => ForwardSpinLineRequirement::AtLeast(value & 0x7f),
                _ => {
                    return Err(ForwardParallelError::InvalidWire(
                        "forward_spin_lines_invalid",
                    ))
                }
            };
            let category = match reader.u8()? {
                0 => ForwardSpinCategory::Any,
                1 => ForwardSpinCategory::T,
                2 => ForwardSpinCategory::Other,
                _ => {
                    return Err(ForwardParallelError::InvalidWire(
                        "forward_spin_category_invalid",
                    ))
                }
            };
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::with_line_requirement(lines, category))
        }
        3 => ForwardSearchMode::MaximumRen,
        _ => return Err(ForwardParallelError::InvalidWire("forward_mode_invalid")),
    };
    Ok(ForwardSearchConfig {
        board,
        height,
        hold_enabled,
        rule_profile,
        spin_profile,
        initial_combo,
        initial_back_to_back,
        line_clear_policy,
        mode,
    })
}

fn encode_state(output: &mut Vec<u8>, key: StateKey, damage: TetrioDamageState) {
    encode_board(output, key.board);
    output.push(piece_code(key.active));
    put_u16(output, key.cursor);
    output.push(key.hold.map_or(u8::MAX, piece_code));
    encode_option_u16(output, damage.combo());
    encode_option_u16(output, damage.back_to_back());
    put_u32(output, damage.total_damage());
}

fn decode_state(
    reader: &mut Reader<'_>,
) -> Result<(StateKey, TetrioDamageState), ForwardParallelError> {
    let board = decode_board(reader)?;
    let active = piece_from_code(reader.u8()?)?;
    let cursor = reader.u16()?;
    let hold = match reader.u8()? {
        u8::MAX => None,
        code => Some(piece_from_code(code)?),
    };
    let combo = reader.option_u16()?;
    let back_to_back = reader.option_u16()?;
    let total_damage = reader.u32()?;
    let damage = TetrioDamageState::from_parts(combo, back_to_back, total_damage);
    Ok((
        StateKey {
            board,
            active,
            cursor,
            hold,
            combo,
            back_to_back,
        },
        damage,
    ))
}

fn encode_action(output: &mut Vec<u8>, action: &ExpandedAction) {
    match action {
        ExpandedAction::Child {
            key,
            damage_state,
            step,
        } => {
            output.push(0);
            encode_state(output, *key, *damage_state);
            encode_action_step(output, step);
        }
        ExpandedAction::DamageTerminal {
            board,
            total_damage,
            step,
        } => {
            output.push(1);
            encode_board(output, *board);
            put_u32(output, *total_damage);
            encode_action_step(output, step);
        }
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
            output.push(2);
            encode_board(output, *board);
            output.push(rotation.quarter_turns());
            output.push(*x as u8);
            output.push(*y as u8);
            output.push(piece_code(*piece));
            output.push(u8::from(*mini));
            output.push(*lines);
            put_u32(output, *total_damage);
            encode_action_step(output, step);
        }
        ExpandedAction::RenTerminal {
            board,
            ren_count,
            step,
        } => {
            output.push(3);
            encode_board(output, *board);
            output.push(*ren_count);
            encode_action_step(output, step);
        }
    }
}

fn decode_action(reader: &mut Reader<'_>) -> Result<ExpandedAction, ForwardParallelError> {
    match reader.u8()? {
        0 => {
            let (key, damage_state) = decode_state(reader)?;
            Ok(ExpandedAction::Child {
                key,
                damage_state,
                step: decode_action_step(reader)?,
            })
        }
        1 => Ok(ExpandedAction::DamageTerminal {
            board: decode_board(reader)?,
            total_damage: reader.u32()?,
            step: decode_action_step(reader)?,
        }),
        2 => Ok(ExpandedAction::Spin {
            board: decode_board(reader)?,
            rotation: RotationState::from_quarter_turns(reader.u8()?)
                .map_err(|_| ForwardParallelError::InvalidWire("forward_rotation_invalid"))?,
            x: reader.u8()? as i8,
            y: reader.u8()? as i8,
            piece: piece_from_code(reader.u8()?)?,
            mini: reader.bool()?,
            lines: reader.u8()?,
            total_damage: reader.u32()?,
            step: decode_action_step(reader)?,
        }),
        3 => Ok(ExpandedAction::RenTerminal {
            board: decode_board(reader)?,
            ren_count: reader.u8()?,
            step: decode_action_step(reader)?,
        }),
        _ => Err(ForwardParallelError::InvalidWire(
            "forward_action_tag_invalid",
        )),
    }
}

fn encode_report(output: &mut Vec<u8>, report: &ForwardSearchReport) {
    for word in report.initial_board() {
        put_u64(output, word);
    }
    put_u64(output, report.visited_states());
    put_u64(output, report.generated_locks());
    put_u32(output, report.peak_frontier() as u32);
    put_u32(output, report.outcomes().len() as u32);
    for outcome in report.outcomes() {
        put_u32(output, outcome.source_pattern_index());
        encode_piece_slice(output, outcome.source_queue());
        output.push(match outcome.group() {
            None => 0,
            Some(ForwardSpinGroup::T) => 1,
            Some(ForwardSpinGroup::Other) => 2,
            Some(ForwardSpinGroup::Integrated) => 3,
        });
        for word in outcome.final_board() {
            put_u64(output, word);
        }
        output.push(outcome.spin_piece().map_or(u8::MAX, piece_code));
        output.push(u8::from(outcome.spin_mini()));
        output.push(outcome.spin_lines());
        output.push(outcome.ren_count().unwrap_or(u8::MAX));
        put_u32(output, outcome.total_damage());
        put_decimal_string(output, outcome.evidence_path_count());
        output.push(u8::from(outcome.evidence_complete()));
        put_u32(output, outcome.path().len() as u32);
        for step in outcome.path() {
            encode_step(output, step);
        }
    }
}

fn decode_report(reader: &mut Reader<'_>) -> Result<ForwardSearchReport, ForwardParallelError> {
    let mut initial_board = [0_u64; 4];
    for word in &mut initial_board {
        *word = reader.u64()?;
    }
    let visited_states = reader.u64()?;
    let generated_locks = reader.u64()?;
    let peak_frontier = reader.count()?;
    let outcome_count = reader.count()?;
    let mut outcomes = Vec::with_capacity(outcome_count);
    for _ in 0..outcome_count {
        let source_pattern_index = reader.u32()?;
        let source_queue = decode_piece_vec(reader)?;
        let group = match reader.u8()? {
            0 => None,
            1 => Some(ForwardSpinGroup::T),
            2 => Some(ForwardSpinGroup::Other),
            3 => Some(ForwardSpinGroup::Integrated),
            _ => {
                return Err(ForwardParallelError::InvalidWire(
                    "forward_spin_group_invalid",
                ))
            }
        };
        let mut final_board = [0_u64; 4];
        for word in &mut final_board {
            *word = reader.u64()?;
        }
        let spin_piece = match reader.u8()? {
            u8::MAX => None,
            code => Some(piece_from_code(code)?),
        };
        let spin_mini = reader.bool()?;
        let spin_lines = reader.u8()?;
        let ren_count = match reader.u8()? {
            u8::MAX => None,
            value @ 0..=21 => Some(value),
            _ => {
                return Err(ForwardParallelError::InvalidWire(
                    "forward_ren_count_invalid",
                ))
            }
        };
        let total_damage = reader.u32()?;
        let evidence_path_count = reader.decimal_string()?;
        let evidence_complete = reader.bool()?;
        if !evidence_complete {
            return Err(ForwardParallelError::InvalidWire(
                "forward_evidence_incomplete",
            ));
        }
        let path_count = reader.count()?;
        let mut path = Vec::with_capacity(path_count);
        for _ in 0..path_count {
            path.push(decode_step(reader)?);
        }
        outcomes.push(
            ForwardSearchOutcome::new(
                0,
                source_pattern_index,
                source_queue,
                group,
                final_board,
                spin_piece,
                spin_mini,
                spin_lines,
                ren_count,
                total_damage,
                path,
            )
            .with_evidence_path_count(evidence_path_count),
        );
    }
    Ok(ForwardSearchReport::new(
        true,
        initial_board,
        1,
        visited_states,
        generated_locks,
        peak_frontier,
        outcomes,
    ))
}

fn encode_step(output: &mut Vec<u8>, step: &ForwardPathStep) {
    output.push(piece_code(step.piece()));
    output.push(step.rotation().quarter_turns());
    output.push(step.x() as u8);
    output.push(step.y() as u8);
    output.push(match step.hold_decision() {
        "none" => 0,
        "store" => 1,
        "swap" => 2,
        _ => u8::MAX,
    });
    output.push(step.cleared_lines());
    match step.spin() {
        None => output.push(0),
        Some((piece, mini)) => {
            output.push(1);
            output.push(piece_code(
                PieceKind::from_ascii(piece).expect("standard spin piece"),
            ));
            output.push(u8::from(mini));
        }
    }
    put_u32(output, step.damage());
    put_u32(output, step.total_damage());
    for word in step.placement_mask() {
        put_u64(output, word);
    }
    put_u32(output, step.cleared_row_mask());
    for word in step.board_after() {
        put_u64(output, word);
    }
}

fn encode_action_step(output: &mut Vec<u8>, step: &ForwardPathStep) {
    output.push(piece_code(step.piece()));
    output.push(step.placement_rotation().quarter_turns());
    output.push(step.x() as u8);
    output.push(step.y() as u8);
    output.push(match step.hold_decision() {
        "none" => 0,
        "store" => 1,
        "swap" => 2,
        _ => u8::MAX,
    });
    output.push(step.cleared_lines());
    match step.spin() {
        None => output.push(0),
        Some((piece, mini)) => {
            output.push(1);
            output.push(piece_code(
                PieceKind::from_ascii(piece).expect("standard spin piece"),
            ));
            output.push(u8::from(mini));
        }
    }
    put_u32(output, step.damage());
}

fn decode_step(reader: &mut Reader<'_>) -> Result<ForwardPathStep, ForwardParallelError> {
    let piece = piece_from_code(reader.u8()?)?;
    let rotation = RotationState::from_quarter_turns(reader.u8()?)
        .map_err(|_| ForwardParallelError::InvalidWire("forward_step_rotation_invalid"))?;
    let x = reader.u8()? as i8;
    let y = reader.u8()? as i8;
    let hold = match reader.u8()? {
        0 => "none",
        1 => "store",
        2 => "swap",
        _ => return Err(ForwardParallelError::InvalidWire("forward_hold_invalid")),
    };
    let cleared_lines = reader.u8()?;
    let spin = match reader.u8()? {
        0 => None,
        1 => Some((piece_from_code(reader.u8()?)?.as_ascii(), reader.bool()?)),
        _ => {
            return Err(ForwardParallelError::InvalidWire(
                "forward_spin_flag_invalid",
            ))
        }
    };
    let damage = reader.u32()?;
    let total_damage = reader.u32()?;
    let mut placement_mask = [0_u64; 4];
    for word in &mut placement_mask {
        *word = reader.u64()?;
    }
    let cleared_row_mask = reader.u32()?;
    let mut board_after = [0_u64; 4];
    for word in &mut board_after {
        *word = reader.u64()?;
    }
    Ok(ForwardPathStep::new(
        piece,
        rotation,
        rotation,
        x,
        y,
        hold,
        cleared_lines,
        spin,
        damage,
        total_damage,
        placement_mask,
        cleared_row_mask,
        board_after,
    ))
}

fn decode_action_step(reader: &mut Reader<'_>) -> Result<ForwardPathStep, ForwardParallelError> {
    let piece = piece_from_code(reader.u8()?)?;
    let rotation = RotationState::from_quarter_turns(reader.u8()?)
        .map_err(|_| ForwardParallelError::InvalidWire("forward_step_rotation_invalid"))?;
    let x = reader.u8()? as i8;
    let y = reader.u8()? as i8;
    let hold = match reader.u8()? {
        0 => "none",
        1 => "store",
        2 => "swap",
        _ => return Err(ForwardParallelError::InvalidWire("forward_hold_invalid")),
    };
    let cleared_lines = reader.u8()?;
    let spin = match reader.u8()? {
        0 => None,
        1 => Some((piece_from_code(reader.u8()?)?.as_ascii(), reader.bool()?)),
        _ => {
            return Err(ForwardParallelError::InvalidWire(
                "forward_spin_flag_invalid",
            ))
        }
    };
    let damage = reader.u32()?;
    Ok(ForwardPathStep::new(
        piece,
        rotation,
        rotation,
        x,
        y,
        hold,
        cleared_lines,
        spin,
        damage,
        0,
        [0; 4],
        0,
        [0; 4],
    ))
}

fn encode_board(output: &mut Vec<u8>, board: ForwardBoard) {
    for word in board.words() {
        put_u64(output, word);
    }
}

fn decode_board(reader: &mut Reader<'_>) -> Result<ForwardBoard, ForwardParallelError> {
    Ok(ForwardBoard::from_words([
        reader.u64()?,
        reader.u64()?,
        reader.u64()?,
        reader.u64()?,
    ]))
}

fn encode_piece_slice(output: &mut Vec<u8>, pieces: &[PieceKind]) {
    put_u32(output, pieces.len() as u32);
    output.extend(pieces.iter().map(|piece| piece_code(*piece)));
}

fn decode_piece_vec(reader: &mut Reader<'_>) -> Result<Vec<PieceKind>, ForwardParallelError> {
    let count = reader.count()?;
    let mut pieces = Vec::with_capacity(count);
    for _ in 0..count {
        pieces.push(piece_from_code(reader.u8()?)?);
    }
    Ok(pieces)
}

fn piece_code(piece: PieceKind) -> u8 {
    PieceKind::STANDARD_TETROMINOES
        .iter()
        .position(|candidate| *candidate == piece)
        .expect("standard piece") as u8
}

fn piece_from_code(code: u8) -> Result<PieceKind, ForwardParallelError> {
    PieceKind::STANDARD_TETROMINOES
        .get(usize::from(code))
        .copied()
        .ok_or(ForwardParallelError::InvalidWire("forward_piece_invalid"))
}

fn rule_code(profile: RuleProfileId) -> u8 {
    match profile {
        RuleProfileId::SrsPlus => 0,
        RuleProfileId::Srs => 1,
        RuleProfileId::SrsX => 2,
        RuleProfileId::NoKick => 3,
        RuleProfileId::Asc => 4,
        RuleProfileId::Ars => 5,
        RuleProfileId::Custom => 6,
        RuleProfileId::Jstris180 => 7,
    }
}

fn rule_from_code(code: u8) -> Result<RuleProfileId, ForwardParallelError> {
    match code {
        0 => Ok(RuleProfileId::SrsPlus),
        1 => Ok(RuleProfileId::Srs),
        2 => Ok(RuleProfileId::SrsX),
        3 => Ok(RuleProfileId::NoKick),
        4 => Ok(RuleProfileId::Asc),
        5 => Ok(RuleProfileId::Ars),
        6 => Ok(RuleProfileId::Custom),
        7 => Ok(RuleProfileId::Jstris180),
        _ => Err(ForwardParallelError::InvalidWire("forward_rule_invalid")),
    }
}

fn spin_profile_code(profile: SpinProfileId) -> u8 {
    match profile {
        SpinProfileId::Disabled => 0,
        SpinProfileId::TSpinSimple => 1,
        SpinProfileId::TSpins => 2,
        SpinProfileId::TSpinsPlus => 3,
        SpinProfileId::AllSpin => 4,
        SpinProfileId::AllSpinPlus => 5,
        SpinProfileId::AllMini => 6,
        SpinProfileId::AllMiniPlus => 7,
    }
}

fn spin_profile_from_code(code: u8) -> Result<SpinProfileId, ForwardParallelError> {
    match code {
        0 => Ok(SpinProfileId::Disabled),
        1 => Ok(SpinProfileId::TSpinSimple),
        2 => Ok(SpinProfileId::TSpins),
        3 => Ok(SpinProfileId::TSpinsPlus),
        4 => Ok(SpinProfileId::AllSpin),
        5 => Ok(SpinProfileId::AllSpinPlus),
        6 => Ok(SpinProfileId::AllMini),
        7 => Ok(SpinProfileId::AllMiniPlus),
        _ => Err(ForwardParallelError::InvalidWire(
            "forward_spin_profile_invalid",
        )),
    }
}

fn encode_option_u16(output: &mut Vec<u8>, value: Option<u16>) {
    put_u16(output, value.unwrap_or(u16::MAX));
}

fn put_header(output: &mut Vec<u8>, magic: u32) {
    put_u32(output, magic);
    put_u32(output, WIRE_VERSION);
}

fn put_u16(output: &mut Vec<u8>, value: u16) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_decimal_string(output: &mut Vec<u8>, value: &str) {
    debug_assert!(!value.is_empty());
    debug_assert!(value.bytes().all(|byte| byte.is_ascii_digit()));
    debug_assert!(value == "0" || !value.starts_with('0'));
    put_u32(
        output,
        u32::try_from(value.len()).expect("forward decimal count length fits the wire"),
    );
    output.extend_from_slice(value.as_bytes());
}

struct Reader<'a> {
    input: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, cursor: 0 }
    }

    fn require_header(&mut self, magic: u32) -> Result<(), ForwardParallelError> {
        if self.u32()? != magic {
            return Err(ForwardParallelError::InvalidWire(
                "forward_wire_magic_mismatch",
            ));
        }
        if self.u32()? != WIRE_VERSION {
            return Err(ForwardParallelError::InvalidWire(
                "forward_wire_version_unsupported",
            ));
        }
        Ok(())
    }

    fn count(&mut self) -> Result<usize, ForwardParallelError> {
        let count = self.u32()? as usize;
        if count > MAX_WIRE_ITEMS {
            return Err(ForwardParallelError::InvalidWire(
                "forward_wire_count_exceeded",
            ));
        }
        Ok(count)
    }

    fn bool(&mut self) -> Result<bool, ForwardParallelError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ForwardParallelError::InvalidWire("forward_bool_invalid")),
        }
    }

    fn decimal_string(&mut self) -> Result<String, ForwardParallelError> {
        let length = self.count()?;
        let bytes = self.take(length)?;
        if bytes.is_empty()
            || !bytes.iter().all(u8::is_ascii_digit)
            || bytes == b"0"
            || (bytes.len() > 1 && bytes[0] == b'0')
        {
            return Err(ForwardParallelError::InvalidWire(
                "forward_decimal_count_invalid",
            ));
        }
        String::from_utf8(bytes.to_vec())
            .map_err(|_| ForwardParallelError::InvalidWire("forward_decimal_count_invalid"))
    }

    fn option_u16(&mut self) -> Result<Option<u16>, ForwardParallelError> {
        Ok(match self.u16()? {
            u16::MAX => None,
            value => Some(value),
        })
    }

    fn u8(&mut self) -> Result<u8, ForwardParallelError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ForwardParallelError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two bytes"),
        ))
    }

    fn u32(&mut self) -> Result<u32, ForwardParallelError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes"),
        ))
    }

    fn u64(&mut self) -> Result<u64, ForwardParallelError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("eight bytes"),
        ))
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ForwardParallelError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(ForwardParallelError::InvalidWire(
                "forward_wire_length_overflow",
            ))?;
        let value = self
            .input
            .get(self.cursor..end)
            .ok_or(ForwardParallelError::InvalidWire("forward_wire_truncated"))?;
        self.cursor = end;
        Ok(value)
    }

    fn finish(self) -> Result<(), ForwardParallelError> {
        if self.cursor == self.input.len() {
            Ok(())
        } else {
            Err(ForwardParallelError::InvalidWire(
                "forward_wire_trailing_bytes",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        board::standard_pc_board::Board256Mask,
        execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    };
    use clearra_rules::profile::rule_profile::RuleProfileId;
    use clearra_scoring::profile::SpinProfileId;
    use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

    use super::*;
    use crate::{ForwardPieceSource, ForwardSearchSession};

    fn run_serial(query: ForwardSearchQuery) -> ForwardSearchReport {
        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        ForwardSearchSession::new(query)
            .expect("serial session")
            .run_to_completion(&control)
            .expect("serial search")
    }

    fn run_parallel(
        query: ForwardSearchQuery,
        reported_workers_used: usize,
    ) -> ForwardSearchReport {
        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let mut coordinator = ForwardParallelCoordinator::new(query, 4).expect("coordinator");
        let initialization = coordinator.worker_initialization();
        let mut workers = (0..3)
            .map(|_| ForwardParallelWorker::new(&initialization).expect("worker"))
            .collect::<Vec<_>>();

        loop {
            let mut partials = Vec::new();
            let mut completed = false;
            for worker in &mut workers {
                let (status, batch) = coordinator.produce(256, &control).expect("produce");
                match status {
                    ForwardParallelProduce::Batch => {
                        let (_, partial) = worker.consume(&batch, &control).expect("consume");
                        partials.push(partial);
                    }
                    ForwardParallelProduce::Completed => {
                        completed = true;
                        break;
                    }
                    ForwardParallelProduce::Pending => break,
                    ForwardParallelProduce::Cancelled => panic!("unexpected cancellation"),
                }
            }
            for partial in partials.into_iter().rev() {
                coordinator.absorb(&partial, &control).expect("absorb");
            }
            if completed {
                break;
            }
        }
        coordinator
            .finish_with_control(reported_workers_used, &control)
            .expect("parallel finish")
    }

    fn assert_exact_equivalence(query: ForwardSearchQuery) {
        let mut serial = run_serial(query.clone());
        let parallel = run_parallel(query, 4);
        canonicalize_outcomes(serial.outcomes_mut(), &ExecutionControl::default())
            .expect("canonicalize serial outcomes");
        assert_eq!(serial.initial_board(), parallel.initial_board());
        assert_eq!(serial.visited_states(), parallel.visited_states());
        assert_eq!(serial.generated_locks(), parallel.generated_locks());
        assert_eq!(serial.peak_frontier(), parallel.peak_frontier());
        assert_eq!(serial.maximum_damage(), parallel.maximum_damage());
        assert_eq!(serial.outcomes(), parallel.outcomes());
        assert_eq!(parallel.workers_used(), 4);
    }

    fn assert_same_search_semantics(mut left: ForwardSearchReport, mut right: ForwardSearchReport) {
        let control = ExecutionControl::default();
        canonicalize_outcomes(left.outcomes_mut(), &control).expect("canonicalize left outcomes");
        canonicalize_outcomes(right.outcomes_mut(), &control).expect("canonicalize right outcomes");
        assert_eq!(left.complete(), right.complete());
        assert_eq!(left.initial_board(), right.initial_board());
        assert_eq!(left.visited_states(), right.visited_states());
        assert_eq!(left.generated_locks(), right.generated_locks());
        assert_eq!(left.peak_frontier(), right.peak_frontier());
        assert_eq!(left.maximum_damage(), right.maximum_damage());
        assert_eq!(left.outcomes(), right.outcomes());
    }

    #[test]
    fn report_wire_preserves_arbitrary_precision_spin_evidence_count() {
        let expected = "340282366920938463463374607431768211456";
        let report = ForwardSearchReport::new(
            true,
            [0; 4],
            1,
            0,
            0,
            0,
            vec![ForwardSearchOutcome::new(
                1,
                0,
                vec![PieceKind::T],
                Some(ForwardSpinGroup::T),
                [0; 4],
                Some(PieceKind::T),
                false,
                1,
                None,
                0,
                Vec::new(),
            )
            .with_evidence_path_count(expected.to_owned())],
        );
        let mut bytes = Vec::new();
        encode_report(&mut bytes, &report);
        let mut reader = Reader::new(&bytes);
        let decoded = decode_report(&mut reader).expect("decode report");
        reader.finish().expect("consume report bytes");

        assert_eq!(decoded.outcomes()[0].evidence_path_count(), expected);
        assert!(decoded.outcomes()[0].evidence_complete());
    }

    #[test]
    fn serial_and_partitioned_boundaries_normalize_completed_initial_rows() {
        let full = 0x3ff_u64;
        let raw_board = full | (0x003 << 10) | (full << 20) | (0x004 << 30);
        let normalized_board = 0x003 | (0x004 << 10);
        let query = |board, mode| {
            ForwardSearchQuery::new(
                Board256Mask::from_words([board, 0, 0, 0]),
                4,
                vec![PieceKind::T],
                false,
                RuleProfileId::SrsPlus,
                SpinProfileId::TSpins,
                None,
                None,
                mode,
            )
        };

        for mode in [
            ForwardSearchMode::MaximumDamage,
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::default()),
        ] {
            let raw_serial = run_serial(query(raw_board, mode));
            let normalized_serial = run_serial(query(normalized_board, mode));
            let raw_parallel = run_parallel(query(raw_board, mode), 4);
            let normalized_parallel = run_parallel(query(normalized_board, mode), 4);

            for report in [
                &raw_serial,
                &normalized_serial,
                &raw_parallel,
                &normalized_parallel,
            ] {
                assert_eq!(report.initial_board(), [normalized_board, 0, 0, 0]);
            }
            assert_same_search_semantics(raw_serial.clone(), normalized_serial);
            assert_same_search_semantics(raw_parallel.clone(), normalized_parallel);
            assert_same_search_semantics(raw_serial, raw_parallel);
        }
    }

    #[test]
    fn finish_reports_the_actual_participating_worker_count() {
        let query = ForwardSearchQuery::new(
            Board256Mask::from_words([0; 4]),
            4,
            vec![PieceKind::O, PieceKind::O],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        );
        let expected = run_parallel(query.clone(), 4);
        let actual = run_parallel(query, 2);

        assert_eq!(actual.workers_used(), 2);
        assert_eq!(actual.visited_states(), expected.visited_states());
        assert_eq!(actual.generated_locks(), expected.generated_locks());
        assert_eq!(actual.peak_frontier(), expected.peak_frontier());
        assert_eq!(actual.maximum_damage(), expected.maximum_damage());
        assert_eq!(actual.outcomes(), expected.outcomes());
    }

    #[test]
    fn fixed_queue_layer_map_reduce_matches_serial_search_exactly() {
        let two_rows = (1_u64 << 20) - 1;
        let two_o_holes = 0b11_u64 | (0b11_u64 << 8) | (0b11_u64 << 10) | (0b11_u64 << 18);
        assert_exact_equivalence(ForwardSearchQuery::new(
            Board256Mask::from_words([two_rows & !two_o_holes, 0, 0, 0]),
            4,
            vec![PieceKind::O, PieceKind::O],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        ));
    }

    #[test]
    fn damage_threshold_layer_map_reduce_matches_serial_search_exactly() {
        let row_without_left_cell = ((1_u64 << 10) - 1) & !1_u64;
        let board = row_without_left_cell | (row_without_left_cell << 10);
        assert_exact_equivalence(ForwardSearchQuery::new(
            Board256Mask::from_words([board, 0, 0, 0]),
            4,
            vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::J],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::AllMiniPlus,
            None,
            None,
            ForwardSearchMode::DamageAtLeast(1),
        ));
    }

    #[test]
    fn pattern_queue_tasks_match_serial_search_exactly() {
        let right_blockers = (1_u64 << 2) | (1_u64 << 12);
        let pattern = QueuePatternExpression::parse("[OT]", 8).expect("pattern");
        assert_exact_equivalence(ForwardSearchQuery::new_with_source(
            Board256Mask::from_words([right_blockers, 0, 0, 0]),
            4,
            ForwardPieceSource::pattern(pattern),
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::AllMiniPlus,
            None,
            None,
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::new(
                Some(0),
                ForwardSpinCategory::Other,
            )),
        ));
    }

    #[test]
    fn long_single_pattern_uses_exact_layer_map_reduce() {
        let pattern = QueuePatternExpression::parse("OOOOOO", 8).expect("pattern");
        assert_exact_equivalence(ForwardSearchQuery::new_with_source(
            Board256Mask::from_words([(1_u64 << 40) - 1, 0, 0, 0]),
            4,
            ForwardPieceSource::pattern(pattern),
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::SpinFinder(ForwardSpinTarget::default()),
        ));
    }

    #[test]
    fn broad_pattern_universe_keeps_pattern_level_parallelism() {
        assert!(should_layer_pattern_search(6, 1, 8));
        assert!(should_layer_pattern_search(8, 2, 8));
        assert!(!should_layer_pattern_search(5, 1, 8));
        assert!(!should_layer_pattern_search(6, 0, 8));
        assert!(!should_layer_pattern_search(6, 3, 8));
        assert!(!should_layer_pattern_search(7, 5_040, 8));
    }

    #[test]
    fn forward_parallel_wire_preserves_jstris_180_rule_identity() {
        let id = RuleProfileId::Jstris180;

        assert_eq!(rule_from_code(rule_code(id)).expect("Jstris rule code"), id);
    }

    #[test]
    fn compact_action_wire_preserves_the_actual_placement_rotation() {
        let step = ForwardPathStep::new(
            PieceKind::I,
            RotationState::Zero,
            RotationState::Two,
            2,
            3,
            "swap",
            1,
            None,
            4,
            9,
            [1, 2, 3, 4],
            5,
            [6, 7, 8, 9],
        );
        let mut wire = Vec::new();
        encode_action_step(&mut wire, &step);

        let mut reader = Reader::new(&wire);
        let decoded = decode_action_step(&mut reader).expect("compact action step");
        reader.finish().expect("complete compact action step");

        assert_eq!(decoded.piece(), PieceKind::I);
        assert_eq!(decoded.rotation(), RotationState::Two);
        assert_eq!(decoded.placement_rotation(), RotationState::Two);
        assert_eq!(decoded.x(), 2);
        assert_eq!(decoded.y(), 3);
        assert_eq!(decoded.hold_decision(), "swap");
        assert_eq!(decoded.cleared_lines(), 1);
        assert_eq!(decoded.damage(), 4);
    }

    #[test]
    fn coordinator_backpressures_a_full_reorder_window() {
        let query = ForwardSearchQuery::new(
            Board256Mask::EMPTY,
            4,
            vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        );
        let mut coordinator = ForwardParallelCoordinator::new(query, 4).expect("coordinator");
        let max_outstanding_tasks = coordinator.max_outstanding_tasks;
        assert_eq!(max_outstanding_tasks, 3 * 32 * 4);
        let CoordinatorState::Fixed(fixed) = &mut coordinator.state else {
            panic!("fixed coordinator");
        };
        for id in 1..=max_outstanding_tasks as u64 {
            fixed.pending.insert(id, 0);
        }

        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let (status, batch) = coordinator
            .produce(256, &control)
            .expect("backpressure result");
        assert_eq!(status, ForwardParallelProduce::Pending);
        assert!(batch.is_empty());
    }

    #[test]
    fn cancelled_absorb_short_circuits_before_wire_decoding() {
        let query = ForwardSearchQuery::new(
            Board256Mask::EMPTY,
            4,
            vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S],
            false,
            RuleProfileId::SrsPlus,
            SpinProfileId::TSpins,
            None,
            None,
            ForwardSearchMode::MaximumDamage,
        );
        let mut coordinator = ForwardParallelCoordinator::new(query, 4).expect("coordinator");
        let cancellation = ExecutionCancellationToken::new();
        cancellation.handle().cancel();
        let control = ExecutionControl::new(cancellation);

        assert_eq!(
            coordinator
                .absorb(&[0xff], &control)
                .expect_err("cancelled absorb"),
            ForwardParallelError::Search(ForwardSearchError::Cancelled)
        );
    }
}
// SRP rationale: this module has one behavior-level change reason: the exact coordinator/worker protocol and its bounded, deterministic multi-worker scheduling.
