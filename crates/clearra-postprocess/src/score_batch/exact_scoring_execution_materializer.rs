// SRP rationale: this module has one change reason: materializing exact scoring executions from validated batches.
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

#[cfg(all(feature = "stage-profiling", not(target_arch = "wasm32")))]
use std::time::Instant;

#[cfg(all(feature = "stage-profiling", target_arch = "wasm32"))]
use wasm_bindgen::prelude::wasm_bindgen;

use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;
use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_objectives::policy::score_objective_policy::ScoreObjectivePolicy;
use clearra_replay::{
    BuildVariantOperation, BuildVariantReplayInput, ExactScoringExecutionBatch,
    ExactScoringExecutionGraph, HoldDecision, KickEvidenceEvent, MovementEvidenceEvent,
    ReplayEngine, ReplayTrace, ScoringExecutionEdge, TraceCanonicalKey, TraceCompleteness,
};
use clearra_scoring::{
    event::SpinDetector,
    model::{ScoreEvaluationPolicy, ScoreModelEvaluator},
    profile::ScoreProfile,
    state::ScoreState,
};

use super::{
    candidate_execution_aggregate::{CandidateExecution, CandidateExecutionAggregate},
    execution_supply::{
        first_standard_bag_lookahead, for_each_supply_successor, terminal_supply_state_is_accepted,
        SupplyState,
    },
};

#[cfg(feature = "stage-profiling")]
const PROFILE_SAMPLE_INTERVAL: u64 = 256;

#[cfg(all(feature = "stage-profiling", target_arch = "wasm32"))]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = performance, js_name = now)]
    fn performance_now_ms() -> f64;
}

#[cfg(all(feature = "stage-profiling", target_arch = "wasm32"))]
type ProfileInstant = f64;
#[cfg(all(feature = "stage-profiling", not(target_arch = "wasm32")))]
type ProfileInstant = Instant;

#[cfg(all(feature = "stage-profiling", target_arch = "wasm32"))]
#[inline]
fn profile_now() -> ProfileInstant {
    performance_now_ms()
}

#[cfg(all(feature = "stage-profiling", not(target_arch = "wasm32")))]
#[inline]
fn profile_now() -> ProfileInstant {
    Instant::now()
}

#[cfg(all(feature = "stage-profiling", target_arch = "wasm32"))]
#[inline]
fn profile_elapsed_ns(started: ProfileInstant) -> u64 {
    ((performance_now_ms() - started).max(0.0) * 1_000_000.0).min(u64::MAX as f64) as u64
}

#[cfg(all(feature = "stage-profiling", not(target_arch = "wasm32")))]
#[inline]
fn profile_elapsed_ns(started: ProfileInstant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

#[cfg(feature = "stage-profiling")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ExactScoringExecutionProfile {
    materialization_ns: u64,
    node_visit_count: u64,
    accepted_execution_count: u64,
    sample_interval: u64,
    sampled_execution_count: u64,
    clock_baseline_sample_ns: u64,
    replay_sample_ns: u64,
    spin_sample_ns: u64,
    spin_coverage_sample_ns: u64,
    spin_coverage_sample_count: u64,
    score_sample_ns: u64,
    trace_identity_sample_ns: u64,
    best_selection_sample_ns: u64,
}

#[cfg(feature = "stage-profiling")]
impl ExactScoringExecutionProfile {
    pub const fn materialization_ns(self) -> u64 {
        self.materialization_ns
    }

    pub const fn node_visit_count(self) -> u64 {
        self.node_visit_count
    }

    pub const fn accepted_execution_count(self) -> u64 {
        self.accepted_execution_count
    }

    pub const fn sample_interval(self) -> u64 {
        self.sample_interval
    }

    pub const fn sampled_execution_count(self) -> u64 {
        self.sampled_execution_count
    }

    pub const fn clock_baseline_sample_ns(self) -> u64 {
        self.clock_baseline_sample_ns
    }

    pub const fn replay_sample_ns(self) -> u64 {
        self.replay_sample_ns
    }

    pub const fn spin_sample_ns(self) -> u64 {
        self.spin_sample_ns
    }

    pub const fn spin_coverage_sample_ns(self) -> u64 {
        self.spin_coverage_sample_ns
    }

    pub const fn spin_coverage_sample_count(self) -> u64 {
        self.spin_coverage_sample_count
    }

    pub const fn score_sample_ns(self) -> u64 {
        self.score_sample_ns
    }

    pub const fn trace_identity_sample_ns(self) -> u64 {
        self.trace_identity_sample_ns
    }

    pub const fn best_selection_sample_ns(self) -> u64 {
        self.best_selection_sample_ns
    }
}

#[derive(Clone, Copy)]
enum ProfiledExecutionStage {
    ClockBaseline,
    Replay,
    Spin,
    SpinCoverage,
    Score,
    TraceIdentity,
    BestSelection,
}

struct MaterializationProfiler {
    #[cfg(feature = "stage-profiling")]
    started: ProfileInstant,
    #[cfg(feature = "stage-profiling")]
    profile: ExactScoringExecutionProfile,
    #[cfg(feature = "stage-profiling")]
    sample_current_execution: bool,
}

impl MaterializationProfiler {
    #[inline]
    fn begin() -> Self {
        Self {
            #[cfg(feature = "stage-profiling")]
            started: profile_now(),
            #[cfg(feature = "stage-profiling")]
            profile: ExactScoringExecutionProfile {
                sample_interval: PROFILE_SAMPLE_INTERVAL,
                ..ExactScoringExecutionProfile::default()
            },
            #[cfg(feature = "stage-profiling")]
            sample_current_execution: false,
        }
    }

    #[inline]
    fn record_node_visit(&mut self) {
        #[cfg(feature = "stage-profiling")]
        {
            self.profile.node_visit_count = self.profile.node_visit_count.saturating_add(1);
        }
    }

    #[inline]
    fn begin_terminal_execution(&mut self) {
        #[cfg(feature = "stage-profiling")]
        {
            let ordinal = self.profile.accepted_execution_count;
            self.profile.accepted_execution_count = ordinal.saturating_add(1);
            self.sample_current_execution =
                profile_sample_hash(ordinal) & (PROFILE_SAMPLE_INTERVAL - 1) == 0;
            if self.sample_current_execution {
                self.profile.sampled_execution_count =
                    self.profile.sampled_execution_count.saturating_add(1);
            }
        }
    }

    #[inline]
    fn measure<T>(&mut self, stage: ProfiledExecutionStage, operation: impl FnOnce() -> T) -> T {
        #[cfg(feature = "stage-profiling")]
        {
            if !self.sample_current_execution {
                return operation();
            }
            let started = profile_now();
            let output = operation();
            let elapsed = profile_elapsed_ns(started);
            let total = match stage {
                ProfiledExecutionStage::ClockBaseline => &mut self.profile.clock_baseline_sample_ns,
                ProfiledExecutionStage::Replay => &mut self.profile.replay_sample_ns,
                ProfiledExecutionStage::Spin => &mut self.profile.spin_sample_ns,
                ProfiledExecutionStage::SpinCoverage => {
                    self.profile.spin_coverage_sample_count =
                        self.profile.spin_coverage_sample_count.saturating_add(1);
                    &mut self.profile.spin_coverage_sample_ns
                }
                ProfiledExecutionStage::Score => &mut self.profile.score_sample_ns,
                ProfiledExecutionStage::TraceIdentity => &mut self.profile.trace_identity_sample_ns,
                ProfiledExecutionStage::BestSelection => &mut self.profile.best_selection_sample_ns,
            };
            *total = total.saturating_add(elapsed);
            output
        }
        #[cfg(not(feature = "stage-profiling"))]
        {
            let _ = stage;
            operation()
        }
    }

    #[cfg(feature = "stage-profiling")]
    #[inline]
    fn finish(self) -> Option<ExactScoringExecutionProfile> {
        let mut profiler = self;
        profiler.profile.materialization_ns = profile_elapsed_ns(profiler.started);
        Some(profiler.profile)
    }
}

#[cfg(feature = "stage-profiling")]
#[inline]
fn profile_sample_hash(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[derive(Clone, Debug)]
pub struct ExactScoringExecutionMaterialization {
    aggregates: Vec<CandidateExecutionAggregate>,
    scored_executions: Vec<ExactScoredExecution>,
    t_spin_single_pattern_ids: BTreeSet<usize>,
    t_spin_single_candidate_ids: BTreeSet<u64>,
    t_spin_single_execution_count: u128,
    complete: bool,
    #[cfg(feature = "stage-profiling")]
    profile: Option<ExactScoringExecutionProfile>,
}

impl ExactScoringExecutionMaterialization {
    pub fn aggregates(&self) -> &[CandidateExecutionAggregate] {
        &self.aggregates
    }

    pub fn scored_executions(&self) -> &[ExactScoredExecution] {
        &self.scored_executions
    }

    pub fn into_aggregates(self) -> Vec<CandidateExecutionAggregate> {
        self.aggregates
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn t_spin_single_pattern_ids(&self) -> impl Iterator<Item = usize> + '_ {
        self.t_spin_single_pattern_ids.iter().copied()
    }

    pub fn t_spin_single_candidate_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.t_spin_single_candidate_ids.iter().copied()
    }

    pub const fn t_spin_single_execution_count(&self) -> u128 {
        self.t_spin_single_execution_count
    }

    pub fn checked_replay_retained_bytes(&self) -> Option<u128> {
        let inline_bytes = (self.aggregates.capacity() as u128)
            .checked_mul(core::mem::size_of::<CandidateExecutionAggregate>() as u128)?;
        self.aggregates
            .iter()
            .try_fold(inline_bytes, |bytes, aggregate| {
                bytes.checked_add(aggregate.checked_nested_retained_bytes()?)
            })
    }

    #[cfg(feature = "stage-profiling")]
    pub const fn profile(&self) -> Option<ExactScoringExecutionProfile> {
        self.profile
    }
}

const COMPACT_SCORE_CELL_TRACE_ID_BYTES: usize = 63;

/// Checked peak projection for the typed score-cell-only path.
///
/// The retained cell vector has at most one entry for every graph/pattern
/// pair. Path and hold storage are one reusable traversal scratch pair, so
/// they are charged once at the largest graph depth rather than once per cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactScoreCellMemoryProjection {
    pub graph_count: usize,
    pub pattern_count: usize,
    pub max_path_len: usize,
    pub cell_capacity: usize,
    pub outer_storage_bytes: u128,
    pub trace_identity_storage_bytes: u128,
    pub path_scratch_bytes: u128,
    pub hold_scratch_bytes: u128,
    pub profile_storage_bytes: u128,
    pub required_peak_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactScoreCellMemoryReport {
    pub projection: ExactScoreCellMemoryProjection,
    pub retained_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactScoreCellMaterializationError {
    Cancelled,
    ProfileMismatch,
    ProjectionOverflow,
    LimitExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    AllocationFailed,
}

/// Score-only authority for typed `pc.score` materialization. It deliberately
/// has no aggregate or replay field; those belong to the compatibility path.
#[derive(Clone, Debug)]
pub struct ExactScoreCellMaterialization {
    scored_executions: Vec<ExactScoredExecution>,
    complete: bool,
}

impl ExactScoreCellMaterialization {
    pub fn scored_executions(&self) -> &[ExactScoredExecution] {
        &self.scored_executions
    }

    pub fn into_scored_executions(self) -> Vec<ExactScoredExecution> {
        self.scored_executions
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        (self.scored_executions.capacity() as u128)
            .checked_mul(core::mem::size_of::<ExactScoredExecution>() as u128)?
            .checked_add(
                self.scored_executions
                    .iter()
                    .try_fold(0_u128, |total, execution| {
                        total.checked_add(execution.trace_identity.capacity() as u128)
                    })?,
            )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactScoringExecutionCancelled;

/// Product-only limits for exhaustive replay materialization.
///
/// Complete-replay products intentionally have a separate policy from score
/// and save products: exceeding one of these limits is an execution error,
/// never a partial family reported as complete.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactReplayMaterializationLimits {
    max_executions: usize,
    max_path_steps: usize,
    max_retained_bytes: u128,
}

impl ExactReplayMaterializationLimits {
    pub const fn new(
        max_executions: usize,
        max_path_steps: usize,
        max_retained_bytes: u128,
    ) -> Self {
        Self {
            max_executions,
            max_path_steps,
            max_retained_bytes,
        }
    }

    pub const fn max_executions(self) -> usize {
        self.max_executions
    }

    pub const fn max_path_steps(self) -> usize {
        self.max_path_steps
    }

    pub const fn max_retained_bytes(self) -> u128 {
        self.max_retained_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactReplayMaterializationReport {
    execution_count: usize,
    raw_execution_count: usize,
    retained_bytes: u128,
    admitted_peak_bytes: u128,
}

impl ExactReplayMaterializationReport {
    pub const fn execution_count(self) -> usize {
        self.execution_count
    }

    pub const fn retained_bytes(self) -> u128 {
        self.retained_bytes
    }

    /// Accepted terminal visits before canonical trace deduplication.
    pub const fn raw_execution_count(self) -> usize {
        self.raw_execution_count
    }

    /// Largest checked live allocation projection, not process RSS.
    pub const fn admitted_peak_bytes(self) -> u128 {
        self.admitted_peak_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExactReplayMaterializationError {
    Cancelled,
    InvalidEvidence,
    ProjectionOverflow,
    ExecutionLimitExceeded {
        max_executions: usize,
    },
    PathStepLimitExceeded {
        max_path_steps: usize,
    },
    MemoryLimitExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    AllocationFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactScoredExecution {
    candidate_identity: StandardBoard64TilingIdentity,
    pattern_id: usize,
    trace_identity: String,
    score: u64,
    attack: u32,
}

impl ExactScoredExecution {
    pub const fn candidate_identity(&self) -> StandardBoard64TilingIdentity {
        self.candidate_identity
    }

    pub const fn pattern_id(&self) -> usize {
        self.pattern_id
    }

    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }

    pub const fn score(&self) -> u64 {
        self.score
    }

    pub const fn attack(&self) -> u32 {
        self.attack
    }

    pub fn into_parts(self) -> (StandardBoard64TilingIdentity, usize, String, u64, u32) {
        (
            self.candidate_identity,
            self.pattern_id,
            self.trace_identity,
            self.score,
            self.attack,
        )
    }
}

#[derive(Clone, Debug)]
struct BestExecution {
    score: u64,
    attack: u32,
    trace_identity: String,
    retained_path: Option<Vec<ScoringExecutionEdge>>,
    retained_holds: Option<Vec<HoldDecision>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScoreCellBestExecution {
    score: u64,
    attack: u32,
}

#[derive(Default)]
struct SpinCoverageAccumulator {
    pattern_ids: BTreeSet<usize>,
    candidate_ids: BTreeSet<u64>,
    execution_count: u128,
}

impl SpinCoverageAccumulator {
    fn record_t_spin_single(&mut self, candidate_id: u64, pattern_id: usize) {
        self.pattern_ids.insert(pattern_id);
        self.candidate_ids.insert(candidate_id);
        self.execution_count = self.execution_count.saturating_add(1);
    }
}

pub struct ExactScoringExecutionMaterializer;

impl ExactScoringExecutionMaterializer {
    /// Materializes one deterministic replay for every reachable terminal
    /// supply state in each `(candidate, pattern)` cell. Unlike score
    /// materialization, this deliberately does not collapse executions by a
    /// score key: terminal hold/cursor state is the evidence consumed by the
    /// save products.
    pub fn materialize_terminal_replays(
        batch: &ExactScoringExecutionBatch,
        control: &ExecutionControl,
    ) -> Result<ExactScoringExecutionMaterialization, ExactScoringExecutionCancelled> {
        let mut aggregates = Vec::with_capacity(batch.graphs().len());
        let mut complete = batch.complete();

        for (graph_index, graph) in batch.graphs().iter().enumerate() {
            if control.is_cancelled() {
                return Err(ExactScoringExecutionCancelled);
            }
            control.report_progress(
                "save-terminal-execution",
                graph_index as u64,
                Some(batch.graphs().len() as u64),
            );
            let mut executions = Vec::new();
            for (pattern_id, sequence) in batch.patterns().iter().enumerate() {
                let (mut terminal_executions, cell_complete) =
                    materialize_terminal_replays_for_cell(
                        batch, graph, sequence, pattern_id, control,
                    )?;
                complete &= cell_complete;
                executions.append(&mut terminal_executions);
            }
            aggregates.push(CandidateExecutionAggregate::new(
                graph.candidate_id(),
                executions,
            ));
        }
        control.report_progress(
            "save-terminal-execution",
            batch.graphs().len() as u64,
            Some(batch.graphs().len() as u64),
        );

        Ok(ExactScoringExecutionMaterialization {
            aggregates,
            scored_executions: Vec::new(),
            t_spin_single_pattern_ids: BTreeSet::new(),
            t_spin_single_candidate_ids: BTreeSet::new(),
            t_spin_single_execution_count: 0,
            complete,
            #[cfg(feature = "stage-profiling")]
            profile: None,
        })
    }

    /// Materializes every distinct valid replay witness retained by the exact
    /// execution DAG. This path is intentionally separate from
    /// [`Self::materialize_terminal_replays`], whose one-terminal-state
    /// representative semantics remain the authority for ordinary save
    /// products.
    ///
    /// Any resource-limit or allocation failure aborts the materialization;
    /// callers must not publish the partial prefix as a complete family.
    pub fn materialize_complete_replays_with_limits(
        batch: &ExactScoringExecutionBatch,
        control: &ExecutionControl,
        limits: ExactReplayMaterializationLimits,
    ) -> Result<
        (
            ExactScoringExecutionMaterialization,
            ExactReplayMaterializationReport,
        ),
        ExactReplayMaterializationError,
    > {
        Self::materialize_complete_replay_graph_range(
            batch,
            0..batch.graphs().len(),
            0..batch.patterns().len(),
            true,
            control,
            limits,
        )
    }

    /// The same exhaustive replay semantics restricted to one immutable
    /// geometry graph. Page owners use this to count/load a family without
    /// retaining the Cartesian expansion of every other geometry.
    pub fn materialize_complete_replay_graph_with_limits(
        batch: &ExactScoringExecutionBatch,
        graph_index: usize,
        control: &ExecutionControl,
        limits: ExactReplayMaterializationLimits,
    ) -> Result<
        (
            ExactScoringExecutionMaterialization,
            ExactReplayMaterializationReport,
        ),
        ExactReplayMaterializationError,
    > {
        if graph_index >= batch.graphs().len() {
            return Err(ExactReplayMaterializationError::InvalidEvidence);
        }
        Self::materialize_complete_replay_graph_range(
            batch,
            graph_index..graph_index + 1,
            0..batch.patterns().len(),
            true,
            control,
            limits,
        )
    }

    /// Exhaustive witnesses for one canonical geometry and actual pattern ID.
    /// Restricting the retained cell does not change its traversal or identity
    /// rules, and permits complete family manifests without a Cartesian owner.
    pub fn materialize_complete_replay_cell_with_limits(
        batch: &ExactScoringExecutionBatch,
        graph_index: usize,
        pattern_id: usize,
        control: &ExecutionControl,
        limits: ExactReplayMaterializationLimits,
    ) -> Result<
        (
            ExactScoringExecutionMaterialization,
            ExactReplayMaterializationReport,
        ),
        ExactReplayMaterializationError,
    > {
        if graph_index >= batch.graphs().len() || pattern_id >= batch.patterns().len() {
            return Err(ExactReplayMaterializationError::InvalidEvidence);
        }
        Self::materialize_complete_replay_graph_range(
            batch,
            graph_index..graph_index + 1,
            pattern_id..pattern_id + 1,
            false,
            control,
            limits,
        )
    }

    fn materialize_complete_replay_graph_range(
        batch: &ExactScoringExecutionBatch,
        graph_range: core::ops::Range<usize>,
        pattern_range: core::ops::Range<usize>,
        report_geometry_progress: bool,
        control: &ExecutionControl,
        limits: ExactReplayMaterializationLimits,
    ) -> Result<
        (
            ExactScoringExecutionMaterialization,
            ExactReplayMaterializationReport,
        ),
        ExactReplayMaterializationError,
    > {
        if control.is_cancelled() {
            return Err(ExactReplayMaterializationError::Cancelled);
        }
        let mut budget = CompleteReplayBudget::new(limits);
        budget.report_progress = report_geometry_progress;
        let mut aggregates = Vec::new();
        aggregates
            .try_reserve_exact(graph_range.len())
            .map_err(|_| ExactReplayMaterializationError::AllocationFailed)?;
        budget.charge_retained(
            (aggregates.capacity() as u128)
                .checked_mul(core::mem::size_of::<CandidateExecutionAggregate>() as u128)
                .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?,
            0,
        )?;
        let mut complete = batch.complete();

        for graph_index in graph_range {
            let graph = &batch.graphs()[graph_index];
            if control.is_cancelled() {
                return Err(ExactReplayMaterializationError::Cancelled);
            }
            if report_geometry_progress {
                control.report_progress(
                    "complete-replay-execution",
                    graph_index as u64,
                    Some(batch.graphs().len() as u64),
                );
            }
            let scratch_capacity = graph.node_count().min(limits.max_path_steps());
            let mut path = Vec::new();
            path.try_reserve_exact(scratch_capacity)
                .map_err(|_| ExactReplayMaterializationError::AllocationFailed)?;
            let mut holds = Vec::new();
            holds
                .try_reserve_exact(scratch_capacity)
                .map_err(|_| ExactReplayMaterializationError::AllocationFailed)?;
            let scratch_bytes = (path.capacity() as u128)
                .checked_mul(core::mem::size_of::<ScoringExecutionEdge>() as u128)
                .and_then(|bytes| {
                    (holds.capacity() as u128)
                        .checked_mul(core::mem::size_of::<HoldDecision>() as u128)
                        .and_then(|hold_bytes| bytes.checked_add(hold_bytes))
                })
                .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?;
            budget.ensure_peak(0, scratch_bytes)?;

            let mut executions = Vec::new();
            for pattern_id in pattern_range.clone() {
                let sequence = &batch.patterns()[pattern_id];
                path.clear();
                holds.clear();
                let cell_complete = visit_complete_replay_paths(
                    batch,
                    graph,
                    sequence,
                    pattern_id,
                    SupplyState {
                        node: graph.root(),
                        cursor: batch.initial_cursor(),
                        hold: batch.initial_hold(),
                    },
                    &mut path,
                    &mut holds,
                    scratch_bytes,
                    &mut executions,
                    &mut budget,
                    control,
                )?;
                complete &= cell_complete;
            }
            executions.sort_unstable_by(|left, right| {
                left.pattern_id()
                    .cmp(&right.pattern_id())
                    .then_with(|| left.trace_identity().cmp(right.trace_identity()))
            });
            executions.dedup_by(|left, right| {
                left.pattern_id() == right.pattern_id()
                    && left.trace_identity() == right.trace_identity()
            });
            aggregates.push(CandidateExecutionAggregate::new(
                graph.candidate_id(),
                executions,
            ));
        }
        if report_geometry_progress {
            control.report_progress(
                "complete-replay-execution",
                batch.graphs().len() as u64,
                Some(batch.graphs().len() as u64),
            );
        }

        let materialized = ExactScoringExecutionMaterialization {
            aggregates,
            scored_executions: Vec::new(),
            t_spin_single_pattern_ids: BTreeSet::new(),
            t_spin_single_candidate_ids: BTreeSet::new(),
            t_spin_single_execution_count: 0,
            complete,
            #[cfg(feature = "stage-profiling")]
            profile: None,
        };
        let retained_bytes = materialized
            .checked_replay_retained_bytes()
            .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?;
        if retained_bytes > limits.max_retained_bytes() {
            return Err(ExactReplayMaterializationError::MemoryLimitExceeded {
                required_memory_bytes: retained_bytes,
                max_memory_bytes: limits.max_retained_bytes(),
            });
        }
        let execution_count = materialized
            .aggregates()
            .iter()
            .try_fold(0_usize, |count, aggregate| {
                count.checked_add(aggregate.executions().len())
            })
            .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?;
        Ok((
            materialized,
            ExactReplayMaterializationReport {
                execution_count,
                raw_execution_count: budget.execution_count,
                retained_bytes,
                admitted_peak_bytes: budget.admitted_peak_bytes,
            },
        ))
    }

    pub fn checked_score_cell_memory_projection(
        batch: &ExactScoringExecutionBatch,
    ) -> Option<ExactScoreCellMemoryProjection> {
        Self::checked_score_cell_memory_projection_with_profile_bytes(batch, 0)
    }

    pub fn checked_score_cell_memory_projection_for_policy(
        batch: &ExactScoringExecutionBatch,
        score_policy: ScoreObjectivePolicy,
    ) -> Option<ExactScoreCellMemoryProjection> {
        let profile_storage_bytes =
            crate::checked_score_profile_memory_projection(score_policy)?.required_memory_bytes;
        Self::checked_score_cell_memory_projection_with_profile_bytes(batch, profile_storage_bytes)
    }

    pub fn checked_score_cell_memory_projection_with_profile_bytes(
        batch: &ExactScoringExecutionBatch,
        profile_storage_bytes: u128,
    ) -> Option<ExactScoreCellMemoryProjection> {
        let max_path_len = batch
            .graphs()
            .iter()
            .map(ExactScoringExecutionGraph::node_count)
            .max()
            .unwrap_or(0);
        checked_score_cell_memory_projection_for_shape(
            batch.graphs().len(),
            batch.patterns().len(),
            max_path_len,
            profile_storage_bytes,
        )
    }

    pub fn materialize_score_cells_with_memory_limit(
        batch: &ExactScoringExecutionBatch,
        score_policy: ScoreObjectivePolicy,
        control: &ExecutionControl,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<
        (ExactScoreCellMaterialization, ExactScoreCellMemoryReport),
        ExactScoreCellMaterializationError,
    > {
        if control.is_cancelled() {
            return Err(ExactScoreCellMaterializationError::Cancelled);
        }
        let projection = Self::checked_score_cell_memory_projection_for_policy(batch, score_policy)
            .ok_or(ExactScoreCellMaterializationError::ProjectionOverflow)?;
        let required_memory_bytes = already_retained_bytes
            .checked_add(projection.required_peak_bytes)
            .ok_or(ExactScoreCellMaterializationError::ProjectionOverflow)?;
        if required_memory_bytes > max_memory_bytes {
            return Err(ExactScoreCellMaterializationError::LimitExceeded {
                required_memory_bytes,
                max_memory_bytes,
            });
        }

        let (profile, profile_report) = crate::score_profile_with_memory_guard(
            score_policy,
            0,
            projection.profile_storage_bytes,
        )
        .map_err(|error| match error {
            crate::ScoreProfileMemoryGuardError::ProjectionOverflow => {
                ExactScoreCellMaterializationError::ProjectionOverflow
            }
            crate::ScoreProfileMemoryGuardError::LimitExceeded { .. } => {
                ExactScoreCellMaterializationError::ProjectionOverflow
            }
            crate::ScoreProfileMemoryGuardError::AllocationFailed => {
                ExactScoreCellMaterializationError::AllocationFailed
            }
        })?;
        debug_assert!(profile_report.retained_bytes <= projection.profile_storage_bytes);
        Self::materialize_score_cells_with_profile_and_memory_limit(
            batch,
            score_policy,
            &profile,
            profile_report.retained_bytes,
            control,
            already_retained_bytes,
            max_memory_bytes,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn materialize_score_cells_with_profile_and_memory_limit(
        batch: &ExactScoringExecutionBatch,
        score_policy: ScoreObjectivePolicy,
        profile: &ScoreProfile,
        profile_retained_bytes: u128,
        control: &ExecutionControl,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<
        (ExactScoreCellMaterialization, ExactScoreCellMemoryReport),
        ExactScoreCellMaterializationError,
    > {
        if control.is_cancelled() {
            return Err(ExactScoreCellMaterializationError::Cancelled);
        }
        if !crate::score_profile_selection::score_profile_matches_policy(profile, score_policy) {
            return Err(ExactScoreCellMaterializationError::ProfileMismatch);
        }
        let projection = Self::checked_score_cell_memory_projection_with_profile_bytes(
            batch,
            profile_retained_bytes,
        )
        .ok_or(ExactScoreCellMaterializationError::ProjectionOverflow)?;
        let required_memory_bytes = already_retained_bytes
            .checked_add(projection.required_peak_bytes)
            .ok_or(ExactScoreCellMaterializationError::ProjectionOverflow)?;
        if required_memory_bytes > max_memory_bytes {
            return Err(ExactScoreCellMaterializationError::LimitExceeded {
                required_memory_bytes,
                max_memory_bytes,
            });
        }
        Self::materialize_score_cells_with_prechecked_profile(
            batch,
            score_policy,
            profile,
            projection,
            control,
            already_retained_bytes,
            max_memory_bytes,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn materialize_score_cells_with_prechecked_profile(
        batch: &ExactScoringExecutionBatch,
        score_policy: ScoreObjectivePolicy,
        profile: &ScoreProfile,
        projection: ExactScoreCellMemoryProjection,
        control: &ExecutionControl,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<
        (ExactScoreCellMaterialization, ExactScoreCellMemoryReport),
        ExactScoreCellMaterializationError,
    > {
        let evaluation_policy = ScoreEvaluationPolicy::tetrio_pc(score_policy.initial_b2b());
        let mut scored_executions = Vec::new();
        scored_executions
            .try_reserve_exact(projection.cell_capacity)
            .map_err(|_| ExactScoreCellMaterializationError::AllocationFailed)?;
        let mut path = Vec::new();
        path.try_reserve_exact(projection.max_path_len)
            .map_err(|_| ExactScoreCellMaterializationError::AllocationFailed)?;
        let mut holds = Vec::new();
        holds
            .try_reserve_exact(projection.max_path_len)
            .map_err(|_| ExactScoreCellMaterializationError::AllocationFailed)?;
        let mut complete = batch.complete();

        for (graph_index, graph) in batch.graphs().iter().enumerate() {
            if control.is_cancelled() {
                return Err(ExactScoreCellMaterializationError::Cancelled);
            }
            control.report_progress(
                "score-cell-execution",
                graph_index as u64,
                Some(batch.graphs().len() as u64),
            );
            for (pattern_id, sequence) in batch.patterns().iter().enumerate() {
                path.clear();
                holds.clear();
                let mut best = None;
                let traversal_complete = visit_score_cell_paths(
                    batch,
                    graph,
                    sequence,
                    SupplyState {
                        node: graph.root(),
                        cursor: batch.initial_cursor(),
                        hold: batch.initial_hold(),
                    },
                    &mut path,
                    &mut holds,
                    projection.max_path_len,
                    profile,
                    ScoreModelEvaluator::initial_state(evaluation_policy),
                    &mut best,
                    control,
                )?;
                complete &= traversal_complete;
                if let Some(best) = best {
                    if scored_executions.len() >= projection.cell_capacity {
                        return Err(ExactScoreCellMaterializationError::ProjectionOverflow);
                    }
                    let trace_identity =
                        compact_score_cell_trace_identity(graph.candidate_id(), pattern_id)?;
                    scored_executions.push(ExactScoredExecution {
                        candidate_identity: graph.identity(),
                        pattern_id,
                        trace_identity,
                        score: best.score,
                        attack: best.attack,
                    });
                }
            }
        }
        control.report_progress(
            "score-cell-execution",
            batch.graphs().len() as u64,
            Some(batch.graphs().len() as u64),
        );
        let materialized = ExactScoreCellMaterialization {
            scored_executions,
            complete,
        };
        let retained_bytes = materialized
            .checked_retained_bytes()
            .ok_or(ExactScoreCellMaterializationError::ProjectionOverflow)?;
        let actual_required_memory_bytes = already_retained_bytes
            .checked_add(retained_bytes)
            .ok_or(ExactScoreCellMaterializationError::ProjectionOverflow)?;
        if actual_required_memory_bytes > max_memory_bytes {
            return Err(ExactScoreCellMaterializationError::LimitExceeded {
                required_memory_bytes: actual_required_memory_bytes,
                max_memory_bytes,
            });
        }
        Ok((
            materialized,
            ExactScoreCellMemoryReport {
                projection,
                retained_bytes,
            },
        ))
    }

    pub fn materialize(
        batch: &ExactScoringExecutionBatch,
        score_policy: ScoreObjectivePolicy,
        control: &ExecutionControl,
    ) -> Result<ExactScoringExecutionMaterialization, ExactScoringExecutionCancelled> {
        Self::materialize_with_replay_retention(batch, score_policy, true, control)
    }

    pub fn materialize_score_cells(
        batch: &ExactScoringExecutionBatch,
        score_policy: ScoreObjectivePolicy,
        control: &ExecutionControl,
    ) -> Result<ExactScoringExecutionMaterialization, ExactScoringExecutionCancelled> {
        Self::materialize_with_replay_retention(batch, score_policy, false, control)
    }

    fn materialize_with_replay_retention(
        batch: &ExactScoringExecutionBatch,
        score_policy: ScoreObjectivePolicy,
        retain_replays: bool,
        control: &ExecutionControl,
    ) -> Result<ExactScoringExecutionMaterialization, ExactScoringExecutionCancelled> {
        let profile = crate::score_profile_selection::score_profile(score_policy);
        let evaluation_policy = ScoreEvaluationPolicy::tetrio_pc(score_policy.initial_b2b());
        let mut aggregates = if retain_replays {
            Vec::with_capacity(batch.graphs().len())
        } else {
            Vec::new()
        };
        let mut scored_executions = Vec::new();
        let mut complete = batch.complete();
        let mut spin_coverage = SpinCoverageAccumulator::default();
        let mut profiler = MaterializationProfiler::begin();

        for (graph_index, graph) in batch.graphs().iter().enumerate() {
            if control.is_cancelled() {
                return Err(ExactScoringExecutionCancelled);
            }
            control.report_progress(
                "score-execution",
                graph_index as u64,
                Some(batch.graphs().len() as u64),
            );
            let mut executions = Vec::new();
            for (pattern_id, sequence) in batch.patterns().iter().enumerate() {
                let mut path = Vec::new();
                let mut holds = Vec::new();
                let mut best = None;
                let traversal_complete = visit_execution_paths(
                    batch,
                    graph,
                    sequence,
                    pattern_id,
                    SupplyState {
                        node: graph.root(),
                        cursor: batch.initial_cursor(),
                        hold: batch.initial_hold(),
                    },
                    &mut path,
                    &mut holds,
                    &profile,
                    ScoreModelEvaluator::initial_state(evaluation_policy),
                    false,
                    retain_replays,
                    &mut best,
                    &mut spin_coverage,
                    &mut profiler,
                    control,
                )?;
                complete &= traversal_complete;
                if let Some(best) = best {
                    let retained_trace = if retain_replays {
                        let path = best
                            .retained_path
                            .as_deref()
                            .expect("replay retention preserves the selected path");
                        let holds = best
                            .retained_holds
                            .as_deref()
                            .expect("replay retention preserves hold decisions");
                        let trace = profiler.measure(ProfiledExecutionStage::Replay, || {
                            replay_path(batch, graph, pattern_id, path, holds)
                        });
                        match trace {
                            Some(trace) if trace.canonical_key() == best.trace_identity => {
                                Some(trace)
                            }
                            _ => {
                                complete = false;
                                continue;
                            }
                        }
                    } else {
                        None
                    };
                    scored_executions.push(ExactScoredExecution {
                        candidate_identity: graph.identity(),
                        pattern_id,
                        trace_identity: best.trace_identity.clone(),
                        score: best.score,
                        attack: best.attack,
                    });
                    if let Some(trace) = retained_trace {
                        executions.push(CandidateExecution::new(
                            pattern_id,
                            best.trace_identity,
                            trace,
                        ));
                    }
                }
            }
            if retain_replays {
                aggregates.push(CandidateExecutionAggregate::new(
                    graph.candidate_id(),
                    executions,
                ));
            }
        }
        control.report_progress(
            "score-execution",
            batch.graphs().len() as u64,
            Some(batch.graphs().len() as u64),
        );
        #[cfg(feature = "stage-profiling")]
        let profile = profiler.finish();
        Ok(ExactScoringExecutionMaterialization {
            aggregates,
            scored_executions,
            t_spin_single_pattern_ids: spin_coverage.pattern_ids,
            t_spin_single_candidate_ids: spin_coverage.candidate_ids,
            t_spin_single_execution_count: spin_coverage.execution_count,
            complete,
            #[cfg(feature = "stage-profiling")]
            profile,
        })
    }
}

type TerminalSupplyKey = (u16, Option<PieceKind>);

#[derive(Clone, Debug)]
struct RetainedTerminalPath {
    edges: Vec<ScoringExecutionEdge>,
    holds: Vec<HoldDecision>,
}

#[allow(clippy::too_many_arguments)]
fn materialize_terminal_replays_for_cell(
    batch: &ExactScoringExecutionBatch,
    graph: &ExactScoringExecutionGraph,
    sequence: &[PieceKind],
    pattern_id: usize,
    control: &ExecutionControl,
) -> Result<(Vec<CandidateExecution>, bool), ExactScoringExecutionCancelled> {
    let node_count = graph.node_count();
    let mut states = (0..node_count)
        .map(|_| BTreeMap::<TerminalSupplyKey, RetainedTerminalPath>::new())
        .collect::<Vec<_>>();
    let Some(root_states) = states.get_mut(graph.root() as usize) else {
        return Ok((Vec::new(), false));
    };
    root_states.insert(
        (batch.initial_cursor(), batch.initial_hold()),
        RetainedTerminalPath {
            edges: Vec::new(),
            holds: Vec::new(),
        },
    );

    let mut executions = Vec::new();
    let mut complete = true;
    for node_index in 0..node_count {
        if control.is_cancelled() {
            return Err(ExactScoringExecutionCancelled);
        }
        let Some(node) = graph.node(node_index as u32) else {
            complete = false;
            continue;
        };
        let incoming = core::mem::take(&mut states[node_index]);
        if node.accepting() {
            for ((cursor, hold), retained) in incoming {
                if !terminal_supply_state_is_accepted(
                    batch,
                    sequence,
                    SupplyState {
                        node: node_index as u32,
                        cursor,
                        hold,
                    },
                ) {
                    continue;
                }
                let Some(trace_identity) = TraceCanonicalKey::from_scoring_path(
                    batch.layout(),
                    &retained.edges,
                    &retained.holds,
                )
                .map(|key| key.stable_key()) else {
                    complete = false;
                    continue;
                };
                let Some(trace) =
                    replay_path(batch, graph, pattern_id, &retained.edges, &retained.holds)
                else {
                    complete = false;
                    continue;
                };
                if trace.canonical_key() != trace_identity {
                    complete = false;
                    continue;
                }
                executions.push(CandidateExecution::new(pattern_id, trace_identity, trace));
            }
            continue;
        }

        for edge in graph.edges(node).iter().copied() {
            let child_index = edge.to() as usize;
            let Some(child) = graph.node(edge.to()) else {
                complete = false;
                continue;
            };
            if child_index <= node_index || child_index >= states.len() {
                complete = false;
                continue;
            }
            for (&(cursor, hold), retained) in &incoming {
                let state = SupplyState {
                    node: node_index as u32,
                    cursor,
                    hold,
                };
                if batch.projects_unplaced_lookahead()
                    && batch.hold_enabled()
                    && usize::from(cursor) == sequence.len()
                    && hold == Some(edge.piece())
                    && child.accepting()
                    && (!batch.projects_standard_bag_lookahead()
                        || first_standard_bag_lookahead(sequence).is_none())
                {
                    retain_terminal_successor(
                        &mut states[child_index],
                        SupplyState {
                            node: edge.to(),
                            cursor: cursor.saturating_add(1),
                            hold,
                        },
                        retained,
                        edge,
                        HoldDecision::ReleaseHeldAtTerminal {
                            held_piece: edge.piece(),
                        },
                    );
                }
                for_each_supply_successor(
                    batch,
                    sequence,
                    state,
                    edge.piece(),
                    |decision, next| {
                        retain_terminal_successor(
                            &mut states[child_index],
                            SupplyState {
                                node: edge.to(),
                                ..next
                            },
                            retained,
                            edge,
                            decision,
                        );
                        Ok(())
                    },
                )?;
            }
        }
    }
    executions.sort_unstable_by(|left, right| left.trace_identity().cmp(right.trace_identity()));
    Ok((executions, complete))
}

fn retain_terminal_successor(
    states: &mut BTreeMap<TerminalSupplyKey, RetainedTerminalPath>,
    next: SupplyState,
    retained: &RetainedTerminalPath,
    edge: ScoringExecutionEdge,
    hold: HoldDecision,
) {
    states.entry((next.cursor, next.hold)).or_insert_with(|| {
        let mut edges = retained.edges.clone();
        edges.push(edge);
        let mut holds = retained.holds.clone();
        holds.push(hold);
        RetainedTerminalPath { edges, holds }
    });
}

#[derive(Clone, Copy, Debug)]
struct CompleteReplayBudget {
    limits: ExactReplayMaterializationLimits,
    execution_count: usize,
    retained_bytes: u128,
    visited_nodes: u64,
    report_progress: bool,
    admitted_peak_bytes: u128,
}

const COMPLETE_REPLAY_PROGRESS_CADENCE: u64 = 4_096;

impl CompleteReplayBudget {
    const fn new(limits: ExactReplayMaterializationLimits) -> Self {
        Self {
            limits,
            execution_count: 0,
            retained_bytes: 0,
            visited_nodes: 0,
            report_progress: true,
            admitted_peak_bytes: 0,
        }
    }

    fn note_visit(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), ExactReplayMaterializationError> {
        self.visited_nodes = self
            .visited_nodes
            .checked_add(1)
            .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?;
        if self.report_progress
            && (self.visited_nodes == 1
                || self.visited_nodes % COMPLETE_REPLAY_PROGRESS_CADENCE == 0)
        {
            control.report_progress("complete-replay-traversal", self.visited_nodes, None);
        }
        Ok(())
    }

    fn ensure_peak(
        &mut self,
        additional_retained_bytes: u128,
        scratch_bytes: u128,
    ) -> Result<(), ExactReplayMaterializationError> {
        let required_memory_bytes = self
            .retained_bytes
            .checked_add(additional_retained_bytes)
            .and_then(|bytes| bytes.checked_add(scratch_bytes))
            .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?;
        if required_memory_bytes > self.limits.max_retained_bytes() {
            return Err(ExactReplayMaterializationError::MemoryLimitExceeded {
                required_memory_bytes,
                max_memory_bytes: self.limits.max_retained_bytes(),
            });
        }
        self.admitted_peak_bytes = self.admitted_peak_bytes.max(required_memory_bytes);
        Ok(())
    }

    fn charge_retained(
        &mut self,
        additional_retained_bytes: u128,
        scratch_bytes: u128,
    ) -> Result<(), ExactReplayMaterializationError> {
        self.ensure_peak(additional_retained_bytes, scratch_bytes)?;
        self.retained_bytes = self
            .retained_bytes
            .checked_add(additional_retained_bytes)
            .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?;
        Ok(())
    }

    fn retain_execution(
        &mut self,
        executions: &mut Vec<CandidateExecution>,
        pattern_id: usize,
        trace_identity: String,
        trace: ReplayTrace,
        scratch_bytes: u128,
    ) -> Result<(), ExactReplayMaterializationError> {
        if self.execution_count >= self.limits.max_executions() {
            return Err(ExactReplayMaterializationError::ExecutionLimitExceeded {
                max_executions: self.limits.max_executions(),
            });
        }
        let nested_bytes = (trace_identity.capacity() as u128)
            .checked_add(
                trace
                    .checked_nested_retained_bytes()
                    .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?,
            )
            .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?;
        let before_capacity = executions.capacity();
        // Reallocation may keep the old backing allocation alive until the
        // new one is obtained. The old capacity is already retained; admit the
        // whole new requested buffer, not only its one-element growth.
        let requested_inline_bytes = if executions.len() == before_capacity {
            (executions.len() as u128)
                .checked_add(1)
                .and_then(|n| n.checked_mul(core::mem::size_of::<CandidateExecution>() as u128))
                .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?
        } else {
            0
        };
        self.ensure_peak(
            nested_bytes
                .checked_add(requested_inline_bytes)
                .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?,
            scratch_bytes,
        )?;
        executions
            .try_reserve_exact(1)
            .map_err(|_| ExactReplayMaterializationError::AllocationFailed)?;
        let additional_inline_bytes = (executions.capacity() - before_capacity) as u128
            * core::mem::size_of::<CandidateExecution>() as u128;
        self.charge_retained(
            nested_bytes
                .checked_add(additional_inline_bytes)
                .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?,
            scratch_bytes,
        )?;
        executions.push(CandidateExecution::new(pattern_id, trace_identity, trace));
        self.execution_count += 1;
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn visit_complete_replay_paths(
    batch: &ExactScoringExecutionBatch,
    graph: &ExactScoringExecutionGraph,
    sequence: &[PieceKind],
    pattern_id: usize,
    state: SupplyState,
    path: &mut Vec<ScoringExecutionEdge>,
    holds: &mut Vec<HoldDecision>,
    scratch_bytes: u128,
    executions: &mut Vec<CandidateExecution>,
    budget: &mut CompleteReplayBudget,
    control: &ExecutionControl,
) -> Result<bool, ExactReplayMaterializationError> {
    if control.is_cancelled() {
        return Err(ExactReplayMaterializationError::Cancelled);
    }
    budget.note_visit(control)?;
    let Some(node) = graph.node(state.node) else {
        return Ok(false);
    };
    if node.accepting() {
        if !terminal_supply_state_is_accepted(batch, sequence, state) {
            return Ok(true);
        }
        if path.is_empty() {
            return Ok(false);
        }
        let Some(trace) = replay_path(batch, graph, pattern_id, path, holds) else {
            return Ok(false);
        };
        let key_projection = trace
            .checked_nested_retained_bytes()
            .and_then(|n| n.checked_add(trace.checked_canonical_key_requested_bytes()?))
            .and_then(|n| n.checked_add(core::mem::size_of::<ReplayTrace>() as u128))
            .ok_or(ExactReplayMaterializationError::ProjectionOverflow)?;
        budget.ensure_peak(key_projection, scratch_bytes)?;
        let trace_identity = trace.canonical_key();
        budget.retain_execution(executions, pattern_id, trace_identity, trace, scratch_bytes)?;
        return Ok(true);
    }

    let Some(edges) = graph.checked_edges(node) else {
        return Ok(false);
    };
    let mut complete = true;
    for &edge in edges {
        let child_index = edge.to() as usize;
        let Some(child) = graph.node(edge.to()) else {
            complete = false;
            continue;
        };
        if child_index <= state.node as usize {
            complete = false;
            continue;
        }
        if batch.projects_unplaced_lookahead()
            && batch.hold_enabled()
            && state.cursor as usize == sequence.len()
            && state.hold == Some(edge.piece())
            && child.accepting()
            && (!batch.projects_standard_bag_lookahead()
                || first_standard_bag_lookahead(sequence).is_none())
        {
            push_complete_replay_scratch(
                path,
                holds,
                edge,
                HoldDecision::ReleaseHeldAtTerminal {
                    held_piece: edge.piece(),
                },
                budget.limits.max_path_steps(),
            )?;
            let branch = visit_complete_replay_paths(
                batch,
                graph,
                sequence,
                pattern_id,
                SupplyState {
                    node: edge.to(),
                    cursor: state.cursor.saturating_add(1),
                    hold: state.hold,
                },
                path,
                holds,
                scratch_bytes,
                executions,
                budget,
                control,
            );
            holds.pop();
            path.pop();
            complete &= branch?;
        }

        let mut branch_error = None;
        let supply_result =
            for_each_supply_successor(batch, sequence, state, edge.piece(), |decision, next| {
                if let Err(error) = push_complete_replay_scratch(
                    path,
                    holds,
                    edge,
                    decision,
                    budget.limits.max_path_steps(),
                ) {
                    branch_error = Some(error);
                    return Err(ExactScoringExecutionCancelled);
                }
                let branch = visit_complete_replay_paths(
                    batch,
                    graph,
                    sequence,
                    pattern_id,
                    SupplyState {
                        node: edge.to(),
                        ..next
                    },
                    path,
                    holds,
                    scratch_bytes,
                    executions,
                    budget,
                    control,
                );
                holds.pop();
                path.pop();
                match branch {
                    Ok(branch_complete) => complete &= branch_complete,
                    Err(error) => {
                        branch_error = Some(error);
                        return Err(ExactScoringExecutionCancelled);
                    }
                }
                Ok(())
            });
        if let Some(error) = branch_error {
            return Err(error);
        }
        if supply_result.is_err() {
            return Err(if control.is_cancelled() {
                ExactReplayMaterializationError::Cancelled
            } else {
                ExactReplayMaterializationError::InvalidEvidence
            });
        }
    }
    Ok(complete)
}

fn push_complete_replay_scratch(
    path: &mut Vec<ScoringExecutionEdge>,
    holds: &mut Vec<HoldDecision>,
    edge: ScoringExecutionEdge,
    hold: HoldDecision,
    max_path_steps: usize,
) -> Result<(), ExactReplayMaterializationError> {
    if path.len() >= max_path_steps || holds.len() >= max_path_steps {
        return Err(ExactReplayMaterializationError::PathStepLimitExceeded { max_path_steps });
    }
    if path.len() == path.capacity() || holds.len() == holds.capacity() {
        return Err(ExactReplayMaterializationError::ProjectionOverflow);
    }
    path.push(edge);
    holds.push(hold);
    Ok(())
}

fn checked_score_cell_memory_projection_for_shape(
    graph_count: usize,
    pattern_count: usize,
    max_path_len: usize,
    profile_storage_bytes: u128,
) -> Option<ExactScoreCellMemoryProjection> {
    let cell_capacity = graph_count.checked_mul(pattern_count)?;
    let outer_storage_bytes =
        (cell_capacity as u128).checked_mul(core::mem::size_of::<ExactScoredExecution>() as u128)?;
    let trace_identity_storage_bytes =
        (cell_capacity as u128).checked_mul(COMPACT_SCORE_CELL_TRACE_ID_BYTES as u128)?;
    let path_scratch_bytes =
        (max_path_len as u128).checked_mul(core::mem::size_of::<ScoringExecutionEdge>() as u128)?;
    let hold_scratch_bytes =
        (max_path_len as u128).checked_mul(core::mem::size_of::<HoldDecision>() as u128)?;
    let required_peak_bytes = outer_storage_bytes
        .checked_add(trace_identity_storage_bytes)?
        .checked_add(path_scratch_bytes)?
        .checked_add(hold_scratch_bytes)?
        .checked_add(profile_storage_bytes)?;
    Some(ExactScoreCellMemoryProjection {
        graph_count,
        pattern_count,
        max_path_len,
        cell_capacity,
        outer_storage_bytes,
        trace_identity_storage_bytes,
        path_scratch_bytes,
        hold_scratch_bytes,
        profile_storage_bytes,
        required_peak_bytes,
    })
}

fn compact_score_cell_trace_identity(
    candidate_id: u64,
    pattern_id: usize,
) -> Result<String, ExactScoreCellMaterializationError> {
    let mut identity = String::new();
    identity
        .try_reserve_exact(COMPACT_SCORE_CELL_TRACE_ID_BYTES)
        .map_err(|_| ExactScoreCellMaterializationError::AllocationFailed)?;
    write!(
        identity,
        "score-cell-v1:{candidate_id:016x}:{:032x}",
        pattern_id as u128
    )
    .map_err(|_| ExactScoreCellMaterializationError::AllocationFailed)?;
    debug_assert_eq!(identity.len(), COMPACT_SCORE_CELL_TRACE_ID_BYTES);
    Ok(identity)
}

#[allow(clippy::too_many_arguments)]
fn visit_score_cell_paths(
    batch: &ExactScoringExecutionBatch,
    graph: &ExactScoringExecutionGraph,
    sequence: &[PieceKind],
    state: SupplyState,
    path: &mut Vec<ScoringExecutionEdge>,
    holds: &mut Vec<HoldDecision>,
    max_path_len: usize,
    profile: &ScoreProfile,
    score_state: ScoreState,
    best: &mut Option<ScoreCellBestExecution>,
    control: &ExecutionControl,
) -> Result<bool, ExactScoreCellMaterializationError> {
    if control.is_cancelled() {
        return Err(ExactScoreCellMaterializationError::Cancelled);
    }
    let Some(node) = graph.node(state.node) else {
        return Ok(false);
    };
    if node.accepting() {
        if terminal_supply_state_is_accepted(batch, sequence, state) {
            let candidate = ScoreCellBestExecution {
                score: score_state.score(),
                attack: score_state.attack(),
            };
            // Attack is informational. This compact path deliberately keeps
            // the first deterministic traversal representative of an exact
            // score tie instead of using attack as a hidden tiebreaker.
            if best
                .as_ref()
                .is_none_or(|current| candidate.score > current.score)
            {
                *best = Some(candidate);
            }
        }
        return Ok(true);
    }

    let mut complete = true;
    for &edge in graph.edges(node) {
        let next_score_state = ScoreModelEvaluator::evaluate_classified_lock(
            profile,
            score_state,
            path.len(),
            edge.cleared_lines(),
            edge.perfect_clear(),
            SpinDetector::detect_scoring_edge_with_profile(edge, profile.spin_profile()),
        );
        if batch.projects_unplaced_lookahead()
            && batch.hold_enabled()
            && state.cursor as usize == sequence.len()
            && state.hold == Some(edge.piece())
            && graph.node(edge.to()).is_some_and(|child| child.accepting())
            && (!batch.projects_standard_bag_lookahead()
                || first_standard_bag_lookahead(sequence).is_none())
        {
            push_score_cell_scratch(
                path,
                holds,
                edge,
                HoldDecision::ReleaseHeldAtTerminal {
                    held_piece: edge.piece(),
                },
                max_path_len,
            )?;
            let result = visit_score_cell_paths(
                batch,
                graph,
                sequence,
                SupplyState {
                    node: edge.to(),
                    cursor: state.cursor.saturating_add(1),
                    hold: state.hold,
                },
                path,
                holds,
                max_path_len,
                profile,
                next_score_state,
                best,
                control,
            );
            holds.pop();
            path.pop();
            complete &= result?;
        }

        let mut branch_error = None;
        let supply_result =
            for_each_supply_successor(batch, sequence, state, edge.piece(), |decision, next| {
                if let Err(error) =
                    push_score_cell_scratch(path, holds, edge, decision, max_path_len)
                {
                    branch_error = Some(error);
                    return Err(ExactScoringExecutionCancelled);
                }
                let result = visit_score_cell_paths(
                    batch,
                    graph,
                    sequence,
                    SupplyState {
                        node: edge.to(),
                        ..next
                    },
                    path,
                    holds,
                    max_path_len,
                    profile,
                    next_score_state,
                    best,
                    control,
                );
                holds.pop();
                path.pop();
                match result {
                    Ok(path_complete) => complete &= path_complete,
                    Err(error) => {
                        branch_error = Some(error);
                        return Err(ExactScoringExecutionCancelled);
                    }
                }
                Ok(())
            });
        if let Some(error) = branch_error {
            return Err(error);
        }
        if supply_result.is_err() {
            return Err(ExactScoreCellMaterializationError::Cancelled);
        }
    }
    Ok(complete)
}

fn push_score_cell_scratch(
    path: &mut Vec<ScoringExecutionEdge>,
    holds: &mut Vec<HoldDecision>,
    edge: ScoringExecutionEdge,
    hold: HoldDecision,
    max_path_len: usize,
) -> Result<(), ExactScoreCellMaterializationError> {
    if path.len() >= max_path_len
        || holds.len() >= max_path_len
        || path.len() == path.capacity()
        || holds.len() == holds.capacity()
    {
        return Err(ExactScoreCellMaterializationError::ProjectionOverflow);
    }
    path.push(edge);
    holds.push(hold);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn visit_execution_paths(
    batch: &ExactScoringExecutionBatch,
    graph: &ExactScoringExecutionGraph,
    sequence: &[PieceKind],
    pattern_id: usize,
    state: SupplyState,
    path: &mut Vec<ScoringExecutionEdge>,
    holds: &mut Vec<HoldDecision>,
    profile: &ScoreProfile,
    score_state: ScoreState,
    has_t_spin_single: bool,
    retain_replay: bool,
    best: &mut Option<BestExecution>,
    spin_coverage: &mut SpinCoverageAccumulator,
    profiler: &mut MaterializationProfiler,
    control: &ExecutionControl,
) -> Result<bool, ExactScoringExecutionCancelled> {
    if control.is_cancelled() {
        return Err(ExactScoringExecutionCancelled);
    }
    profiler.record_node_visit();
    let Some(node) = graph.node(state.node) else {
        return Ok(false);
    };
    if node.accepting() {
        if terminal_supply_state_is_accepted(batch, sequence, state) {
            profiler.begin_terminal_execution();
            profiler.measure(ProfiledExecutionStage::ClockBaseline, || {});
            if has_t_spin_single {
                profiler.measure(ProfiledExecutionStage::SpinCoverage, || {
                    spin_coverage.record_t_spin_single(graph.candidate_id(), pattern_id);
                });
            }
            let score = score_state.score();
            let attack = score_state.attack();
            let could_improve = best.as_ref().is_none_or(|current| score >= current.score);
            if !could_improve {
                return Ok(true);
            }
            let Some(trace_identity) =
                profiler.measure(ProfiledExecutionStage::TraceIdentity, || {
                    TraceCanonicalKey::from_scoring_path(batch.layout(), path, holds)
                        .map(|key| key.stable_key())
                })
            else {
                return Ok(false);
            };
            let candidate = BestExecution {
                score,
                attack,
                trace_identity,
                retained_path: retain_replay.then(|| path.clone()),
                retained_holds: retain_replay.then(|| holds.clone()),
            };
            profiler.measure(ProfiledExecutionStage::BestSelection, || {
                if best.as_ref().is_none_or(|current| {
                    candidate.score > current.score
                        || (candidate.score == current.score
                            && candidate.trace_identity < current.trace_identity)
                }) {
                    *best = Some(candidate);
                }
            });
        }
        return Ok(true);
    }

    let mut complete = true;
    for &edge in graph.edges(node) {
        let edge_has_t_spin_single = profiler.measure(ProfiledExecutionStage::Spin, || {
            SpinDetector::is_exact_t_spin_single_edge(edge)
        });
        let next_score_state = profiler.measure(ProfiledExecutionStage::Score, || {
            ScoreModelEvaluator::evaluate_classified_lock(
                profile,
                score_state,
                path.len(),
                edge.cleared_lines(),
                edge.perfect_clear(),
                SpinDetector::detect_scoring_edge_with_profile(edge, profile.spin_profile()),
            )
        });
        if batch.projects_unplaced_lookahead()
            && batch.hold_enabled()
            && state.cursor as usize == sequence.len()
            && state.hold == Some(edge.piece())
            && graph.node(edge.to()).is_some_and(|child| child.accepting())
            && (!batch.projects_standard_bag_lookahead()
                || first_standard_bag_lookahead(sequence).is_none())
        {
            path.push(edge);
            holds.push(HoldDecision::ReleaseHeldAtTerminal {
                held_piece: edge.piece(),
            });
            let path_complete = visit_execution_paths(
                batch,
                graph,
                sequence,
                pattern_id,
                SupplyState {
                    node: edge.to(),
                    cursor: state.cursor.saturating_add(1),
                    hold: state.hold,
                },
                path,
                holds,
                profile,
                next_score_state,
                has_t_spin_single || edge_has_t_spin_single,
                retain_replay,
                best,
                spin_coverage,
                profiler,
                control,
            )?;
            complete &= path_complete;
            holds.pop();
            path.pop();
        }
        for_each_supply_successor(batch, sequence, state, edge.piece(), |decision, next| {
            path.push(edge);
            holds.push(decision);
            let result = visit_execution_paths(
                batch,
                graph,
                sequence,
                pattern_id,
                SupplyState {
                    node: edge.to(),
                    ..next
                },
                path,
                holds,
                profile,
                next_score_state,
                has_t_spin_single || edge_has_t_spin_single,
                retain_replay,
                best,
                spin_coverage,
                profiler,
                control,
            );
            holds.pop();
            path.pop();
            match result {
                Ok(path_complete) => complete &= path_complete,
                Err(error) => return Err(error),
            }
            Ok(())
        })?;
    }
    Ok(complete)
}

pub(super) fn replay_path(
    batch: &ExactScoringExecutionBatch,
    graph: &ExactScoringExecutionGraph,
    pattern_id: usize,
    path: &[ScoringExecutionEdge],
    holds: &[HoldDecision],
) -> Option<ReplayTrace> {
    let mut operations = Vec::with_capacity(path.len());
    let mut movement = Vec::with_capacity(path.len());
    let mut kicks = Vec::new();
    for (step_index, edge) in path.iter().copied().enumerate() {
        let x = u16::try_from(edge.x()).ok()?;
        let y = u16::try_from(edge.y()).ok()?;
        operations.push(BuildVariantOperation::new(
            edge.piece(),
            edge.rotation(),
            x,
            y,
        ));
        let evidence = edge.lock_evidence();
        movement.push(MovementEvidenceEvent::new(
            step_index,
            true,
            evidence.last_action_was_rotation(),
            evidence.used_kick(),
            evidence.used_180(),
            true,
        ));
        if evidence.last_action_was_rotation() {
            let predecessor = evidence.predecessor();
            kicks.push(
                KickEvidenceEvent::new(
                    step_index,
                    evidence.from_rotation().quarter_turns(),
                    edge.rotation().quarter_turns(),
                    evidence.rotation_request(),
                    evidence.kick_index(),
                    i16::from(evidence.kick_dx()),
                    i16::from(evidence.kick_dy()),
                )
                .with_profile_ids(batch.kick_table_id(), batch.rule_profile_id())
                .with_anchors(
                    (i16::from(predecessor.0), i16::from(predecessor.1)),
                    (i16::from(edge.x()), i16::from(edge.y())),
                )
                .with_first_success_confirmed(true),
            );
        }
    }
    let variant_id = format!("candidate:{}:pattern:{}", graph.candidate_id(), pattern_id);
    let input = BuildVariantReplayInput::new(
        variant_id,
        batch.layout(),
        batch.initial_occupied(),
        operations,
    )
    .with_hold_decisions(holds.to_vec())
    .with_trace_marker(false, false)
    .with_movement_evidence(movement)
    .with_kick_evidence(kicks)
    .with_trace_completeness(TraceCompleteness::Complete);
    let trace = ReplayEngine::build_variant_to_trace(&input).ok()?;
    trace
        .solution_trace()
        .steps()
        .iter()
        .zip(path)
        .all(|(step, edge)| step.line_clear().cleared_lines() == edge.cleared_lines())
        .then_some(trace)
}

#[cfg(test)]
mod score_cell_memory_tests {
    use clearra_core_domain::{
        execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
        piece::{piece_kind::PieceKind, rotation::RotationState},
        solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
    };
    use clearra_geometry::layout::board64_layout::Board64Layout;
    use clearra_objectives::policy::score_objective_policy::ScoreObjectivePolicy;
    use clearra_replay::{
        ExactScoringExecutionBatch, ExactScoringExecutionGraph, ScoringExecutionEdge,
        ScoringExecutionNode, ScoringLockEvidence,
    };

    use super::*;

    fn batch() -> ExactScoringExecutionBatch {
        let identity = StandardBoard64TilingIdentity::from_placements(
            0,
            [PiecePlacementMask::new(PieceKind::I, 0xf)],
        )
        .expect("identity");
        let edges = vec![edge(1, 1, false), edge(2, 2, true)];
        ExactScoringExecutionBatch::new(
            Board64Layout::standard_10_by_lines(4).expect("layout"),
            0,
            vec![vec![PieceKind::I]],
            0,
            None,
            false,
            false,
            false,
            1,
            1,
            vec![ExactScoringExecutionGraph::new(
                1,
                identity,
                0,
                vec![
                    ScoringExecutionNode::new(0, edges.len() as u32, false),
                    ScoringExecutionNode::new(edges.len() as u32, 0, true),
                    ScoringExecutionNode::new(edges.len() as u32, 0, true),
                ],
                edges,
            )],
            true,
        )
    }

    fn edge(to: u32, cleared_lines: u8, perfect_clear: bool) -> ScoringExecutionEdge {
        ScoringExecutionEdge::new(
            to,
            0,
            PieceKind::I,
            RotationState::Zero,
            0,
            0,
            cleared_lines,
            0,
            0,
            ScoringLockEvidence::no_rotation(RotationState::Zero),
        )
        .with_perfect_clear(perfect_clear)
    }

    #[test]
    fn projection_math_is_checked_and_accounts_for_every_specialized_owner() {
        assert!(checked_score_cell_memory_projection_for_shape(usize::MAX, 2, 1, 0).is_none());

        let projection =
            ExactScoringExecutionMaterializer::checked_score_cell_memory_projection(&batch())
                .expect("checked score-cell projection");
        assert_eq!(projection.graph_count, 1);
        assert_eq!(projection.pattern_count, 1);
        assert_eq!(projection.max_path_len, 3);
        assert_eq!(projection.cell_capacity, 1);
        assert_eq!(
            projection.required_peak_bytes,
            projection.outer_storage_bytes
                + projection.trace_identity_storage_bytes
                + projection.path_scratch_bytes
                + projection.hold_scratch_bytes
                + projection.profile_storage_bytes
        );

        let error = ExactScoringExecutionMaterializer::materialize_score_cells_with_memory_limit(
            &batch(),
            ScoreObjectivePolicy::summary(),
            &ExecutionControl::default(),
            u128::MAX,
            u128::MAX,
        )
        .expect_err("retained-plus-projection overflow");
        assert_eq!(
            error,
            ExactScoreCellMaterializationError::ProjectionOverflow
        );
    }

    #[test]
    fn exact_cap_succeeds_and_one_byte_under_fails_before_materialization() {
        let batch = batch();
        let projection =
            ExactScoringExecutionMaterializer::checked_score_cell_memory_projection_for_policy(
                &batch,
                ScoreObjectivePolicy::summary(),
            )
            .expect("projection");
        let already_retained_bytes = 7;
        let exact_cap = projection.required_peak_bytes + already_retained_bytes;
        let (_, report) =
            ExactScoringExecutionMaterializer::materialize_score_cells_with_memory_limit(
                &batch,
                ScoreObjectivePolicy::summary(),
                &ExecutionControl::default(),
                already_retained_bytes,
                exact_cap,
            )
            .expect("exact cap");
        assert_eq!(report.projection, projection);
        assert!(report.retained_bytes <= projection.required_peak_bytes);

        let error = ExactScoringExecutionMaterializer::materialize_score_cells_with_memory_limit(
            &batch,
            ScoreObjectivePolicy::summary(),
            &ExecutionControl::default(),
            already_retained_bytes,
            exact_cap - 1,
        )
        .expect_err("one byte under");
        assert_eq!(
            error,
            ExactScoreCellMaterializationError::LimitExceeded {
                required_memory_bytes: exact_cap,
                max_memory_bytes: exact_cap - 1,
            }
        );
    }

    #[test]
    fn prebuilt_profile_guard_reuses_authority_at_exact_cap_and_rejects_one_under() {
        let batch = batch();
        let policy = ScoreObjectivePolicy::summary();
        let profile_projection =
            crate::checked_score_profile_memory_projection(policy).expect("profile projection");
        let (profile, profile_report) = crate::score_profile_with_memory_guard(
            policy,
            0,
            profile_projection.required_memory_bytes,
        )
        .expect("guarded profile");
        let projection = ExactScoringExecutionMaterializer::
            checked_score_cell_memory_projection_with_profile_bytes(
                &batch,
                profile_report.retained_bytes,
            )
            .expect("prebuilt profile projection");
        let (compatibility, _) =
            ExactScoringExecutionMaterializer::materialize_score_cells_with_memory_limit(
                &batch,
                policy,
                &ExecutionControl::default(),
                0,
                projection.required_peak_bytes,
            )
            .expect("compatibility guarded path");
        let (prebuilt, report) = ExactScoringExecutionMaterializer::
            materialize_score_cells_with_profile_and_memory_limit(
                &batch,
                policy,
                &profile,
                profile_report.retained_bytes,
                &ExecutionControl::default(),
                0,
                projection.required_peak_bytes,
            )
            .expect("prebuilt guarded path");
        let values = |materialized: &ExactScoreCellMaterialization| {
            materialized
                .scored_executions()
                .iter()
                .map(|execution| {
                    (
                        execution.candidate_identity(),
                        execution.pattern_id(),
                        execution.trace_identity().to_owned(),
                        execution.score(),
                        execution.attack(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(values(&prebuilt), values(&compatibility));
        assert_eq!(prebuilt.complete(), compatibility.complete());
        assert_eq!(report.projection, projection);

        let error = ExactScoringExecutionMaterializer::
            materialize_score_cells_with_profile_and_memory_limit(
                &batch,
                policy,
                &profile,
                profile_report.retained_bytes,
                &ExecutionControl::default(),
                0,
                projection.required_peak_bytes - 1,
            )
            .expect_err("one byte under prebuilt profile cap");
        assert_eq!(
            error,
            ExactScoreCellMaterializationError::LimitExceeded {
                required_memory_bytes: projection.required_peak_bytes,
                max_memory_bytes: projection.required_peak_bytes - 1,
            }
        );
    }

    #[test]
    fn specialized_materialization_distinguishes_cancellation() {
        let batch = batch();
        let projection =
            ExactScoringExecutionMaterializer::checked_score_cell_memory_projection_for_policy(
                &batch,
                ScoreObjectivePolicy::summary(),
            )
            .expect("projection");
        let cancellation = ExecutionCancellationToken::new();
        cancellation.handle().cancel();
        let error = ExactScoringExecutionMaterializer::materialize_score_cells_with_memory_limit(
            &batch,
            ScoreObjectivePolicy::summary(),
            &ExecutionControl::new(cancellation),
            0,
            projection.required_peak_bytes,
        )
        .expect_err("cancelled");
        assert_eq!(error, ExactScoreCellMaterializationError::Cancelled);
    }

    #[test]
    fn score_cell_path_keeps_score_only_maxima_without_aggregates_or_replays() {
        let batch = batch();
        let policy = ScoreObjectivePolicy::summary();
        let legacy = ExactScoringExecutionMaterializer::materialize_score_cells(
            &batch,
            policy,
            &ExecutionControl::default(),
        )
        .expect("existing score-cell materialization");
        assert!(legacy.aggregates().is_empty());
        let projection =
            ExactScoringExecutionMaterializer::checked_score_cell_memory_projection_for_policy(
                &batch, policy,
            )
            .expect("projection");
        let (specialized, report) =
            ExactScoringExecutionMaterializer::materialize_score_cells_with_memory_limit(
                &batch,
                policy,
                &ExecutionControl::default(),
                0,
                projection.required_peak_bytes,
            )
            .expect("specialized score cells");

        let legacy_values = legacy
            .scored_executions()
            .iter()
            .map(|execution| {
                (
                    execution.candidate_identity(),
                    execution.pattern_id(),
                    execution.score(),
                    execution.attack(),
                )
            })
            .collect::<Vec<_>>();
        let specialized_values = specialized
            .scored_executions()
            .iter()
            .map(|execution| {
                (
                    execution.candidate_identity(),
                    execution.pattern_id(),
                    execution.score(),
                    execution.attack(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(specialized_values, legacy_values);
        assert!(specialized.complete());
        assert_eq!(specialized.scored_executions().len(), 1);
        let trace_identity = specialized.scored_executions()[0]
            .trace_identity()
            .to_owned();
        assert_eq!(trace_identity.len(), COMPACT_SCORE_CELL_TRACE_ID_BYTES);
        assert_eq!(
            trace_identity,
            "score-cell-v1:0000000000000001:00000000000000000000000000000000"
        );
        assert!(!trace_identity.starts_with("ctk"));
        assert!(report.retained_bytes <= report.projection.required_peak_bytes);

        let [execution] = specialized
            .into_scored_executions()
            .try_into()
            .expect("one cell");
        let (_, pattern_id, moved_identity, _, _) = execution.into_parts();
        assert_eq!(pattern_id, 0);
        assert_eq!(moved_identity, trace_identity);
    }

    #[test]
    fn compact_trace_identity_bound_covers_maximum_identifiers_exactly() {
        let identity = compact_score_cell_trace_identity(u64::MAX, usize::MAX)
            .expect("fixed compact identity");
        assert_eq!(identity.len(), COMPACT_SCORE_CELL_TRACE_ID_BYTES);
        assert!(identity.capacity() >= COMPACT_SCORE_CELL_TRACE_ID_BYTES);
        let mut components = identity.split(':');
        assert_eq!(components.next(), Some("score-cell-v1"));
        assert_eq!(components.next(), Some("ffffffffffffffff"));
        let pattern_component = components.next().expect("pattern component");
        assert_eq!(pattern_component.len(), 32);
        assert!(pattern_component.ends_with(&format!("{:x}", usize::MAX)));
        assert_eq!(components.next(), None);
    }
}

#[cfg(test)]
mod complete_replay_materialization_tests {
    use clearra_core_domain::{
        execution_cancellation::ExecutionControl,
        piece::{piece_kind::PieceKind, rotation::RotationState},
        solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
    };
    use clearra_geometry::layout::board64_layout::Board64Layout;
    use clearra_replay::{
        ExactScoringExecutionBatch, ExactScoringExecutionGraph, ScoringExecutionEdge,
        ScoringExecutionNode, ScoringLockEvidence,
    };

    use super::*;

    fn replay_collision_batch() -> ExactScoringExecutionBatch {
        let identity = StandardBoard64TilingIdentity::from_placements(0, [])
            .expect("empty candidate identity");
        let edges = vec![
            o_edge(1, 0, 0, 0, false),
            o_edge(2, 1, 2, 0, false),
            o_edge(3, 1, 2, 0, false),
            o_edge(3, 0, 0, 0, false),
            o_edge(4, 2, 4, 0, false),
            o_edge(5, 3, 6, 0, false),
            o_edge(6, 4, 8, 2, true),
        ];
        ExactScoringExecutionBatch::new(
            Board64Layout::standard_10_by_lines(2).expect("layout"),
            0,
            vec![vec![PieceKind::O; 5]],
            0,
            None,
            false,
            false,
            false,
            1,
            1,
            vec![ExactScoringExecutionGraph::new(
                9,
                identity,
                0,
                vec![
                    ScoringExecutionNode::new(0, 2, false),
                    ScoringExecutionNode::new(2, 1, false),
                    ScoringExecutionNode::new(3, 1, false),
                    ScoringExecutionNode::new(4, 1, false),
                    ScoringExecutionNode::new(5, 1, false),
                    ScoringExecutionNode::new(6, 1, false),
                    ScoringExecutionNode::new(7, 0, true),
                ],
                edges,
            )],
            true,
        )
    }

    fn o_edge(
        to: u32,
        operation_index: u8,
        x: i8,
        cleared_lines: u8,
        perfect_clear: bool,
    ) -> ScoringExecutionEdge {
        ScoringExecutionEdge::new(
            to,
            operation_index,
            PieceKind::O,
            RotationState::Zero,
            x,
            0,
            cleared_lines,
            0,
            0,
            ScoringLockEvidence::no_rotation(RotationState::Zero),
        )
        .with_perfect_clear(perfect_clear)
    }

    #[test]
    fn complete_replay_keeps_both_paths_that_merge_at_one_terminal_supply_state() {
        let batch = replay_collision_batch();
        let control = ExecutionControl::default();
        let representative =
            ExactScoringExecutionMaterializer::materialize_terminal_replays(&batch, &control)
                .expect("ordinary save representative");
        assert_eq!(representative.aggregates()[0].executions().len(), 1);

        let (complete, report) =
            ExactScoringExecutionMaterializer::materialize_complete_replays_with_limits(
                &batch,
                &control,
                ExactReplayMaterializationLimits::new(8, 8, 1024 * 1024),
            )
            .expect("complete replay family");
        let executions = complete.aggregates()[0].executions();
        assert!(complete.complete());
        assert_eq!(report.execution_count(), 2);
        assert_eq!(executions.len(), 2);
        assert_ne!(
            executions[0].trace_identity(),
            executions[1].trace_identity()
        );
        assert!(executions
            .windows(2)
            .all(|pair| { pair[0].trace_identity() < pair[1].trace_identity() }));
        for execution in executions {
            let steps = execution.replay_trace().solution_trace().steps();
            assert_eq!(steps.len(), 5);
            let terminal = steps.last().expect("terminal lock");
            assert_eq!(terminal.piece_decision().output_cursor(), 5);
            assert_eq!(terminal.piece_decision().output_hold_piece(), None);
            assert_eq!(terminal.line_clear().cleared_lines(), 2);
            assert_eq!(terminal.board_after().after_line_clear().occupied(), 0);
        }
    }

    #[test]
    fn complete_replay_cell_peak_is_checked_and_internal_progress_is_suppressed() {
        use std::sync::{Arc, Mutex};
        struct Events(Mutex<Vec<&'static str>>);
        impl clearra_core_domain::execution_cancellation::ProgressSink for Events {
            fn report(
                &self,
                progress: clearra_core_domain::execution_cancellation::ExecutionProgress,
            ) {
                self.0.lock().unwrap().push(progress.stage);
            }
        }
        let events = Arc::new(Events(Mutex::new(Vec::new())));
        let control = ExecutionControl::default().with_progress_sink(events.clone());
        let batch = replay_collision_batch();
        let (_, report) =
            ExactScoringExecutionMaterializer::materialize_complete_replay_cell_with_limits(
                &batch,
                0,
                0,
                &control,
                ExactReplayMaterializationLimits::new(8, 8, 1024 * 1024),
            )
            .unwrap();
        assert_eq!(report.raw_execution_count(), 2);
        assert_eq!(report.execution_count(), 2);
        assert!(report.admitted_peak_bytes() >= report.retained_bytes());
        assert!(
            events.0.lock().unwrap().is_empty(),
            "outer cell scanner owns progress cadence"
        );
        let error =
            ExactScoringExecutionMaterializer::materialize_complete_replay_cell_with_limits(
                &batch,
                0,
                0,
                &control,
                ExactReplayMaterializationLimits::new(8, 8, report.admitted_peak_bytes() - 1),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            ExactReplayMaterializationError::MemoryLimitExceeded { .. }
        ));
    }

    #[test]
    fn exhaustive_limits_abort_instead_of_returning_a_truncated_complete_family() {
        let error = ExactScoringExecutionMaterializer::materialize_complete_replays_with_limits(
            &replay_collision_batch(),
            &ExecutionControl::default(),
            ExactReplayMaterializationLimits::new(1, 8, 1024 * 1024),
        )
        .expect_err("second witness exceeds the explicit family limit");
        assert_eq!(
            error,
            ExactReplayMaterializationError::ExecutionLimitExceeded { max_executions: 1 }
        );

        let error = ExactScoringExecutionMaterializer::materialize_complete_replays_with_limits(
            &replay_collision_batch(),
            &ExecutionControl::default(),
            ExactReplayMaterializationLimits::new(8, 8, 1),
        )
        .expect_err("one byte cannot retain a replay family");
        assert!(matches!(
            error,
            ExactReplayMaterializationError::MemoryLimitExceeded { .. }
        ));
    }
}
