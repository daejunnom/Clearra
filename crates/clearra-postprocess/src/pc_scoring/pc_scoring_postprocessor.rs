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

use crate::{CandidateExecutionAggregate, ScoreMatrix};

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
        .max_by_key(|cell| (cell.score(), cell.attack()))
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

    fn value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
        fields
            .iter()
            .find_map(|(field_key, value)| (field_key == key).then_some(value.as_str()))
    }
}
