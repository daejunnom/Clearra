//! Full-height adapter to the common exact supply traversal and score model.
//! A complete colored family is still Core/App authority, not a property of
//! a public batch constructor or one replay witness.
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
    solution::ExtendedTilingSolutionKey,
};
use clearra_objectives::policy::score_objective_policy::ScoreObjectivePolicy;
use clearra_replay::{
    FullHeightExecutionBatch, FullHeightReplayBuildError, FullHeightReplayError,
    FullHeightReplayProjector, FullHeightReplayTrace, HoldDecision, ScoringExecutionEdge,
};
use clearra_scoring::{
    model::{ScoreEvaluationPolicy, ScoreModelEvaluator},
    state::ScoreState,
};
use std::fmt::Write;

use super::{
    exact_scoring_execution_materializer::{
        compact_score_cell_trace_identity, COMPACT_SCORE_CELL_TRACE_ID_BYTES,
    },
    execution_supply::SupplyState,
    score_cell_traversal::{
        visit_score_cell_paths, ScoreCellPhysicalProjection, ScoreCellTraversalError,
    },
    ScoreCell,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FullHeightScoreCellError {
    Cancelled,
    InvalidEvidence,
    ProjectionOverflow,
    AllocationFailed,
    MemoryLimitExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FullHeightScoreCellMemoryProjection {
    pub candidate_count: usize,
    pub pattern_count: usize,
    pub cell_capacity: usize,
    pub max_path_len: usize,
    pub cell_storage_bytes: u128,
    pub scratch_bytes: u128,
    pub profile_storage_bytes: u128,
    pub required_peak_bytes: u128,
}

#[derive(Debug, Eq, PartialEq)]
pub struct FullHeightScoreCellMaterialization {
    cells: Vec<ScoreCell>,
    complete: bool,
    admitted_peak_bytes: u128,
}

impl FullHeightScoreCellMaterialization {
    pub fn cells(&self) -> &[ScoreCell] {
        &self.cells
    }
    pub fn into_cells(self) -> Vec<ScoreCell> {
        self.cells
    }
    pub const fn complete(&self) -> bool {
        self.complete
    }
    pub const fn admitted_peak_bytes(&self) -> u128 {
        self.admitted_peak_bytes
    }
    pub fn checked_retained_bytes(&self) -> Option<u128> {
        let inline = (self.cells.capacity() as u128)
            .checked_mul(core::mem::size_of::<ScoreCell>() as u128)?;
        self.cells.iter().try_fold(inline, |bytes, cell| {
            bytes.checked_add(cell.checked_string_retained_bytes()?)
        })
    }
}

#[derive(Clone, Copy)]
struct PhysicalState {
    board: Board256Mask,
    operations: u64,
}

struct PhysicalProjection {
    height: u8,
    required_operations: u64,
}
impl ScoreCellPhysicalProjection for PhysicalProjection {
    type State = PhysicalState;
    fn advance(
        &self,
        state: PhysicalState,
        edge: ScoringExecutionEdge,
    ) -> Result<PhysicalState, ScoreCellTraversalError> {
        let bit = 1_u64
            .checked_shl(u32::from(edge.operation_index()))
            .ok_or(ScoreCellTraversalError::InvalidEvidence)?;
        if bit & self.required_operations == 0 || bit & state.operations != 0 {
            return Err(ScoreCellTraversalError::InvalidEvidence);
        }
        let transition =
            FullHeightReplayProjector::project_scoring_step(self.height, state.board, edge)
                .map_err(|_| ScoreCellTraversalError::InvalidEvidence)?;
        Ok(PhysicalState {
            board: transition.after_line_clear(),
            operations: state.operations | bit,
        })
    }
    fn validate_terminal(&self, state: PhysicalState) -> Result<(), ScoreCellTraversalError> {
        if !state.board.is_empty() || state.operations != self.required_operations {
            return Err(ScoreCellTraversalError::InvalidEvidence);
        }
        Ok(())
    }
}

struct BestExecution {
    score: u64,
    attack: u32,
    canonical_trace: String,
}

pub struct FullHeightScoreCellMaterializer;
impl FullHeightScoreCellMaterializer {
    pub fn checked_memory_projection(
        batch: &FullHeightExecutionBatch,
        policy: ScoreObjectivePolicy,
    ) -> Option<FullHeightScoreCellMemoryProjection> {
        let (candidate_count, max_path_len) = validated_shape(batch).ok()?;
        let pattern_count = batch.execution().patterns().len();
        let cell_capacity = candidate_count.checked_mul(pattern_count)?;
        let cell_storage_bytes = (cell_capacity as u128).checked_mul(
            (core::mem::size_of::<ScoreCell>() + COMPACT_SCORE_CELL_TRACE_ID_BYTES) as u128,
        )?;
        let scratch_bytes = (max_path_len as u128)
            .checked_mul(
                (core::mem::size_of::<ScoringExecutionEdge>()
                    + core::mem::size_of::<HoldDecision>()
                    + core::mem::size_of::<(ScoringExecutionEdge, HoldDecision)>())
                    as u128,
            )?
            .checked_add(FullHeightReplayTrace::checked_step_buffer_bytes(
                max_path_len,
            )?)?
            // Current best and the next exact-score tie key coexist. These
            // keys are scratch only; output cells use the existing fixed ID.
            .checked_add(2 * canonical_key_capacity(max_path_len)? as u128)?;
        let profile_storage_bytes =
            crate::checked_score_profile_memory_projection(policy)?.required_memory_bytes;
        Some(FullHeightScoreCellMemoryProjection {
            candidate_count,
            pattern_count,
            cell_capacity,
            max_path_len,
            cell_storage_bytes,
            scratch_bytes,
            profile_storage_bytes,
            required_peak_bytes: cell_storage_bytes
                .checked_add(scratch_bytes)?
                .checked_add(profile_storage_bytes)?,
        })
    }

    pub fn materialize_with_memory_limit(
        batch: &FullHeightExecutionBatch,
        policy: ScoreObjectivePolicy,
        control: &ExecutionControl,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<FullHeightScoreCellMaterialization, FullHeightScoreCellError> {
        if control.is_cancelled() {
            return Err(FullHeightScoreCellError::Cancelled);
        }
        // checked_memory_projection is allocation-free. A malformed graph
        // family is invalid evidence, not a resource limit or negative proof.
        validated_shape(batch)?;
        let projection = Self::checked_memory_projection(batch, policy)
            .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
        let mut peak = already_retained_bytes
            .checked_add(projection.required_peak_bytes)
            .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
        ensure_limit(peak, max_memory_bytes)?;
        let (profile, report) = crate::score_profile_with_memory_guard(
            policy,
            already_retained_bytes,
            max_memory_bytes,
        )
        .map_err(|error| match error {
            crate::ScoreProfileMemoryGuardError::AllocationFailed => {
                FullHeightScoreCellError::AllocationFailed
            }
            crate::ScoreProfileMemoryGuardError::ProjectionOverflow => {
                FullHeightScoreCellError::ProjectionOverflow
            }
            crate::ScoreProfileMemoryGuardError::LimitExceeded {
                required_memory_bytes,
                max_memory_bytes,
            } => FullHeightScoreCellError::MemoryLimitExceeded {
                required_memory_bytes,
                max_memory_bytes,
            },
        })?;
        // Check allocator-visible slack after each reserve, before another
        // owner is allocated. Static projection never substitutes for capacity.
        peak = peak
            .checked_add(
                report
                    .retained_bytes
                    .saturating_sub(projection.profile_storage_bytes),
            )
            .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
        ensure_limit(peak, max_memory_bytes)?;
        let mut cells = Vec::new();
        let mut path = Vec::new();
        let mut holds = Vec::new();
        let mut selected_path = Vec::new();
        reserve_scratch(
            &mut cells,
            projection.cell_capacity,
            &mut peak,
            max_memory_bytes,
        )?;
        reserve_scratch(
            &mut path,
            projection.max_path_len,
            &mut peak,
            max_memory_bytes,
        )?;
        reserve_scratch(
            &mut holds,
            projection.max_path_len,
            &mut peak,
            max_memory_bytes,
        )?;
        reserve_scratch(
            &mut selected_path,
            projection.max_path_len,
            &mut peak,
            max_memory_bytes,
        )?;
        let execution = batch.execution();
        let graphs = execution.graphs();
        let mut start = 0;
        let mut complete = execution.complete();
        let mut scratch_slack_peak = 0_u128;
        while start < graphs.len() {
            let id = graphs[start].candidate_id();
            let end = start
                + graphs[start..]
                    .iter()
                    .take_while(|graph| graph.candidate_id() == id)
                    .count();
            control.report_progress(
                "score-cell-execution",
                id - 1,
                Some(projection.candidate_count as u64),
            );
            for (pattern_id, sequence) in execution.patterns().iter().enumerate() {
                let mut best: Option<BestExecution> = None;
                for graph in &graphs[start..end] {
                    let identity =
                        ExtendedTilingSolutionKey::parse_canonical(graph.candidate_key())
                            .map_err(|_| FullHeightScoreCellError::InvalidEvidence)?;
                    let physical = PhysicalProjection {
                        height: batch.height(),
                        required_operations: (1_u64 << identity.placement_count()) - 1,
                    };
                    path.clear();
                    holds.clear();
                    let mut terminal_error = None;
                    let traversal = visit_score_cell_paths(
                        execution,
                        graph,
                        sequence,
                        SupplyState {
                            node: graph.root(),
                            cursor: execution.initial_cursor(),
                            hold: execution.initial_hold(),
                        },
                        PhysicalState {
                            board: batch.initial(),
                            operations: 0,
                        },
                        &physical,
                        &mut path,
                        &mut holds,
                        projection.max_path_len,
                        &profile,
                        ScoreModelEvaluator::initial_state(ScoreEvaluationPolicy::tetrio_pc(
                            policy.initial_b2b(),
                        )),
                        &mut |path, holds, score: ScoreState| {
                            if best.as_ref().is_some_and(|best| score.score() < best.score) {
                                return Ok(());
                            }
                            let candidate = make_best(
                                batch,
                                path,
                                holds,
                                score,
                                &mut selected_path,
                                projection.max_path_len,
                                &mut peak,
                                &mut scratch_slack_peak,
                                max_memory_bytes,
                                control,
                            )
                            .map_err(|error| {
                                terminal_error = Some(error);
                                ScoreCellTraversalError::InvalidEvidence
                            })?;
                            if best.as_ref().is_none_or(|current| {
                                candidate.score > current.score
                                    || (candidate.score == current.score
                                        && candidate.canonical_trace < current.canonical_trace)
                            }) {
                                best = Some(candidate);
                            }
                            Ok(())
                        },
                        control,
                    );
                    if let Some(error) = terminal_error {
                        return Err(error);
                    }
                    complete &= traversal.map_err(|error| match error {
                        ScoreCellTraversalError::Cancelled => FullHeightScoreCellError::Cancelled,
                        ScoreCellTraversalError::InvalidEvidence
                        | ScoreCellTraversalError::ScratchCapacity => {
                            FullHeightScoreCellError::InvalidEvidence
                        }
                    })?;
                }
                if let Some(best) = best {
                    let trace = compact_score_cell_trace_identity(id, pattern_id)
                        .map_err(|_| FullHeightScoreCellError::AllocationFailed)?;
                    peak = peak
                        .checked_add(
                            trace
                                .capacity()
                                .saturating_sub(COMPACT_SCORE_CELL_TRACE_ID_BYTES)
                                as u128,
                        )
                        .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
                    ensure_limit(peak, max_memory_bytes)?;
                    if cells.len() >= projection.cell_capacity {
                        return Err(FullHeightScoreCellError::ProjectionOverflow);
                    }
                    cells.push(ScoreCell::new_with_static_accuracy(
                        id,
                        pattern_id,
                        trace,
                        best.score,
                        best.attack,
                        profile.accuracy_level().as_str(),
                    ));
                }
            }
            start = end;
        }
        control.report_progress(
            "score-cell-execution",
            projection.candidate_count as u64,
            Some(projection.candidate_count as u64),
        );
        Ok(FullHeightScoreCellMaterialization {
            cells,
            complete,
            admitted_peak_bytes: peak,
        })
    }
}

fn validated_shape(
    batch: &FullHeightExecutionBatch,
) -> Result<(usize, usize), FullHeightScoreCellError> {
    let mut previous: Option<(&str, u64)> = None;
    let mut candidate_count = 0_usize;
    let mut max_path_len = 0;
    for graph in batch.execution().graphs() {
        let identity = ExtendedTilingSolutionKey::parse_canonical(graph.candidate_key())
            .map_err(|_| FullHeightScoreCellError::InvalidEvidence)?;
        let count = identity.placement_count();
        if count == 0
            || count > 60
            || identity.height() != batch.height()
            || identity.initial_board() != batch.initial()
        {
            return Err(FullHeightScoreCellError::InvalidEvidence);
        }
        max_path_len = max_path_len.max(count);
        if previous.is_none_or(|(key, _)| key != graph.candidate_key()) {
            if previous.is_some_and(|(key, _)| key >= graph.candidate_key()) {
                return Err(FullHeightScoreCellError::InvalidEvidence);
            }
            candidate_count += 1;
            if graph.candidate_id() != candidate_count as u64 {
                return Err(FullHeightScoreCellError::InvalidEvidence);
            }
        } else if previous.is_some_and(|(_, id)| id != graph.candidate_id()) {
            return Err(FullHeightScoreCellError::InvalidEvidence);
        }
        previous = Some((graph.candidate_key(), graph.candidate_id()));
    }
    Ok((candidate_count, max_path_len))
}

#[allow(clippy::too_many_arguments)]
fn make_best(
    batch: &FullHeightExecutionBatch,
    path: &[ScoringExecutionEdge],
    holds: &[HoldDecision],
    score: ScoreState,
    selected_path: &mut Vec<(ScoringExecutionEdge, HoldDecision)>,
    max_path_len: usize,
    peak: &mut u128,
    scratch_slack_peak: &mut u128,
    max_memory_bytes: u128,
    control: &ExecutionControl,
) -> Result<BestExecution, FullHeightScoreCellError> {
    if path.len() != holds.len() || path.len() > selected_path.capacity() {
        return Err(FullHeightScoreCellError::InvalidEvidence);
    }
    selected_path.clear();
    selected_path.extend(path.iter().copied().zip(holds.iter().copied()));
    let step_credit = FullHeightReplayTrace::checked_step_buffer_bytes(max_path_len)
        .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
    let mut trace_slack = 0_u128;
    let trace = FullHeightReplayTrace::from_selected_path(
        batch.height(),
        batch.initial(),
        usize::from(batch.execution().initial_cursor()),
        batch.execution().initial_hold(),
        selected_path,
        control,
        |bytes| {
            trace_slack = trace_slack.max(bytes.saturating_sub(step_credit));
            ensure_limit(
                peak.checked_add(bytes.saturating_sub(step_credit))
                    .ok_or(FullHeightScoreCellError::ProjectionOverflow)?,
                max_memory_bytes,
            )
        },
    )
    .map_err(|error| match error {
        FullHeightReplayBuildError::MemoryGuard(error) => error,
        FullHeightReplayBuildError::Replay(FullHeightReplayError::Cancelled) => {
            FullHeightScoreCellError::Cancelled
        }
        FullHeightReplayBuildError::Replay(FullHeightReplayError::AllocationFailed) => {
            FullHeightScoreCellError::AllocationFailed
        }
        FullHeightReplayBuildError::Replay(_) => FullHeightScoreCellError::InvalidEvidence,
    })?;
    let capacity =
        canonical_key_capacity(max_path_len).ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
    let mut key = String::new();
    key.try_reserve_exact(capacity)
        .map_err(|_| FullHeightScoreCellError::AllocationFailed)?;
    let scratch_slack = (key.capacity().saturating_sub(capacity) as u128)
        .checked_mul(2)
        .and_then(|bytes| bytes.checked_add(trace_slack))
        .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
    // Temporary trace/key buffers are dropped and reused. Their allocation
    // slack contributes its maximum, never a sum over every score tie.
    if scratch_slack > *scratch_slack_peak {
        *peak = peak
            .checked_add(scratch_slack - *scratch_slack_peak)
            .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
        *scratch_slack_peak = scratch_slack;
    }
    ensure_limit(*peak, max_memory_bytes)?;
    trace
        .write_canonical_key(&mut key)
        .map_err(|_| FullHeightScoreCellError::InvalidEvidence)?;
    // The physical replay key omits score evidence by design. Add a typed
    // internal witness suffix so equal physical poses with different legal
    // last-action/spin evidence do not depend on worker completion order.
    // No score or attack value participates in this canonical tiebreaker.
    for edge in path {
        let evidence = edge.lock_evidence();
        let (previous_x, previous_y) = evidence.predecessor();
        let rotation_request = match evidence.rotation_request() {
            clearra_replay::RotationRequest::None => 0,
            clearra_replay::RotationRequest::Clockwise => 1,
            clearra_replay::RotationRequest::CounterClockwise => 2,
            clearra_replay::RotationRequest::HalfTurn => 3,
        };
        write!(
            key,
            "#r{}q{}k{}d{}:{}p{}:{}f{}i{}c{}:{}",
            evidence.from_rotation().quarter_turns(),
            rotation_request,
            evidence.kick_index(),
            evidence.kick_dx(),
            evidence.kick_dy(),
            previous_x,
            previous_y,
            u8::from(evidence.first_success_confirmed()),
            u8::from(evidence.immobile_before_clear()),
            edge.blocked_t_corners(),
            edge.blocked_t_front_corners(),
        )
        .map_err(|_| FullHeightScoreCellError::InvalidEvidence)?;
    }
    if key.len() > capacity {
        return Err(FullHeightScoreCellError::ProjectionOverflow);
    }
    Ok(BestExecution {
        score: score.score(),
        attack: score.attack(),
        canonical_trace: key,
    })
}

fn canonical_key_capacity(max_path_len: usize) -> Option<usize> {
    max_path_len.checked_mul(256)?.checked_add(256)
}
fn ensure_limit(
    required_memory_bytes: u128,
    max_memory_bytes: u128,
) -> Result<(), FullHeightScoreCellError> {
    if required_memory_bytes > max_memory_bytes {
        Err(FullHeightScoreCellError::MemoryLimitExceeded {
            required_memory_bytes,
            max_memory_bytes,
        })
    } else {
        Ok(())
    }
}
fn reserve_scratch<T>(
    buffer: &mut Vec<T>,
    requested: usize,
    peak: &mut u128,
    max: u128,
) -> Result<(), FullHeightScoreCellError> {
    buffer
        .try_reserve_exact(requested)
        .map_err(|_| FullHeightScoreCellError::AllocationFailed)?;
    let slack = (buffer.capacity().saturating_sub(requested) as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
    *peak = peak
        .checked_add(slack)
        .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
    ensure_limit(*peak, max)
}

#[cfg(test)]
#[path = "full_height_score_cell_tests.rs"]
mod tests;
