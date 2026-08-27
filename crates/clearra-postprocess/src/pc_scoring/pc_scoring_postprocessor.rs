use std::fmt::{self, Write};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::pattern::{pattern_id::PatternId, weighted_pattern_set::WeightedPatternSet};
use clearra_objectives::policy::score_objective_policy::{
    ScoreObjectiveMode, ScoreObjectivePolicy,
};
use clearra_replay::{ReplayEvent, ReplayTrace};
use clearra_scoring::{
    model::{ScoreEvaluationBasis, ScoreEvaluationPolicy, ScoreModelEvaluator},
    profile::ScoreProfile,
};

use crate::{CandidateExecutionAggregate, ScoreCell, ScoreMatrix};

#[derive(Clone, Copy, Debug)]
pub struct PcScoringPostProcessInput<'a> {
    replay_trace: Option<&'a ReplayTrace>,
    candidate_executions: &'a [CandidateExecutionAggregate],
    pattern_weights: &'a [f64],
    pattern_count: usize,
    score_matrix_source_complete: bool,
    score_policy: ScoreObjectivePolicy,
    search_objective_complete: bool,
    coverage_probability: &'a str,
    retained_trace_count: usize,
}

impl<'a> PcScoringPostProcessInput<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        replay_trace: Option<&'a ReplayTrace>,
        candidate_executions: &'a [CandidateExecutionAggregate],
        pattern_weights: &'a [f64],
        pattern_count: usize,
        score_matrix_source_complete: bool,
        score_policy: ScoreObjectivePolicy,
        search_objective_complete: bool,
        coverage_probability: &'a str,
        retained_trace_count: usize,
    ) -> Self {
        Self {
            replay_trace,
            candidate_executions,
            pattern_weights,
            pattern_count,
            score_matrix_source_complete,
            score_policy,
            search_objective_complete,
            coverage_probability,
            retained_trace_count,
        }
    }

    fn evaluation_basis(self) -> ScoreEvaluationBasis {
        if self.score_matrix_source_complete {
            ScoreEvaluationBasis::AllTraces
        } else if self.retained_trace_count > 0 {
            ScoreEvaluationBasis::RetainedTraces
        } else {
            ScoreEvaluationBasis::Sample
        }
    }

    fn evaluation_scope(self) -> &'static str {
        if self.score_matrix_source_complete {
            "full"
        } else {
            "sample"
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcScoringPostProcessor;

#[derive(Clone, Debug, PartialEq)]
pub struct PcScoringPostProcessResult {
    fields: Vec<(String, String)>,
}

impl PcScoringPostProcessResult {
    pub fn fields(self) -> Vec<(String, String)> {
        self.fields
    }

    pub fn checked_retained_bytes(&self) -> Option<u128> {
        (self.fields.capacity() as u128)
            .checked_mul(core::mem::size_of::<(String, String)>() as u128)?
            .checked_add(self.fields.iter().try_fold(0_u128, |total, (key, value)| {
                total
                    .checked_add(key.capacity() as u128)?
                    .checked_add(value.capacity() as u128)
            })?)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcScoringMemoryProjection {
    pub profile_retained_bytes: u128,
    pub matrix_retained_bytes: u128,
    pub weight_storage_bytes: u128,
    pub highest_cell_storage_bytes: u128,
    pub field_capacity: usize,
    pub field_outer_storage_bytes: u128,
    pub field_string_storage_bytes: u128,
    pub required_peak_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcScoringMemoryReport {
    pub projection: PcScoringMemoryProjection,
    pub result_retained_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcScoringMemoryGuardError {
    Cancelled,
    ProfileMismatch,
    ProjectionOverflow,
    LimitExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
    AllocationFailed,
}

impl PcScoringPostProcessor {
    pub fn fields(input: PcScoringPostProcessInput<'_>) -> Vec<(String, String)> {
        Self::fields_with_control(input, &ExecutionControl::default())
            .expect("default execution control cannot be cancelled")
    }

    pub fn fields_with_control(
        input: PcScoringPostProcessInput<'_>,
        control: &ExecutionControl,
    ) -> Result<Vec<(String, String)>, PcPostProcessCancelled> {
        Self::process_with_control(input, control).map(PcScoringPostProcessResult::fields)
    }

    pub fn process_with_control(
        input: PcScoringPostProcessInput<'_>,
        control: &ExecutionControl,
    ) -> Result<PcScoringPostProcessResult, PcPostProcessCancelled> {
        let profile = crate::score_profile_selection::score_profile(input.score_policy);
        let evaluation_policy = ScoreEvaluationPolicy::tetrio_pc(input.score_policy.initial_b2b());
        let matrix = ScoreMatrix::materialize_with_policy_and_control(
            input.candidate_executions,
            &profile,
            evaluation_policy,
            input.pattern_count,
            input.score_matrix_source_complete,
            control,
        )
        .map_err(|_| PcPostProcessCancelled)?;
        Self::process_matrix(input, &profile, matrix, evaluation_policy, control)
    }

    pub fn process_materialized_with_control(
        input: PcScoringPostProcessInput<'_>,
        matrix: ScoreMatrix,
        control: &ExecutionControl,
    ) -> Result<PcScoringPostProcessResult, PcPostProcessCancelled> {
        let profile = crate::score_profile_selection::score_profile(input.score_policy);
        if matrix.profile_id() != profile.id() {
            return Self::process_matrix(
                input,
                &profile,
                ScoreMatrix::unavailable(&profile, input.pattern_count),
                ScoreEvaluationPolicy::tetrio_pc(input.score_policy.initial_b2b()),
                control,
            );
        }
        let evaluation_policy = ScoreEvaluationPolicy::tetrio_pc(input.score_policy.initial_b2b());
        Self::process_matrix(input, &profile, matrix, evaluation_policy, control)
    }

    pub fn checked_materialized_memory_projection(
        input: PcScoringPostProcessInput<'_>,
        profile: &ScoreProfile,
        profile_retained_bytes: u128,
        matrix: &ScoreMatrix,
    ) -> Option<PcScoringMemoryProjection> {
        (crate::score_profile_selection::score_profile_matches_policy(profile, input.score_policy)
            && matrix.profile_id() == profile.id())
        .then_some(())?;
        let evaluation_policy = ScoreEvaluationPolicy::tetrio_pc(input.score_policy.initial_b2b());
        checked_guarded_projection(
            input,
            profile,
            profile_retained_bytes,
            matrix,
            evaluation_policy,
        )
    }

    /// Fallible typed score-summary construction under one total memory cap.
    ///
    /// `already_retained_bytes` is caller-owned storage outside `profile` and
    /// `matrix`; the returned projection charges both of them, weights, the
    /// highest-cell index, field vector, and every field key/value string.
    pub fn process_materialized_with_memory_guard(
        input: PcScoringPostProcessInput<'_>,
        profile: &ScoreProfile,
        profile_retained_bytes: u128,
        matrix: ScoreMatrix,
        control: &ExecutionControl,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<(PcScoringPostProcessResult, PcScoringMemoryReport), PcScoringMemoryGuardError>
    {
        if control.is_cancelled() {
            return Err(PcScoringMemoryGuardError::Cancelled);
        }
        if !crate::score_profile_selection::score_profile_matches_policy(
            profile,
            input.score_policy,
        ) || matrix.profile_id() != profile.id()
        {
            return Err(PcScoringMemoryGuardError::ProfileMismatch);
        }
        let evaluation_policy = ScoreEvaluationPolicy::tetrio_pc(input.score_policy.initial_b2b());
        let projection = checked_guarded_projection(
            input,
            profile,
            profile_retained_bytes,
            &matrix,
            evaluation_policy,
        )
        .ok_or(PcScoringMemoryGuardError::ProjectionOverflow)?;
        let required_memory_bytes = already_retained_bytes
            .checked_add(projection.required_peak_bytes)
            .ok_or(PcScoringMemoryGuardError::ProjectionOverflow)?;
        if required_memory_bytes > max_memory_bytes {
            return Err(PcScoringMemoryGuardError::LimitExceeded {
                required_memory_bytes,
                max_memory_bytes,
            });
        }

        let weights =
            guarded_materialized_weights(input.pattern_weights, input.pattern_count, control)?;
        let highest = guarded_highest_cells_by_pattern(&matrix, control)?;
        let facts = score_field_facts_from_highest(&highest, weights.as_deref());
        let mut sink = GuardedFieldSink::with_capacity(projection.field_capacity, control)?;
        emit_score_fields(
            &mut sink,
            input,
            profile,
            &matrix,
            evaluation_policy,
            weights.is_some(),
            facts,
        )?;
        let result = PcScoringPostProcessResult {
            fields: sink.into_fields(),
        };
        let result_retained_bytes = result
            .checked_retained_bytes()
            .ok_or(PcScoringMemoryGuardError::ProjectionOverflow)?;
        let actual_peak_bytes = matrix
            .checked_retained_bytes()
            .and_then(|bytes| bytes.checked_add(profile_retained_bytes))
            .and_then(|bytes| {
                bytes.checked_add(
                    (weights.as_ref().map_or(0, Vec::capacity) as u128)
                        .checked_mul(core::mem::size_of::<ProbabilityValue>() as u128)?,
                )
            })
            .and_then(|bytes| {
                bytes.checked_add(
                    (highest.capacity() as u128)
                        .checked_mul(core::mem::size_of::<&ScoreCell>() as u128)?,
                )
            })
            .and_then(|bytes| bytes.checked_add(result_retained_bytes))
            .and_then(|bytes| already_retained_bytes.checked_add(bytes))
            .ok_or(PcScoringMemoryGuardError::ProjectionOverflow)?;
        if actual_peak_bytes > max_memory_bytes {
            return Err(PcScoringMemoryGuardError::LimitExceeded {
                required_memory_bytes: actual_peak_bytes,
                max_memory_bytes,
            });
        }
        Ok((
            result,
            PcScoringMemoryReport {
                projection,
                result_retained_bytes,
            },
        ))
    }

    fn process_matrix(
        input: PcScoringPostProcessInput<'_>,
        profile: &ScoreProfile,
        matrix: ScoreMatrix,
        evaluation_policy: ScoreEvaluationPolicy,
        control: &ExecutionControl,
    ) -> Result<PcScoringPostProcessResult, PcPostProcessCancelled> {
        if control.is_cancelled() {
            return Err(PcPostProcessCancelled);
        }
        let mut fields = score_contract_fields(input, profile, &matrix, evaluation_policy);
        fields.extend(score_summary_fields(input, &matrix));
        fields.extend(score_mode_completion_fields(input, &matrix));
        Ok(PcScoringPostProcessResult { fields })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcPostProcessCancelled;

#[derive(Clone, Copy, Debug, Default)]
struct ScoreFieldFacts<'a> {
    best: Option<&'a ScoreCell>,
    pattern_optimal_count: usize,
    totals: WeightedScoreTotals,
}

trait ScoreFieldSink {
    fn emit(
        &mut self,
        key: &'static str,
        value: &dyn fmt::Display,
    ) -> Result<(), PcScoringMemoryGuardError>;
}

#[derive(Default)]
struct ProjectionFieldSink {
    field_count: usize,
    string_storage_bytes: u128,
}

impl ScoreFieldSink for ProjectionFieldSink {
    fn emit(
        &mut self,
        key: &'static str,
        value: &dyn fmt::Display,
    ) -> Result<(), PcScoringMemoryGuardError> {
        self.field_count = self
            .field_count
            .checked_add(1)
            .ok_or(PcScoringMemoryGuardError::ProjectionOverflow)?;
        let value_len = displayed_len(value)?;
        self.string_storage_bytes = self
            .string_storage_bytes
            .checked_add(key.len() as u128)
            .and_then(|bytes| bytes.checked_add(value_len as u128))
            .ok_or(PcScoringMemoryGuardError::ProjectionOverflow)?;
        Ok(())
    }
}

struct GuardedFieldSink<'a> {
    fields: Vec<(String, String)>,
    control: &'a ExecutionControl,
}

impl<'a> GuardedFieldSink<'a> {
    fn with_capacity(
        capacity: usize,
        control: &'a ExecutionControl,
    ) -> Result<Self, PcScoringMemoryGuardError> {
        let mut fields = Vec::new();
        fields
            .try_reserve_exact(capacity)
            .map_err(|_| PcScoringMemoryGuardError::AllocationFailed)?;
        Ok(Self { fields, control })
    }

    fn into_fields(self) -> Vec<(String, String)> {
        self.fields
    }
}

impl ScoreFieldSink for GuardedFieldSink<'_> {
    fn emit(
        &mut self,
        key: &'static str,
        value: &dyn fmt::Display,
    ) -> Result<(), PcScoringMemoryGuardError> {
        if self.control.is_cancelled() {
            return Err(PcScoringMemoryGuardError::Cancelled);
        }
        if self.fields.len() == self.fields.capacity() {
            return Err(PcScoringMemoryGuardError::ProjectionOverflow);
        }
        self.fields
            .push((guarded_string(key)?, guarded_display_string(value)?));
        Ok(())
    }
}

#[derive(Default)]
struct DisplayLength {
    len: usize,
}

impl Write for DisplayLength {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.len = self.len.checked_add(value.len()).ok_or(fmt::Error)?;
        Ok(())
    }
}

fn displayed_len(value: &dyn fmt::Display) -> Result<usize, PcScoringMemoryGuardError> {
    let mut length = DisplayLength::default();
    write!(&mut length, "{value}").map_err(|_| PcScoringMemoryGuardError::ProjectionOverflow)?;
    Ok(length.len)
}

fn guarded_string(value: &str) -> Result<String, PcScoringMemoryGuardError> {
    let mut output = String::new();
    output
        .try_reserve_exact(value.len())
        .map_err(|_| PcScoringMemoryGuardError::AllocationFailed)?;
    output.push_str(value);
    Ok(output)
}

fn guarded_display_string(value: &dyn fmt::Display) -> Result<String, PcScoringMemoryGuardError> {
    let required = displayed_len(value)?;
    let mut output = String::new();
    output
        .try_reserve_exact(required)
        .map_err(|_| PcScoringMemoryGuardError::AllocationFailed)?;
    write!(&mut output, "{value}").map_err(|_| PcScoringMemoryGuardError::AllocationFailed)?;
    debug_assert_eq!(output.len(), required);
    Ok(output)
}

fn checked_guarded_projection(
    input: PcScoringPostProcessInput<'_>,
    profile: &ScoreProfile,
    profile_retained_bytes: u128,
    matrix: &ScoreMatrix,
    evaluation_policy: ScoreEvaluationPolicy,
) -> Option<PcScoringMemoryProjection> {
    let weights_valid =
        materialized_weight_values_are_valid(input.pattern_weights, input.pattern_count);
    let facts = score_field_facts_without_allocation(
        matrix,
        weights_valid.then_some(input.pattern_weights),
    );
    let mut sink = ProjectionFieldSink::default();
    emit_score_fields(
        &mut sink,
        input,
        profile,
        matrix,
        evaluation_policy,
        weights_valid,
        facts,
    )
    .ok()?;

    let matrix_retained_bytes = matrix.checked_retained_bytes()?;
    let weight_storage_bytes = if weights_valid {
        (input.pattern_count as u128)
            .checked_mul(core::mem::size_of::<ProbabilityValue>() as u128)?
    } else {
        0
    };
    let highest_cell_storage_bytes = (facts.pattern_optimal_count as u128)
        .checked_mul(core::mem::size_of::<&ScoreCell>() as u128)?;
    let field_outer_storage_bytes =
        (sink.field_count as u128).checked_mul(core::mem::size_of::<(String, String)>() as u128)?;
    let required_peak_bytes = profile_retained_bytes
        .checked_add(matrix_retained_bytes)?
        .checked_add(weight_storage_bytes)?
        .checked_add(highest_cell_storage_bytes)?
        .checked_add(field_outer_storage_bytes)?
        .checked_add(sink.string_storage_bytes)?;
    Some(PcScoringMemoryProjection {
        profile_retained_bytes,
        matrix_retained_bytes,
        weight_storage_bytes,
        highest_cell_storage_bytes,
        field_capacity: sink.field_count,
        field_outer_storage_bytes,
        field_string_storage_bytes: sink.string_storage_bytes,
        required_peak_bytes,
    })
}

fn materialized_weight_values_are_valid(values: &[f64], pattern_count: usize) -> bool {
    if values.len() != pattern_count || pattern_count == 0 {
        return false;
    }
    let Some(total_weight) = values.iter().try_fold(0.0, |total, value| {
        ProbabilityValue::new(*value).ok().map(|_| total + value)
    }) else {
        return false;
    };
    let weighted_set_tolerance = f64::EPSILON * values.len().max(1) as f64 * 2.0;
    (total_weight - 1.0).abs() <= 1.0e-8 && total_weight <= 1.0 + weighted_set_tolerance
}

fn guarded_materialized_weights(
    values: &[f64],
    pattern_count: usize,
    control: &ExecutionControl,
) -> Result<Option<Vec<ProbabilityValue>>, PcScoringMemoryGuardError> {
    if !materialized_weight_values_are_valid(values, pattern_count) {
        return Ok(None);
    }
    let mut weights = Vec::new();
    weights
        .try_reserve_exact(pattern_count)
        .map_err(|_| PcScoringMemoryGuardError::AllocationFailed)?;
    for value in values {
        if control.is_cancelled() {
            return Err(PcScoringMemoryGuardError::Cancelled);
        }
        let weight = ProbabilityValue::new(*value)
            .map_err(|_| PcScoringMemoryGuardError::ProjectionOverflow)?;
        weights.push(weight);
    }
    Ok(Some(weights))
}

fn guarded_highest_cells_by_pattern<'a>(
    matrix: &'a ScoreMatrix,
    control: &ExecutionControl,
) -> Result<Vec<&'a ScoreCell>, PcScoringMemoryGuardError> {
    let group_count = score_field_facts_without_allocation(matrix, None).pattern_optimal_count;
    let mut highest = Vec::new();
    highest
        .try_reserve_exact(group_count)
        .map_err(|_| PcScoringMemoryGuardError::AllocationFailed)?;
    let cells = matrix.cells();
    let mut cursor = 0;
    while cursor < cells.len() {
        if control.is_cancelled() {
            return Err(PcScoringMemoryGuardError::Cancelled);
        }
        let pattern_id = cells[cursor].pattern_id();
        let mut best = &cells[cursor];
        cursor += 1;
        while cursor < cells.len() && cells[cursor].pattern_id() == pattern_id {
            if score_cell_is_preferred_for_maximum(&cells[cursor], best) {
                best = &cells[cursor];
            }
            cursor += 1;
        }
        highest.push(best);
    }
    debug_assert_eq!(highest.len(), group_count);
    Ok(highest)
}

fn score_field_facts_without_allocation<'a>(
    matrix: &'a ScoreMatrix,
    weights: Option<&[f64]>,
) -> ScoreFieldFacts<'a> {
    let cells = matrix.cells();
    let mut facts = ScoreFieldFacts::default();
    let mut cursor = 0;
    while cursor < cells.len() {
        let pattern_id = cells[cursor].pattern_id();
        let mut best = &cells[cursor];
        cursor += 1;
        while cursor < cells.len() && cells[cursor].pattern_id() == pattern_id {
            if score_cell_is_preferred_for_maximum(&cells[cursor], best) {
                best = &cells[cursor];
            }
            cursor += 1;
        }
        facts.pattern_optimal_count += 1;
        if facts
            .best
            .is_none_or(|current| score_cell_is_preferred_for_maximum(best, current))
        {
            facts.best = Some(best);
        }
        if let Some(weight) = weights.and_then(|values| values.get(pattern_id)).copied() {
            facts.totals.covered_probability += weight;
            facts.totals.expected_score += weight * best.score() as f64;
            facts.totals.expected_attack += weight * f64::from(best.attack());
        }
    }
    facts
}

fn score_field_facts_from_highest<'a>(
    highest: &[&'a ScoreCell],
    weights: Option<&[ProbabilityValue]>,
) -> ScoreFieldFacts<'a> {
    let mut facts = ScoreFieldFacts {
        pattern_optimal_count: highest.len(),
        ..ScoreFieldFacts::default()
    };
    for &cell in highest {
        if facts
            .best
            .is_none_or(|current| score_cell_is_preferred_for_maximum(cell, current))
        {
            facts.best = Some(cell);
        }
        if let Some(weight) = weights
            .and_then(|values| values.get(cell.pattern_id()))
            .copied()
        {
            facts.totals.covered_probability += weight.get();
            facts.totals.expected_score += weight.get() * cell.score() as f64;
            facts.totals.expected_attack += weight.get() * f64::from(cell.attack());
        }
    }
    facts
}

fn score_cell_is_preferred_for_maximum(candidate: &ScoreCell, current: &ScoreCell) -> bool {
    let score_order = candidate.score().cmp(&current.score());
    if !score_order.is_eq() {
        return score_order.is_gt();
    }
    (candidate.candidate_id(), candidate.trace_identity())
        < (current.candidate_id(), current.trace_identity())
}

fn emit_guarded(
    sink: &mut impl ScoreFieldSink,
    key: &'static str,
    value: impl fmt::Display,
) -> Result<(), PcScoringMemoryGuardError> {
    sink.emit(key, &value)
}

fn emit_score_fields(
    sink: &mut impl ScoreFieldSink,
    input: PcScoringPostProcessInput<'_>,
    profile: &ScoreProfile,
    matrix: &ScoreMatrix,
    evaluation_policy: ScoreEvaluationPolicy,
    weights_valid: bool,
    facts: ScoreFieldFacts<'_>,
) -> Result<(), PcScoringMemoryGuardError> {
    emit_guarded(
        sink,
        "score_post_processing",
        input.score_policy.requested(),
    )?;
    emit_guarded(sink, "score_requested", input.score_policy.requested())?;
    emit_guarded(
        sink,
        "score_objective_mode",
        input.score_policy.mode().as_str(),
    )?;
    emit_guarded(sink, "score_initial_b2b", input.score_policy.initial_b2b())?;
    emit_guarded(
        sink,
        "score_b2b_chain_rule",
        profile.b2b_policy().chain_rule().as_str(),
    )?;
    emit_guarded(
        sink,
        "score_all_clear_b2b_extra_increment",
        profile
            .b2b_policy()
            .chain_rule()
            .all_clear_extra_increment(),
    )?;
    emit_guarded(sink, "score_hard_drop_included", false)?;
    emit_guarded(sink, "score_soft_drop_included", false)?;
    emit_guarded(sink, "score_attack_model", profile.attack_model().as_str())?;
    emit_guarded(
        sink,
        "score_level_multiplier",
        evaluation_policy.level_multiplier(),
    )?;
    emit_guarded(sink, "score_level_system_enabled", false)?;
    emit_guarded(
        sink,
        "score_spin_piece_scope",
        input.score_policy.spin_profile().as_str(),
    )?;
    emit_guarded(sink, "score_same_shape_policy", "highest-legal-trace")?;
    emit_guarded(sink, "score_core_hot_path", false)?;
    emit_guarded(sink, "score_postprocess_owner", "clearra-postprocess")?;
    emit_guarded(sink, "score_profile", profile.id())?;
    emit_guarded(
        sink,
        "score_accuracy_level",
        profile.accuracy_level().as_str(),
    )?;
    emit_guarded(sink, "score_accuracy_reason", profile.accuracy_reason())?;
    emit_guarded(
        sink,
        "score_profile_accuracy_mode",
        profile.accuracy_level().as_str(),
    )?;
    emit_guarded(
        sink,
        "score_profile_specific_exact",
        profile.profile_specific_exact(),
    )?;
    let event_basis = if matrix.materialized() {
        "exact-scoring-execution-graph"
    } else if !input.candidate_executions.is_empty() {
        "build-variant-replay"
    } else {
        "none"
    };
    emit_guarded(sink, "score_event_basis", event_basis)?;
    emit_guarded(
        sink,
        "score_interpretation_basis",
        "legal-buildable-replay-profile",
    )?;
    emit_guarded(sink, "score_evaluation_trace_count", matrix.cell_count())?;
    emit_guarded(sink, "score_evaluation_complete", matrix.complete())?;
    emit_guarded(
        sink,
        "score_evaluation_basis",
        input.evaluation_basis().as_str(),
    )?;
    emit_guarded(sink, "score_evaluation_scope", input.evaluation_scope())?;
    emit_guarded(sink, "score_matrix_materialized", matrix.materialized())?;
    emit_guarded(sink, "score_matrix_complete", matrix.complete())?;
    emit_guarded(sink, "score_matrix_cell_count", matrix.cell_count())?;
    emit_guarded(sink, "score_matrix_pattern_count", matrix.pattern_count())?;
    emit_guarded(sink, "score_matrix_profile_id", matrix.profile_id())?;
    emit_guarded(sink, "score_matrix_accuracy_level", matrix.accuracy_level())?;
    emit_guarded(
        sink,
        "score_matrix_incomplete_reason",
        matrix.incomplete_reason().unwrap_or("none"),
    )?;
    emit_guarded(sink, "score_probability_before", input.coverage_probability)?;
    emit_guarded(sink, "score_probability_after", input.coverage_probability)?;
    emit_guarded(sink, "score_does_not_change_probability_union", true)?;
    emit_guarded(sink, "score_best_complete", matrix.complete())?;

    if let Some(best) = facts.best {
        emit_guarded(sink, "score_best_score", best.score())?;
        emit_guarded(sink, "score_best_attack", best.attack())?;
    }

    if let Some(trace) = input.replay_trace {
        let evaluation = ScoreModelEvaluator::evaluate_replay_trace_with_policy(
            profile,
            trace,
            evaluation_policy,
        );
        let events = trace.events();
        emit_guarded(
            sink,
            "score_representative_score",
            evaluation.final_state().score(),
        )?;
        emit_guarded(
            sink,
            "score_representative_attack",
            evaluation.final_state().attack(),
        )?;
        emit_guarded(sink, "score_event_count", evaluation.event_count())?;
        emit_guarded(
            sink,
            "placement_event_available",
            events
                .iter()
                .any(|event| matches!(event, ReplayEvent::Placement(_))),
        )?;
        emit_guarded(
            sink,
            "clear_event_available",
            events
                .iter()
                .any(|event| matches!(event, ReplayEvent::LineClear(_))),
        )?;
        emit_guarded(
            sink,
            "drop_event_basis_available",
            events
                .iter()
                .any(|event| matches!(event, ReplayEvent::Drop(_))),
        )?;
        emit_guarded(
            sink,
            "spin_event_basis_available",
            events
                .iter()
                .any(|event| matches!(event, ReplayEvent::SpinBasis(_))),
        )?;
    } else {
        emit_guarded(sink, "placement_event_available", false)?;
        emit_guarded(sink, "clear_event_available", false)?;
        emit_guarded(sink, "drop_event_basis_available", false)?;
        emit_guarded(sink, "spin_event_basis_available", false)?;
    }

    if !weights_valid {
        emit_guarded(sink, "score_summary_complete", false)?;
        emit_guarded(
            sink,
            "score_summary_incomplete_reason",
            "pattern_weight_model_not_materialized",
        )?;
    } else if !matrix.complete() {
        emit_guarded(sink, "score_summary_complete", false)?;
        emit_guarded(
            sink,
            "score_summary_incomplete_reason",
            matrix
                .incomplete_reason()
                .unwrap_or("score_matrix_incomplete"),
        )?;
    } else {
        let conditional_average = (facts.totals.covered_probability > 0.0)
            .then(|| facts.totals.expected_score / facts.totals.covered_probability);
        emit_guarded(sink, "score_summary_complete", true)?;
        emit_guarded(sink, "score_summary_incomplete_reason", "none")?;
        emit_guarded(
            sink,
            "score_all_universe_patterns_covered",
            facts.pattern_optimal_count == input.pattern_count,
        )?;
        emit_guarded(
            sink,
            "score_pattern_optimal_count",
            facts.pattern_optimal_count,
        )?;
        emit_guarded(
            sink,
            "score_failed_pc_pattern_count",
            input
                .pattern_count
                .saturating_sub(facts.pattern_optimal_count),
        )?;
        emit_guarded(sink, "score_failed_pc_pattern_score", 0)?;
        emit_guarded(
            sink,
            "score_field_average_basis",
            "all-materialized-patterns-failed-pc-zero",
        )?;
        emit_guarded(
            sink,
            "score_covered_probability",
            facts.totals.covered_probability,
        )?;
        emit_guarded(
            sink,
            "score_field_average_score",
            facts.totals.expected_score,
        )?;
        emit_guarded(
            sink,
            "score_unconditional_expected_score",
            facts.totals.expected_score,
        )?;
        emit_guarded(
            sink,
            "score_unconditional_expected_attack",
            facts.totals.expected_attack,
        )?;
        if let Some(value) = conditional_average {
            emit_guarded(
                sink,
                "score_covered_pattern_conditional_average_score",
                value,
            )?;
        }
    }

    if input.score_policy.mode() == ScoreObjectiveMode::Summary {
        let reason = if !input.search_objective_complete {
            "search_objective_incomplete"
        } else if !weights_valid {
            "pattern_weight_model_not_materialized"
        } else if !matrix.complete() {
            matrix
                .incomplete_reason()
                .unwrap_or("score_matrix_incomplete")
        } else {
            "none"
        };
        emit_guarded(sink, "objective_complete", reason == "none")?;
        emit_guarded(sink, "objective_incomplete_reason", reason)?;
    }
    Ok(())
}

fn score_mode_completion_fields(
    input: PcScoringPostProcessInput<'_>,
    matrix: &ScoreMatrix,
) -> Vec<(String, String)> {
    if input.score_policy.mode() != ScoreObjectiveMode::Summary {
        return Vec::new();
    }

    let reason = if !input.search_objective_complete {
        "search_objective_incomplete"
    } else if materialized_weights(input.pattern_weights, input.pattern_count).is_none() {
        "pattern_weight_model_not_materialized"
    } else if !matrix.complete() {
        matrix
            .incomplete_reason()
            .unwrap_or("score_matrix_incomplete")
    } else {
        "none"
    };

    vec![
        field("objective_complete", reason == "none"),
        field("objective_incomplete_reason", reason),
    ]
}

fn score_contract_fields(
    input: PcScoringPostProcessInput<'_>,
    profile: &ScoreProfile,
    matrix: &ScoreMatrix,
    evaluation_policy: ScoreEvaluationPolicy,
) -> Vec<(String, String)> {
    let probability = input.coverage_probability.to_owned();
    let mut fields = vec![
        field("score_post_processing", input.score_policy.requested()),
        field("score_requested", input.score_policy.requested()),
        field("score_objective_mode", input.score_policy.mode().as_str()),
        field("score_initial_b2b", input.score_policy.initial_b2b()),
        field(
            "score_b2b_chain_rule",
            profile.b2b_policy().chain_rule().as_str(),
        ),
        field(
            "score_all_clear_b2b_extra_increment",
            profile
                .b2b_policy()
                .chain_rule()
                .all_clear_extra_increment(),
        ),
        field("score_hard_drop_included", false),
        field("score_soft_drop_included", false),
        field("score_attack_model", profile.attack_model().as_str()),
        field(
            "score_level_multiplier",
            evaluation_policy.level_multiplier(),
        ),
        field("score_level_system_enabled", false),
        field(
            "score_spin_piece_scope",
            input.score_policy.spin_profile().as_str(),
        ),
        field("score_same_shape_policy", "highest-legal-trace"),
        field("score_core_hot_path", false),
        field("score_postprocess_owner", "clearra-postprocess"),
        field("score_profile", profile.id()),
        field("score_accuracy_level", profile.accuracy_level().as_str()),
        field("score_accuracy_reason", profile.accuracy_reason()),
        field(
            "score_profile_accuracy_mode",
            profile.accuracy_level().as_str(),
        ),
        field(
            "score_profile_specific_exact",
            profile.profile_specific_exact(),
        ),
        field(
            "score_event_basis",
            if matrix.materialized() {
                "exact-scoring-execution-graph"
            } else if !input.candidate_executions.is_empty() {
                "build-variant-replay"
            } else {
                "none"
            },
        ),
        field(
            "score_interpretation_basis",
            "legal-buildable-replay-profile",
        ),
        field("score_evaluation_trace_count", matrix.cell_count()),
        field("score_evaluation_complete", matrix.complete()),
        field("score_evaluation_basis", input.evaluation_basis().as_str()),
        field("score_evaluation_scope", input.evaluation_scope()),
        field("score_matrix_materialized", matrix.materialized()),
        field("score_matrix_complete", matrix.complete()),
        field("score_matrix_cell_count", matrix.cell_count()),
        field("score_matrix_pattern_count", matrix.pattern_count()),
        field("score_matrix_profile_id", matrix.profile_id()),
        field("score_matrix_accuracy_level", matrix.accuracy_level()),
        field(
            "score_matrix_incomplete_reason",
            matrix.incomplete_reason().unwrap_or("none"),
        ),
        field("score_probability_before", probability.clone()),
        field("score_probability_after", probability),
        field("score_does_not_change_probability_union", true),
        field("score_best_complete", matrix.complete()),
    ];

    if let Some(best) = matrix
        .highest_legal_cells_by_pattern()
        .into_iter()
        .max_by(|left, right| {
            left.score()
                .cmp(&right.score())
                .then_with(|| right.candidate_id().cmp(&left.candidate_id()))
                .then_with(|| right.trace_identity().cmp(left.trace_identity()))
        })
    {
        fields.extend([
            field("score_best_score", best.score()),
            field("score_best_attack", best.attack()),
        ]);
    }

    if let Some(trace) = input.replay_trace {
        let evaluation = ScoreModelEvaluator::evaluate_replay_trace_with_policy(
            profile,
            trace,
            evaluation_policy,
        );
        let events = trace.events();
        fields.extend([
            field(
                "score_representative_score",
                evaluation.final_state().score(),
            ),
            field(
                "score_representative_attack",
                evaluation.final_state().attack(),
            ),
            field("score_event_count", evaluation.event_count()),
            field(
                "placement_event_available",
                events
                    .iter()
                    .any(|event| matches!(event, ReplayEvent::Placement(_))),
            ),
            field(
                "clear_event_available",
                events
                    .iter()
                    .any(|event| matches!(event, ReplayEvent::LineClear(_))),
            ),
            field(
                "drop_event_basis_available",
                events
                    .iter()
                    .any(|event| matches!(event, ReplayEvent::Drop(_))),
            ),
            field(
                "spin_event_basis_available",
                events
                    .iter()
                    .any(|event| matches!(event, ReplayEvent::SpinBasis(_))),
            ),
        ]);
    } else {
        fields.extend([
            field("placement_event_available", false),
            field("clear_event_available", false),
            field("drop_event_basis_available", false),
            field("spin_event_basis_available", false),
        ]);
    }
    fields
}

fn score_summary_fields(
    input: PcScoringPostProcessInput<'_>,
    matrix: &ScoreMatrix,
) -> Vec<(String, String)> {
    let Some(weights) = materialized_weights(input.pattern_weights, input.pattern_count) else {
        return vec![
            field("score_summary_complete", false),
            field(
                "score_summary_incomplete_reason",
                "pattern_weight_model_not_materialized",
            ),
        ];
    };
    if !matrix.complete() {
        return vec![
            field("score_summary_complete", false),
            field(
                "score_summary_incomplete_reason",
                matrix
                    .incomplete_reason()
                    .unwrap_or("score_matrix_incomplete"),
            ),
        ];
    }

    let best = matrix.highest_legal_cells_by_pattern();
    let totals = weighted_score_totals(&best, &weights);
    let conditional_average = (totals.covered_probability > 0.0)
        .then(|| totals.expected_score / totals.covered_probability);
    let mut fields = vec![
        field("score_summary_complete", true),
        field("score_summary_incomplete_reason", "none"),
        field(
            "score_all_universe_patterns_covered",
            best.len() == input.pattern_count,
        ),
        field("score_pattern_optimal_count", best.len()),
        field(
            "score_failed_pc_pattern_count",
            input.pattern_count.saturating_sub(best.len()),
        ),
        field("score_failed_pc_pattern_score", 0),
        field(
            "score_field_average_basis",
            "all-materialized-patterns-failed-pc-zero",
        ),
        field("score_covered_probability", totals.covered_probability),
        field("score_field_average_score", totals.expected_score),
        field("score_unconditional_expected_score", totals.expected_score),
        field(
            "score_unconditional_expected_attack",
            totals.expected_attack,
        ),
    ];
    if let Some(value) = conditional_average {
        fields.push(field(
            "score_covered_pattern_conditional_average_score",
            value,
        ));
    }
    fields
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct WeightedScoreTotals {
    covered_probability: f64,
    expected_score: f64,
    expected_attack: f64,
}

fn weighted_score_totals(
    cells: &[&crate::ScoreCell],
    weights: &WeightedPatternSet,
) -> WeightedScoreTotals {
    cells
        .iter()
        .fold(WeightedScoreTotals::default(), |mut totals, cell| {
            if let Some(weight) = weights.weight(PatternId::new(cell.pattern_id())) {
                totals.covered_probability += weight.get();
                totals.expected_score += weight.get() * cell.score() as f64;
                totals.expected_attack += weight.get() * f64::from(cell.attack());
            }
            totals
        })
}

fn materialized_weights(values: &[f64], pattern_count: usize) -> Option<WeightedPatternSet> {
    if values.len() != pattern_count || pattern_count == 0 {
        return None;
    }
    let total_weight = values.iter().try_fold(0.0, |total, value| {
        value.is_finite().then_some(total + value)
    })?;
    if (total_weight - 1.0).abs() > 1.0e-8 {
        return None;
    }
    let weights = values
        .iter()
        .copied()
        .map(ProbabilityValue::new)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    WeightedPatternSet::new(weights).ok()
}

fn field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guarded_fixture() -> (
        PcScoringPostProcessInput<'static>,
        ScoreProfile,
        u128,
        ScoreMatrix,
    ) {
        static WEIGHTS: [f64; 2] = [0.25, 0.75];
        let policy = ScoreObjectivePolicy::summary();
        let profile_projection =
            crate::checked_score_profile_memory_projection(policy).expect("profile projection");
        let (profile, profile_report) = crate::score_profile_with_memory_guard(
            policy,
            0,
            profile_projection.required_memory_bytes,
        )
        .expect("guarded profile");
        let accuracy = profile.accuracy_level().as_str();
        let matrix = ScoreMatrix::from_materialized_cells(
            vec![
                ScoreCell::new_with_static_accuracy(1, 0, "trace-a", 100, 1, accuracy),
                ScoreCell::new_with_static_accuracy(2, 1, "trace-b", 300, 3, accuracy),
            ],
            &profile,
            2,
            true,
        );
        (
            PcScoringPostProcessInput::new(None, &[], &WEIGHTS, 2, true, policy, true, "1", 2),
            profile,
            profile_report.retained_bytes,
            matrix,
        )
    }

    #[test]
    fn pc_scoring_postprocess_is_owned_outside_core_executor() {
        let fields = PcScoringPostProcessor::fields(PcScoringPostProcessInput::new(
            None,
            &[],
            &[],
            0,
            false,
            ScoreObjectivePolicy::DISABLED,
            true,
            "0.625",
            0,
        ));

        assert_eq!(value(&fields, "score_post_processing"), Some("false"));
        assert_eq!(value(&fields, "score_core_hot_path"), Some("false"));
        assert_eq!(
            value(&fields, "score_postprocess_owner"),
            Some("clearra-postprocess")
        );
        assert_eq!(value(&fields, "score_probability_before"), Some("0.625"));
        assert_eq!(value(&fields, "score_probability_after"), Some("0.625"));
        assert_eq!(value(&fields, "score_best_score"), None);
    }

    #[test]
    fn guarded_materialized_processor_matches_compatibility_fields_at_exact_cap() {
        let (input, profile, profile_retained_bytes, matrix) = guarded_fixture();
        let legacy = PcScoringPostProcessor::process_materialized_with_control(
            input,
            matrix.clone(),
            &ExecutionControl::default(),
        )
        .expect("legacy materialized fields");
        let projection = PcScoringPostProcessor::checked_materialized_memory_projection(
            input,
            &profile,
            profile_retained_bytes,
            &matrix,
        )
        .expect("checked guarded projection");
        let already_retained_bytes = 13;
        let exact_cap = already_retained_bytes + projection.required_peak_bytes;
        let (guarded, report) = PcScoringPostProcessor::process_materialized_with_memory_guard(
            input,
            &profile,
            profile_retained_bytes,
            matrix,
            &ExecutionControl::default(),
            already_retained_bytes,
            exact_cap,
        )
        .expect("exact guarded cap");

        assert_eq!(guarded, legacy);
        assert_eq!(report.projection, projection);
        assert!(
            report.result_retained_bytes
                <= projection.field_outer_storage_bytes + projection.field_string_storage_bytes
        );
        assert_eq!(
            projection.weight_storage_bytes,
            2 * core::mem::size_of::<ProbabilityValue>() as u128
        );
        assert_eq!(
            projection.highest_cell_storage_bytes,
            2 * core::mem::size_of::<&ScoreCell>() as u128
        );
    }

    #[test]
    fn guarded_materialized_processor_rejects_one_byte_under_before_work() {
        let (input, profile, profile_retained_bytes, matrix) = guarded_fixture();
        let projection = PcScoringPostProcessor::checked_materialized_memory_projection(
            input,
            &profile,
            profile_retained_bytes,
            &matrix,
        )
        .expect("projection");
        let error = PcScoringPostProcessor::process_materialized_with_memory_guard(
            input,
            &profile,
            profile_retained_bytes,
            matrix,
            &ExecutionControl::default(),
            0,
            projection.required_peak_bytes - 1,
        )
        .expect_err("one byte under");
        assert_eq!(
            error,
            PcScoringMemoryGuardError::LimitExceeded {
                required_memory_bytes: projection.required_peak_bytes,
                max_memory_bytes: projection.required_peak_bytes - 1,
            }
        );
    }

    #[test]
    fn guarded_materialized_processor_preserves_incomplete_field_branches() {
        let (input, profile, profile_retained_bytes, matrix) = guarded_fixture();
        let mut invalid_weights = input;
        invalid_weights.pattern_weights = &[];
        assert_guarded_matches_compatibility(
            invalid_weights,
            &profile,
            profile_retained_bytes,
            matrix.clone(),
        );

        let incomplete = ScoreMatrix::from_materialized_cells(
            matrix.cells().to_vec(),
            &profile,
            matrix.pattern_count(),
            false,
        );
        assert_guarded_matches_compatibility(input, &profile, profile_retained_bytes, incomplete);
    }

    #[test]
    fn guarded_materialized_processor_distinguishes_cancel_overflow_and_allocation_failure() {
        let (input, profile, profile_retained_bytes, matrix) = guarded_fixture();
        let cancellation =
            clearra_core_domain::execution_cancellation::ExecutionCancellationToken::new();
        cancellation.handle().cancel();
        let cancelled = PcScoringPostProcessor::process_materialized_with_memory_guard(
            input,
            &profile,
            profile_retained_bytes,
            matrix.clone(),
            &ExecutionControl::new(cancellation),
            0,
            u128::MAX,
        )
        .expect_err("cancelled");
        assert_eq!(cancelled, PcScoringMemoryGuardError::Cancelled);

        let overflow = PcScoringPostProcessor::process_materialized_with_memory_guard(
            input,
            &profile,
            profile_retained_bytes,
            matrix,
            &ExecutionControl::default(),
            u128::MAX,
            u128::MAX,
        )
        .expect_err("retained plus projection overflow");
        assert_eq!(overflow, PcScoringMemoryGuardError::ProjectionOverflow);

        let control = ExecutionControl::default();
        let allocation = GuardedFieldSink::with_capacity(usize::MAX, &control);
        assert!(matches!(
            allocation,
            Err(PcScoringMemoryGuardError::AllocationFailed)
        ));
    }

    fn value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
        fields
            .iter()
            .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
    }

    fn assert_guarded_matches_compatibility(
        input: PcScoringPostProcessInput<'_>,
        profile: &ScoreProfile,
        profile_retained_bytes: u128,
        matrix: ScoreMatrix,
    ) {
        let legacy = PcScoringPostProcessor::process_materialized_with_control(
            input,
            matrix.clone(),
            &ExecutionControl::default(),
        )
        .expect("compatibility fields");
        let projection = PcScoringPostProcessor::checked_materialized_memory_projection(
            input,
            profile,
            profile_retained_bytes,
            &matrix,
        )
        .expect("projection");
        let (guarded, _) = PcScoringPostProcessor::process_materialized_with_memory_guard(
            input,
            profile,
            profile_retained_bytes,
            matrix,
            &ExecutionControl::default(),
            0,
            projection.required_peak_bytes,
        )
        .expect("guarded fields");
        assert_eq!(guarded, legacy);
    }
}
