// SRP rationale: this module has one change reason: canonical score-matrix representation and queries.
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_objectives::max_score::{MaterializedScoreCell, MaterializedScoreMatrix};
use clearra_replay::ReplayEvent;
use clearra_scoring::{
    model::{ScoreEvaluationPolicy, ScoreModelEvaluator},
    profile::{ScoreProfile, ScoringAccuracyLevel, TraceRequirement},
};

use super::candidate_execution_aggregate::CandidateExecutionAggregate;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreCell {
    candidate_id: u64,
    pattern_id: usize,
    trace_identity: String,
    score: u64,
    attack: u32,
    accuracy_level: Cow<'static, str>,
}

impl ScoreCell {
    pub fn new(
        candidate_id: u64,
        pattern_id: usize,
        trace_identity: impl Into<String>,
        score: u64,
        attack: u32,
        accuracy_level: impl Into<String>,
    ) -> Self {
        Self {
            candidate_id,
            pattern_id,
            trace_identity: trace_identity.into(),
            score,
            attack,
            accuracy_level: Cow::Owned(accuracy_level.into()),
        }
    }

    pub fn new_with_static_accuracy(
        candidate_id: u64,
        pattern_id: usize,
        trace_identity: impl Into<String>,
        score: u64,
        attack: u32,
        accuracy_level: &'static str,
    ) -> Self {
        Self {
            candidate_id,
            pattern_id,
            trace_identity: trace_identity.into(),
            score,
            attack,
            accuracy_level: Cow::Borrowed(accuracy_level),
        }
    }

    pub fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub fn pattern_id(&self) -> usize {
        self.pattern_id
    }

    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }

    pub fn score(&self) -> u64 {
        self.score
    }

    pub fn attack(&self) -> u32 {
        self.attack
    }

    pub fn accuracy_level(&self) -> &str {
        self.accuracy_level.as_ref()
    }

    fn checked_string_retained_bytes(&self) -> Option<u128> {
        let accuracy_bytes = match &self.accuracy_level {
            Cow::Borrowed(_) => 0,
            Cow::Owned(value) => value.capacity() as u128,
        };
        (self.trace_identity.capacity() as u128).checked_add(accuracy_bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreMatrix {
    cells: Vec<ScoreCell>,
    pattern_count: usize,
    profile_id: String,
    accuracy_level: String,
    materialized: bool,
    complete: bool,
    incomplete_reason: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreMatrixMemoryProjection {
    pub cell_capacity: usize,
    pub cell_outer_storage_bytes: u128,
    pub cell_string_storage_bytes: u128,
    pub profile_id_storage_bytes: u128,
    pub accuracy_level_storage_bytes: u128,
    pub required_peak_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreMatrixMemoryReport {
    pub projection: ScoreMatrixMemoryProjection,
    pub retained_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoreMatrixMemoryGuardError {
    ProjectionOverflow,
    LimitExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    AllocationFailed,
}

impl ScoreMatrix {
    pub fn from_materialized_cells(
        cells: Vec<ScoreCell>,
        profile: &ScoreProfile,
        pattern_count: usize,
        source_complete: bool,
    ) -> Self {
        let profile_id = profile.id().to_owned();
        let accuracy_level = profile.accuracy_level().as_str().to_owned();
        Self::finish_materialized_cells(
            cells,
            profile,
            pattern_count,
            source_complete,
            profile_id,
            accuracy_level,
        )
    }

    pub fn checked_materialized_cells_memory_projection(
        cells: &[ScoreCell],
        cell_capacity: usize,
        profile: &ScoreProfile,
    ) -> Option<ScoreMatrixMemoryProjection> {
        (cell_capacity >= cells.len()).then_some(())?;
        let cell_outer_storage_bytes =
            (cell_capacity as u128).checked_mul(core::mem::size_of::<ScoreCell>() as u128)?;
        let cell_string_storage_bytes = cells.iter().try_fold(0_u128, |total, cell| {
            total.checked_add(cell.checked_string_retained_bytes()?)
        })?;
        let profile_id_storage_bytes = profile.id().len() as u128;
        let accuracy_level_storage_bytes = profile.accuracy_level().as_str().len() as u128;
        let required_peak_bytes = cell_outer_storage_bytes
            .checked_add(cell_string_storage_bytes)?
            .checked_add(profile_id_storage_bytes)?
            .checked_add(accuracy_level_storage_bytes)?;
        Some(ScoreMatrixMemoryProjection {
            cell_capacity,
            cell_outer_storage_bytes,
            cell_string_storage_bytes,
            profile_id_storage_bytes,
            accuracy_level_storage_bytes,
            required_peak_bytes,
        })
    }

    /// Builds matrix metadata fallibly while charging the caller-owned cell
    /// buffer and its strings under the same total cap.
    #[allow(clippy::too_many_arguments)]
    pub fn from_materialized_cells_with_memory_guard(
        cells: Vec<ScoreCell>,
        profile: &ScoreProfile,
        pattern_count: usize,
        source_complete: bool,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<(Self, ScoreMatrixMemoryReport), ScoreMatrixMemoryGuardError> {
        let projection =
            Self::checked_materialized_cells_memory_projection(&cells, cells.capacity(), profile)
                .ok_or(ScoreMatrixMemoryGuardError::ProjectionOverflow)?;
        let required_memory_bytes = already_retained_bytes
            .checked_add(projection.required_peak_bytes)
            .ok_or(ScoreMatrixMemoryGuardError::ProjectionOverflow)?;
        if required_memory_bytes > max_memory_bytes {
            return Err(ScoreMatrixMemoryGuardError::LimitExceeded {
                required_memory_bytes,
                max_memory_bytes,
            });
        }

        let profile_id = try_owned_str(profile.id())?;
        let accuracy_level = try_owned_str(profile.accuracy_level().as_str())?;
        let matrix = Self::finish_materialized_cells(
            cells,
            profile,
            pattern_count,
            source_complete,
            profile_id,
            accuracy_level,
        );
        let retained_bytes = matrix
            .checked_retained_bytes()
            .ok_or(ScoreMatrixMemoryGuardError::ProjectionOverflow)?;
        let actual_required_memory_bytes = already_retained_bytes
            .checked_add(retained_bytes)
            .ok_or(ScoreMatrixMemoryGuardError::ProjectionOverflow)?;
        if actual_required_memory_bytes > max_memory_bytes {
            return Err(ScoreMatrixMemoryGuardError::LimitExceeded {
                required_memory_bytes: actual_required_memory_bytes,
                max_memory_bytes,
            });
        }
        Ok((
            matrix,
            ScoreMatrixMemoryReport {
                projection,
                retained_bytes,
            },
        ))
    }

    fn finish_materialized_cells(
        mut cells: Vec<ScoreCell>,
        profile: &ScoreProfile,
        pattern_count: usize,
        source_complete: bool,
        profile_id: String,
        accuracy_level: String,
    ) -> Self {
        let profile_supported = !matches!(
            profile.accuracy_level(),
            ScoringAccuracyLevel::Unsupported | ScoringAccuracyLevel::InsufficientTrace
        );
        let shape_valid = pattern_count > 0
            && cells.iter().all(|cell| {
                cell.candidate_id != 0
                    && cell.pattern_id < pattern_count
                    && cell.accuracy_level.as_ref() == profile.accuracy_level().as_str()
            });
        cells.sort_by(|left, right| {
            (
                left.pattern_id,
                left.candidate_id,
                left.trace_identity.as_str(),
            )
                .cmp(&(
                    right.pattern_id,
                    right.candidate_id,
                    right.trace_identity.as_str(),
                ))
        });
        let conflicting_cells = cells.windows(2).any(|pair| {
            pair[0].candidate_id == pair[1].candidate_id
                && pair[0].pattern_id == pair[1].pattern_id
                && pair[0].trace_identity == pair[1].trace_identity
                && (pair[0].score != pair[1].score || pair[0].attack != pair[1].attack)
        });
        cells.dedup_by(|left, right| {
            left.candidate_id == right.candidate_id
                && left.pattern_id == right.pattern_id
                && left.trace_identity == right.trace_identity
        });
        let complete = source_complete && profile_supported && shape_valid && !conflicting_cells;
        let incomplete_reason = if complete {
            None
        } else if !source_complete {
            Some("score_execution_source_incomplete")
        } else if pattern_count == 0 {
            Some("pattern_universe_not_materialized")
        } else if !profile_supported {
            Some("score_profile_not_supported")
        } else if conflicting_cells {
            Some("score_materialized_cell_conflict")
        } else {
            Some("score_materialized_cell_identity_invalid")
        };
        let materialized =
            score_matrix_materialized(pattern_count, source_complete, !cells.is_empty());
        Self {
            cells,
            pattern_count,
            profile_id,
            accuracy_level,
            materialized,
            complete,
            incomplete_reason,
        }
    }

    pub fn materialize(
        aggregates: &[CandidateExecutionAggregate],
        profile: &ScoreProfile,
        pattern_count: usize,
        source_complete: bool,
    ) -> Self {
        Self::materialize_with_control(
            aggregates,
            profile,
            pattern_count,
            source_complete,
            &ExecutionControl::default(),
        )
        .expect("default execution control cannot be cancelled")
    }

    pub fn materialize_with_control(
        aggregates: &[CandidateExecutionAggregate],
        profile: &ScoreProfile,
        pattern_count: usize,
        source_complete: bool,
        control: &ExecutionControl,
    ) -> Result<Self, ScoreMatrixCancelled> {
        Self::materialize_with_policy_and_control(
            aggregates,
            profile,
            ScoreEvaluationPolicy::profile_defaults(),
            pattern_count,
            source_complete,
            control,
        )
    }

    pub fn materialize_with_policy_and_control(
        aggregates: &[CandidateExecutionAggregate],
        profile: &ScoreProfile,
        evaluation_policy: ScoreEvaluationPolicy,
        pattern_count: usize,
        source_complete: bool,
        control: &ExecutionControl,
    ) -> Result<Self, ScoreMatrixCancelled> {
        let mut cells = Vec::new();
        let mut identities = BTreeSet::new();
        let mut incomplete_reason = None;
        let mut complete = source_complete
            && pattern_count > 0
            && !matches!(
                profile.accuracy_level(),
                ScoringAccuracyLevel::Unsupported | ScoringAccuracyLevel::InsufficientTrace
            );
        if !source_complete {
            incomplete_reason = Some(if aggregates.is_empty() {
                "score_matrix_not_materialized"
            } else {
                "score_execution_source_incomplete"
            });
        } else if pattern_count == 0 {
            incomplete_reason = Some("pattern_universe_not_materialized");
        } else if !complete {
            incomplete_reason = Some("score_profile_not_supported");
        }

        for (aggregate_index, aggregate) in aggregates.iter().enumerate() {
            if control.is_cancelled() {
                return Err(ScoreMatrixCancelled);
            }
            control.report_progress(
                "score-matrix",
                aggregate_index as u64,
                Some(aggregates.len() as u64),
            );
            for execution in aggregate.executions() {
                if control.is_cancelled() {
                    return Err(ScoreMatrixCancelled);
                }
                if execution.pattern_id() >= pattern_count {
                    complete = false;
                    incomplete_reason.get_or_insert("score_pattern_identity_out_of_range");
                    continue;
                }
                if !trace_satisfies_requirement(
                    execution.replay_trace().events(),
                    profile.trace_requirement(),
                ) {
                    complete = false;
                    incomplete_reason.get_or_insert("score_trace_requirement_not_met");
                    continue;
                }
                if !trace_has_complete_spin_evidence(execution.replay_trace(), profile) {
                    complete = false;
                    incomplete_reason.get_or_insert("score_spin_evidence_incomplete");
                    continue;
                }
                let identity = (
                    aggregate.candidate_id(),
                    execution.pattern_id(),
                    execution.trace_identity().to_owned(),
                );
                if !identities.insert(identity) {
                    continue;
                }
                let evaluation = ScoreModelEvaluator::evaluate_replay_trace_with_policy(
                    profile,
                    execution.replay_trace(),
                    evaluation_policy,
                );
                cells.push(ScoreCell {
                    candidate_id: aggregate.candidate_id(),
                    pattern_id: execution.pattern_id(),
                    trace_identity: execution.trace_identity().to_owned(),
                    score: evaluation.final_state().score(),
                    attack: evaluation.final_state().attack(),
                    accuracy_level: Cow::Borrowed(profile.accuracy_level().as_str()),
                });
            }
        }
        cells.sort_by(|left, right| {
            (
                left.pattern_id,
                left.candidate_id,
                left.trace_identity.as_str(),
            )
                .cmp(&(
                    right.pattern_id,
                    right.candidate_id,
                    right.trace_identity.as_str(),
                ))
        });

        control.report_progress(
            "score-matrix",
            aggregates.len() as u64,
            Some(aggregates.len() as u64),
        );
        let materialized =
            score_matrix_materialized(pattern_count, source_complete, !cells.is_empty());
        Ok(Self {
            cells,
            pattern_count,
            profile_id: profile.id().to_owned(),
            accuracy_level: profile.accuracy_level().as_str().to_owned(),
            materialized,
            complete,
            incomplete_reason: (!complete)
                .then_some(incomplete_reason.unwrap_or("score_matrix_incomplete")),
        })
    }

    pub fn unavailable(profile: &ScoreProfile, pattern_count: usize) -> Self {
        Self {
            cells: Vec::new(),
            pattern_count,
            profile_id: profile.id().to_owned(),
            accuracy_level: profile.accuracy_level().as_str().to_owned(),
            materialized: false,
            complete: false,
            incomplete_reason: Some("score_matrix_not_materialized"),
        }
    }

    pub fn cells(&self) -> &[ScoreCell] {
        &self.cells
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    pub fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }

    pub fn accuracy_level(&self) -> &str {
        &self.accuracy_level
    }

    pub fn complete(&self) -> bool {
        self.complete
    }

    pub fn materialized(&self) -> bool {
        self.materialized
    }

    pub fn exact(&self) -> bool {
        self.complete && self.accuracy_level == "profile-specific-exact"
    }

    pub fn incomplete_reason(&self) -> Option<&'static str> {
        self.incomplete_reason
    }

    /// Checked retained heap storage after score cells have been moved into
    /// the matrix. Static metadata such as `incomplete_reason` is not charged.
    pub fn checked_retained_bytes(&self) -> Option<u128> {
        let outer_storage_bytes = (self.cells.capacity() as u128)
            .checked_mul(core::mem::size_of::<ScoreCell>() as u128)?;
        let cell_string_bytes = self.cells.iter().try_fold(0_u128, |total, cell| {
            total.checked_add(cell.checked_string_retained_bytes()?)
        })?;
        outer_storage_bytes
            .checked_add(cell_string_bytes)?
            .checked_add(self.profile_id.capacity() as u128)?
            .checked_add(self.accuracy_level.capacity() as u128)
    }

    pub fn to_objective_matrix(&self) -> Option<MaterializedScoreMatrix> {
        let cells = self
            .cells
            .iter()
            .map(|cell| {
                Some(MaterializedScoreCell::new(
                    usize::try_from(cell.candidate_id).ok()?,
                    clearra_coverage::pattern::pattern_id::PatternId::new(cell.pattern_id),
                    cell.trace_identity.clone(),
                    cell.score,
                    cell.attack,
                    cell.accuracy_level.to_string(),
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(MaterializedScoreMatrix::new(
            self.pattern_count,
            cells,
            self.profile_id.clone(),
            self.accuracy_level.clone(),
            self.complete,
        ))
    }

    /// Each geometry candidate may have multiple legal movement traces for the
    /// same supply pattern. Scoring assigns that shape its highest legal trace
    /// before any cross-candidate objective is evaluated.
    pub fn highest_legal_cells_by_candidate_pattern(&self) -> Vec<&ScoreCell> {
        let mut cells = BTreeMap::<(u64, usize), &ScoreCell>::new();
        for cell in &self.cells {
            cells
                .entry((cell.candidate_id, cell.pattern_id))
                .and_modify(|current| {
                    if score_cell_order(cell, current).is_gt() {
                        *current = cell;
                    }
                })
                .or_insert(cell);
        }
        cells.into_values().collect()
    }

    pub fn highest_legal_cells_by_pattern(&self) -> Vec<&ScoreCell> {
        extrema_by_pattern(self.highest_legal_cells_by_candidate_pattern(), true)
    }

    /// Returns every candidate that reaches the maximum score for each
    /// pattern. Multiple traces belonging to the same candidate are reduced
    /// first, so one canonical trace represents that candidate. Attack stays
    /// informational and never participates in either reduction.
    pub fn highest_score_cells_by_pattern_preserving_candidate_ties(&self) -> Vec<&ScoreCell> {
        let candidate_maxima = self.highest_legal_cells_by_candidate_pattern();
        let mut maximum_score_by_pattern = BTreeMap::<usize, u64>::new();
        for cell in &candidate_maxima {
            maximum_score_by_pattern
                .entry(cell.pattern_id)
                .and_modify(|score| *score = (*score).max(cell.score))
                .or_insert(cell.score);
        }

        candidate_maxima
            .into_iter()
            .filter(|cell| {
                maximum_score_by_pattern.get(&cell.pattern_id).copied() == Some(cell.score)
            })
            .collect()
    }

    pub fn minimal_shape_cells_by_pattern(&self) -> Vec<&ScoreCell> {
        extrema_by_pattern(self.highest_legal_cells_by_candidate_pattern(), false)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreMatrixCancelled;

fn score_matrix_materialized(pattern_count: usize, source_complete: bool, has_cells: bool) -> bool {
    pattern_count > 0 && (source_complete || has_cells)
}

fn try_owned_str(value: &str) -> Result<String, ScoreMatrixMemoryGuardError> {
    try_owned_str_with_capacity(value, value.len())
}

fn try_owned_str_with_capacity(
    value: &str,
    capacity: usize,
) -> Result<String, ScoreMatrixMemoryGuardError> {
    if capacity < value.len() {
        return Err(ScoreMatrixMemoryGuardError::ProjectionOverflow);
    }
    let mut owned = String::new();
    owned
        .try_reserve_exact(capacity)
        .map_err(|_| ScoreMatrixMemoryGuardError::AllocationFailed)?;
    owned.push_str(value);
    Ok(owned)
}

fn trace_satisfies_requirement(events: &[ReplayEvent], requirement: TraceRequirement) -> bool {
    match requirement {
        TraceRequirement::None => true,
        TraceRequirement::PlacementTrace => events
            .iter()
            .any(|event| matches!(event, ReplayEvent::Placement(_))),
        TraceRequirement::FullDropTrace => events
            .iter()
            .any(|event| matches!(event, ReplayEvent::Drop(_))),
        TraceRequirement::KickEvidenceTrace => events
            .iter()
            .any(|event| matches!(event, ReplayEvent::KickEvidence(_))),
    }
}

fn trace_has_complete_spin_evidence(
    trace: &clearra_replay::ReplayTrace,
    profile: &ScoreProfile,
) -> bool {
    if trace.events().iter().any(|event| {
        matches!(
            event,
            ReplayEvent::TraceCompleteness(completeness)
                if completeness.completeness() != clearra_replay::TraceCompleteness::Complete
        )
    }) {
        return false;
    }
    trace.solution_trace().steps().iter().all(|step| {
        let requires_evidence = profile
            .spin_profile()
            .requires_complete_movement_evidence(step.piece_decision().active_piece().as_ascii());
        if !requires_evidence {
            return true;
        }
        let movement = trace.events().iter().find_map(|event| match event {
            ReplayEvent::MovementEvidence(evidence)
                if evidence.step_index() == step.step_index() =>
            {
                Some(*evidence)
            }
            _ => None,
        });
        let Some(movement) = movement else {
            return false;
        };
        if !movement.path_complete() || !movement.rotation_evidence_complete() {
            return false;
        }
        if !movement.last_action_was_rotation() {
            return !movement.used_kick();
        }
        if !movement.used_kick() {
            return true;
        }
        let Ok(result_x) = i16::try_from(step.placement().x()) else {
            return false;
        };
        let Ok(result_y) = i16::try_from(step.placement().y()) else {
            return false;
        };
        trace.events().iter().any(|event| {
            matches!(
                event,
                ReplayEvent::KickEvidence(evidence)
                    if evidence.step_index() == step.step_index()
                        && evidence.first_success_confirmed()
                        && evidence.to_rotation() == step.placement().rotation().quarter_turns()
                        && evidence.result() == (result_x, result_y)
            )
        })
    })
}

fn extrema_by_pattern(mut cells: Vec<&ScoreCell>, maximum: bool) -> Vec<&ScoreCell> {
    cells.sort_by_key(|cell| (cell.pattern_id, cell.candidate_id));
    let mut selected = BTreeMap::<usize, &ScoreCell>::new();
    for cell in cells {
        selected
            .entry(cell.pattern_id)
            .and_modify(|current| {
                if score_cell_is_preferred(cell, current, maximum) {
                    *current = cell;
                }
            })
            .or_insert(cell);
    }
    selected.into_values().collect()
}

fn score_cell_is_preferred(candidate: &ScoreCell, current: &ScoreCell, maximum: bool) -> bool {
    let score_order = candidate.score.cmp(&current.score);
    if !score_order.is_eq() {
        return if maximum {
            score_order.is_gt()
        } else {
            score_order.is_lt()
        };
    }
    (candidate.candidate_id, candidate.trace_identity.as_str())
        < (current.candidate_id, current.trace_identity.as_str())
}

fn score_cell_order(left: &ScoreCell, right: &ScoreCell) -> std::cmp::Ordering {
    left.score
        .cmp(&right.score)
        .then_with(|| right.candidate_id.cmp(&left.candidate_id))
        .then_with(|| right.trace_identity.cmp(&left.trace_identity))
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_geometry::layout::board64_layout::Board64Layout;
    use clearra_objectives::policy::score_objective_policy::ScoreObjectivePolicy;
    use clearra_replay::{BuildVariantOperation, BuildVariantReplayInput, ReplayEngine};

    use super::*;
    use crate::{
        CandidateExecution, CandidateExecutionAggregate, PcScoringPostProcessInput,
        PcScoringPostProcessor,
    };

    fn cell_signatures(cells: Vec<&ScoreCell>) -> Vec<(u64, usize, String, u64, u32)> {
        cells
            .into_iter()
            .map(|cell| {
                (
                    cell.candidate_id(),
                    cell.pattern_id(),
                    cell.trace_identity().to_owned(),
                    cell.score(),
                    cell.attack(),
                )
            })
            .collect()
    }

    fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
        fields
            .iter()
            .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
    }

    #[test]
    fn score_extrema_reduce_each_candidate_to_its_highest_trace_before_pattern_reductions() {
        let profile = ScoreProfile::new("score-extrema", "Score extrema");
        let accuracy = profile.accuracy_level().as_str();
        let cells = vec![
            // Score alone defines the optimum. Attack is informational, and
            // the lexicographically smaller trace represents an exact tie.
            ScoreCell::new_with_static_accuracy(1, 0, "score-loses", 19, 99, accuracy),
            ScoreCell::new_with_static_accuracy(1, 0, "attack-loses", 20, 1, accuracy),
            ScoreCell::new_with_static_accuracy(1, 0, "trace-z", 20, 2, accuracy),
            ScoreCell::new_with_static_accuracy(1, 0, "trace-a", 20, 2, accuracy),
            // Exact cross-candidate ties choose the smaller candidate id.
            ScoreCell::new_with_static_accuracy(2, 0, "candidate-two", 20, 2, accuracy),
            ScoreCell::new_with_static_accuracy(3, 0, "candidate-three", 20, 2, accuracy),
            // The lower envelope must see candidate 4's maximum (15), not its
            // otherwise globally minimal but non-selected trace (1).
            ScoreCell::new_with_static_accuracy(4, 0, "hidden-low-trace", 1, 0, accuracy),
            ScoreCell::new_with_static_accuracy(4, 0, "candidate-four-max", 15, 8, accuracy),
        ];
        let mut reversed_cells = cells.clone();
        reversed_cells.reverse();
        let forward = ScoreMatrix::from_materialized_cells(cells, &profile, 1, true);
        let reversed = ScoreMatrix::from_materialized_cells(reversed_cells, &profile, 1, true);

        let expected_candidate_maxima = vec![
            (1, 0, "attack-loses".to_owned(), 20, 1),
            (2, 0, "candidate-two".to_owned(), 20, 2),
            (3, 0, "candidate-three".to_owned(), 20, 2),
            (4, 0, "candidate-four-max".to_owned(), 15, 8),
        ];
        let expected_pattern_maximum = vec![(1, 0, "attack-loses".to_owned(), 20, 1)];
        let expected_pattern_maximum_ties = vec![
            (1, 0, "attack-loses".to_owned(), 20, 1),
            (2, 0, "candidate-two".to_owned(), 20, 2),
            (3, 0, "candidate-three".to_owned(), 20, 2),
        ];
        let expected_score_minimal = vec![(4, 0, "candidate-four-max".to_owned(), 15, 8)];

        for matrix in [&forward, &reversed] {
            assert!(matrix.complete());
            assert_eq!(
                cell_signatures(matrix.highest_legal_cells_by_candidate_pattern()),
                expected_candidate_maxima
            );
            assert_eq!(
                cell_signatures(matrix.highest_legal_cells_by_pattern()),
                expected_pattern_maximum
            );
            assert_eq!(
                cell_signatures(matrix.highest_score_cells_by_pattern_preserving_candidate_ties()),
                expected_pattern_maximum_ties
            );
            assert_eq!(
                cell_signatures(matrix.minimal_shape_cells_by_pattern()),
                expected_score_minimal
            );
        }
    }

    #[test]
    fn missing_required_trace_keeps_the_score_matrix_and_objective_incomplete() {
        let profile = ScoreProfile::new("kick-required", "Kick required")
            .with_trace_requirement(TraceRequirement::KickEvidenceTrace);
        let input = BuildVariantReplayInput::new(
            "no-kick-evidence",
            Board64Layout::standard_10_by_lines(2).expect("layout"),
            0,
            vec![BuildVariantOperation::new(
                PieceKind::O,
                RotationState::Zero,
                0,
                0,
            )],
        );
        let trace = ReplayEngine::build_variant_to_trace(&input).expect("replay");
        assert!(!trace
            .events()
            .iter()
            .any(|event| matches!(event, ReplayEvent::KickEvidence(_))));
        let aggregates = [CandidateExecutionAggregate::new(
            1,
            vec![CandidateExecution::new(0, "no-kick-evidence", trace)],
        )];

        let matrix = ScoreMatrix::materialize(&aggregates, &profile, 1, true);

        assert!(matrix.materialized());
        assert!(!matrix.complete());
        assert_eq!(matrix.cell_count(), 0);
        assert_eq!(
            matrix.incomplete_reason(),
            Some("score_trace_requirement_not_met")
        );
        assert!(!matrix
            .to_objective_matrix()
            .expect("objective matrix")
            .complete());
    }

    #[test]
    fn invalid_weights_cannot_make_a_complete_matrix_summary_or_objective_complete() {
        let policy = ScoreObjectivePolicy::summary();
        let profile = crate::score_profile_selection::score_profile(policy);
        let matrix = ScoreMatrix::from_materialized_cells(
            vec![ScoreCell::new_with_static_accuracy(
                1,
                0,
                "complete-cell",
                100,
                1,
                profile.accuracy_level().as_str(),
            )],
            &profile,
            2,
            true,
        );
        let missing = [];
        let non_finite = [f64::NAN, 0.0];
        let negative = [-0.1, 1.1];
        let incomplete_total = [0.25, 0.25];

        assert!(matrix.complete());
        for weights in [
            missing.as_slice(),
            non_finite.as_slice(),
            negative.as_slice(),
            incomplete_total.as_slice(),
        ] {
            let fields = PcScoringPostProcessor::process_materialized_with_control(
                PcScoringPostProcessInput::new(None, &[], weights, 2, true, policy, true, "1", 1),
                matrix.clone(),
                &ExecutionControl::default(),
            )
            .expect("postprocess")
            .fields();

            assert_eq!(field_value(&fields, "score_matrix_complete"), Some("true"));
            assert_eq!(
                field_value(&fields, "score_summary_complete"),
                Some("false")
            );
            assert_eq!(
                field_value(&fields, "score_summary_incomplete_reason"),
                Some("pattern_weight_model_not_materialized")
            );
            assert_eq!(field_value(&fields, "objective_complete"), Some("false"));
            assert_eq!(
                field_value(&fields, "objective_incomplete_reason"),
                Some("pattern_weight_model_not_materialized")
            );
        }
    }

    #[test]
    fn partial_replay_matrix_is_materialized_but_incomplete() {
        let profile = ScoreProfile::new("partial", "Partial");
        let input = BuildVariantReplayInput::new(
            "partial-trace",
            Board64Layout::standard_10_by_lines(2).expect("layout"),
            0,
            vec![BuildVariantOperation::new(
                PieceKind::O,
                RotationState::Zero,
                0,
                0,
            )],
        );
        let trace = ReplayEngine::build_variant_to_trace(&input).expect("replay");
        let aggregates = [CandidateExecutionAggregate::new(
            1,
            vec![CandidateExecution::new(0, "partial-trace", trace)],
        )];

        let matrix = ScoreMatrix::materialize(&aggregates, &profile, 1, false);

        assert!(matrix.materialized());
        assert!(!matrix.complete());
        assert_eq!(matrix.cell_count(), 1);
        assert_eq!(
            matrix.incomplete_reason(),
            Some("score_execution_source_incomplete")
        );
    }

    #[test]
    fn partial_precomputed_matrix_is_materialized_but_incomplete() {
        let profile = ScoreProfile::new("partial", "Partial");
        let matrix = ScoreMatrix::from_materialized_cells(
            vec![ScoreCell::new(
                1,
                0,
                "partial-cell",
                100,
                0,
                profile.accuracy_level().as_str(),
            )],
            &profile,
            1,
            false,
        );

        assert!(matrix.materialized());
        assert!(!matrix.complete());
        assert_eq!(matrix.cell_count(), 1);
        assert_eq!(
            matrix.incomplete_reason(),
            Some("score_execution_source_incomplete")
        );
        let expected_retained_bytes = (matrix.cells.capacity() as u128)
            * (core::mem::size_of::<ScoreCell>() as u128)
            + matrix
                .cells
                .iter()
                .map(|cell| {
                    cell.trace_identity.capacity() as u128
                        + match &cell.accuracy_level {
                            Cow::Borrowed(_) => 0,
                            Cow::Owned(value) => value.capacity() as u128,
                        }
                })
                .sum::<u128>()
            + matrix.profile_id.capacity() as u128
            + matrix.accuracy_level.capacity() as u128;
        assert_eq!(
            matrix.checked_retained_bytes(),
            Some(expected_retained_bytes)
        );
    }

    #[test]
    fn static_accuracy_cell_does_not_allocate_per_cell_accuracy_storage() {
        let cell =
            ScoreCell::new_with_static_accuracy(1, 0, "fixed-trace", 100, 0, "basic-approximation");

        assert!(matches!(cell.accuracy_level, Cow::Borrowed(_)));
        assert_eq!(cell.accuracy_level(), "basic-approximation");
    }

    #[test]
    fn guarded_materialized_matrix_checks_exact_cap_underflow_overflow_and_allocation() {
        let profile = ScoreProfile::new("guarded-profile", "Guarded profile");
        let make_cells = || {
            vec![ScoreCell::new_with_static_accuracy(
                1,
                0,
                "guarded-trace",
                100,
                1,
                profile.accuracy_level().as_str(),
            )]
        };
        let cells = make_cells();
        let projection = ScoreMatrix::checked_materialized_cells_memory_projection(
            &cells,
            cells.capacity(),
            &profile,
        )
        .expect("matrix projection");
        let already_retained_bytes = 5;
        let exact_cap = already_retained_bytes + projection.required_peak_bytes;
        let legacy = ScoreMatrix::from_materialized_cells(cells.clone(), &profile, 1, true);
        let (guarded, report) = ScoreMatrix::from_materialized_cells_with_memory_guard(
            cells,
            &profile,
            1,
            true,
            already_retained_bytes,
            exact_cap,
        )
        .expect("exact cap");
        assert_eq!(guarded, legacy);
        assert_eq!(report.projection, projection);
        assert!(report.retained_bytes <= projection.required_peak_bytes);

        let under = ScoreMatrix::from_materialized_cells_with_memory_guard(
            make_cells(),
            &profile,
            1,
            true,
            already_retained_bytes,
            exact_cap - 1,
        )
        .expect_err("one byte under");
        assert_eq!(
            under,
            ScoreMatrixMemoryGuardError::LimitExceeded {
                required_memory_bytes: exact_cap,
                max_memory_bytes: exact_cap - 1,
            }
        );

        let overflow = ScoreMatrix::from_materialized_cells_with_memory_guard(
            make_cells(),
            &profile,
            1,
            true,
            u128::MAX,
            u128::MAX,
        )
        .expect_err("retained plus matrix projection overflow");
        assert_eq!(overflow, ScoreMatrixMemoryGuardError::ProjectionOverflow);

        assert_eq!(
            try_owned_str_with_capacity("", usize::MAX),
            Err(ScoreMatrixMemoryGuardError::AllocationFailed)
        );
    }

    #[test]
    fn incomplete_empty_matrix_is_not_materialized() {
        let profile = ScoreProfile::new("empty", "Empty");
        let matrix = ScoreMatrix::from_materialized_cells(Vec::new(), &profile, 1, false);

        assert!(!matrix.materialized());
        assert!(!matrix.complete());
        assert_eq!(matrix.cell_count(), 0);
    }

    #[test]
    fn complete_empty_matrix_remains_materialized() {
        let profile = ScoreProfile::new("empty", "Empty");
        let matrix = ScoreMatrix::from_materialized_cells(Vec::new(), &profile, 1, true);

        assert!(matrix.materialized());
        assert!(matrix.complete());
        assert_eq!(matrix.cell_count(), 0);
    }
}
