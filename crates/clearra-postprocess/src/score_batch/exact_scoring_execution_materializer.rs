use std::collections::BTreeSet;

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
            return output;
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

    #[cfg(feature = "stage-profiling")]
    pub const fn profile(&self) -> Option<ExactScoringExecutionProfile> {
        self.profile
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactScoringExecutionCancelled;

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
}

#[derive(Clone, Debug)]
struct BestExecution {
    score: u64,
    attack: u32,
    trace_identity: String,
    retained_path: Option<Vec<ScoringExecutionEdge>>,
    retained_holds: Option<Vec<HoldDecision>>,
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
                    evaluation_policy,
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
    evaluation_policy: ScoreEvaluationPolicy,
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
            let could_improve = best.as_ref().is_none_or(|current| {
                score > current.score || (score == current.score && attack >= current.attack)
            });
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
                        || (candidate.score == current.score && candidate.attack > current.attack)
                        || (candidate.score == current.score
                            && candidate.attack == current.attack
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
            && first_standard_bag_lookahead(sequence).is_none()
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
                evaluation_policy,
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
                evaluation_policy,
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

fn replay_path(
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
