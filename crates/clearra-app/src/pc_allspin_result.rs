use clearra_core_executor::CoreExecutionResult;

use crate::pc_result_projection::{
    PcResultProblemProvenance, PcResultProjection, ValidatedPcResultProjection,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcAllSpinWitness {
    candidate_key: String,
    pattern_index: usize,
}

impl PcAllSpinWitness {
    pub fn candidate_key(&self) -> &str {
        &self.candidate_key
    }

    pub const fn pattern_index(&self) -> usize {
        self.pattern_index
    }

    pub const fn deterministic(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcAllSpinResultReport {
    projection: PcResultProjection,
    problem_preset: Option<String>,
    preserving_queue_count: Option<usize>,
    original_queue_count: Option<usize>,
    preservation_probability: Option<String>,
    count_complete: bool,
    probability_complete: bool,
    complete: bool,
    preserves_back_to_back: Option<bool>,
    witness: Option<PcAllSpinWitness>,
    incomplete_reasons: Vec<&'static str>,
}

impl PcAllSpinResultReport {
    pub fn from_execution_result(
        result: &CoreExecutionResult,
        projection: PcResultProjection,
    ) -> Option<Self> {
        Self::from_execution_result_inner(result, projection, None)
    }

    pub(crate) fn from_execution_result_with_provenance(
        result: &CoreExecutionResult,
        projection: PcResultProjection,
        provenance: PcResultProblemProvenance,
    ) -> Option<Self> {
        Self::from_execution_result_inner(result, projection, Some(provenance))
    }

    fn from_execution_result_inner(
        result: &CoreExecutionResult,
        projection: PcResultProjection,
        provenance: Option<PcResultProblemProvenance>,
    ) -> Option<Self> {
        let profile = projection.spin_profile()?;
        let mut incomplete_reasons = Vec::new();

        let declared_problem_preset = result.field("problem_preset");
        let problem_preset = provenance
            .map(PcResultProblemProvenance::problem_preset)
            .or_else(|| {
                declared_problem_preset
                    .filter(|preset| matches!(*preset, "opening-pc" | "scenario-pc"))
            })
            .map(ToOwned::to_owned);
        if let Some(provenance) = provenance {
            if declared_problem_preset.is_some_and(|preset| preset != provenance.problem_preset()) {
                push_reason(&mut incomplete_reasons, "problem-preset-mismatch");
            }
            if result
                .field("compiled_goal")
                .is_some_and(|goal| goal != "clear-to-empty")
            {
                push_reason(&mut incomplete_reasons, "not-clear-to-empty");
            }
        } else {
            if problem_preset.is_none() {
                push_reason(&mut incomplete_reasons, "unsupported-problem-preset");
            }
            require_field(
                result,
                "compiled_goal",
                "clear-to-empty",
                "not-clear-to-empty",
                &mut incomplete_reasons,
            );
        }
        require_bool(
            result,
            "execution_constraint_preserve_b2b",
            true,
            "preserve-b2b-not-requested",
            &mut incomplete_reasons,
        );
        require_bool(
            result,
            "execution_constraint_materialized",
            true,
            "preserve-b2b-not-materialized",
            &mut incomplete_reasons,
        );
        require_field(
            result,
            "execution_constraint_spin_profile",
            profile.as_str(),
            "spin-profile-mismatch",
            &mut incomplete_reasons,
        );
        require_field(
            result,
            "b2b_preservation_selection",
            "existential",
            "non-existential-selection",
            &mut incomplete_reasons,
        );
        require_field(
            result,
            "b2b_preservation_denominator_semantics",
            "original-materialized-queue",
            "denominator-semantics-mismatch",
            &mut incomplete_reasons,
        );
        require_field(
            result,
            "b2b_preservation_evaluation_basis",
            "candidate-pattern-existence",
            "evaluation-basis-mismatch",
            &mut incomplete_reasons,
        );
        require_bool(
            result,
            "b2b_preservation_path_multiplicity_counted",
            false,
            "path-multiplicity-counted",
            &mut incomplete_reasons,
        );
        require_bool(
            result,
            "postprocess_scoring_requested",
            false,
            "score-selection-requested",
            &mut incomplete_reasons,
        );
        require_field(
            result,
            "score_objective_mode",
            "disabled",
            "score-selection-requested",
            &mut incomplete_reasons,
        );

        let preserving_queue_count = result.usize_field("b2b_preserving_pattern_count");
        let original_queue_count = result.usize_field("b2b_preservation_pattern_universe_count");
        let preservation_probability = result
            .field("b2b_preservation_probability")
            .and_then(valid_probability)
            .map(ToOwned::to_owned);
        let count_complete = result.bool_field("b2b_preservation_count_complete") == Some(true);
        let probability_complete =
            result.bool_field("b2b_preservation_probability_complete") == Some(true);

        if preserving_queue_count.is_none() {
            push_reason(&mut incomplete_reasons, "preserving-count-missing");
        }
        if original_queue_count.is_none() {
            push_reason(&mut incomplete_reasons, "original-universe-count-missing");
        }
        if preservation_probability.is_none() {
            push_reason(&mut incomplete_reasons, "preservation-probability-missing");
        }
        if !count_complete {
            push_reason(&mut incomplete_reasons, "preserving-count-incomplete");
        }
        if !probability_complete {
            push_reason(
                &mut incomplete_reasons,
                "preservation-probability-incomplete",
            );
        }
        if original_queue_count == Some(0) {
            push_reason(&mut incomplete_reasons, "original-universe-empty");
        }
        if matches!(
            (preserving_queue_count, original_queue_count),
            (Some(preserving), Some(original)) if preserving > original
        ) {
            push_reason(&mut incomplete_reasons, "preserving-count-exceeds-universe");
        }
        if let (Some(preserving), Some(original), Some(probability)) = (
            preserving_queue_count,
            original_queue_count,
            preservation_probability
                .as_deref()
                .and_then(|value| value.parse::<f64>().ok()),
        ) {
            let probability_mismatch = if preserving == 0 {
                probability != 0.0
            } else if preserving == original {
                probability != 1.0
            } else if preserving < original {
                !(probability > 0.0 && probability < 1.0)
            } else {
                false
            };
            if probability_mismatch {
                push_reason(
                    &mut incomplete_reasons,
                    if matches!(projection, PcResultProjection::AllSpinSolution(_)) {
                        "exact-queue-probability-mismatch"
                    } else {
                        "preservation-probability-count-mismatch"
                    },
                );
            }
        }

        let mut witness = None;
        let mut preserves_back_to_back = None;
        if matches!(projection, PcResultProjection::AllSpinSolution(_)) {
            if original_queue_count != Some(1) {
                push_reason(&mut incomplete_reasons, "exact-queue-universe-not-one");
            }
            if preserving_queue_count.is_some_and(|count| count > 1) {
                push_reason(
                    &mut incomplete_reasons,
                    "exact-queue-preserving-count-invalid",
                );
            }
            let availability = result.execution_report().solution_set_availability();
            if !availability.uses_explicit_contract()
                || !availability.contract_valid()
                || !availability
                    .materialized_key_count_matches(result.normalized_solution_keys().len())
            {
                push_reason(&mut incomplete_reasons, "solution-set-not-materialized");
            }

            match preserving_queue_count {
                Some(1) => {
                    if result.usize_field("b2b_preserving_solution_count") == Some(0)
                        || result
                            .usize_field("b2b_preserving_solution_count")
                            .is_none()
                    {
                        push_reason(&mut incomplete_reasons, "preserving-solution-missing");
                    }
                    let candidate_key = result
                        .field("b2b_preservation_witness_candidate_key")
                        .filter(|value| !value.is_empty());
                    let pattern_index =
                        result.usize_field("b2b_preservation_witness_pattern_index");
                    require_bool(
                        result,
                        "b2b_preservation_witness_available",
                        true,
                        "deterministic-witness-missing",
                        &mut incomplete_reasons,
                    );
                    require_field(
                        result,
                        "b2b_preservation_witness_kind",
                        "candidate-pattern",
                        "witness-kind-mismatch",
                        &mut incomplete_reasons,
                    );
                    require_field(
                        result,
                        "b2b_preservation_witness_pattern_semantics",
                        "original-queue-index",
                        "witness-pattern-semantics-mismatch",
                        &mut incomplete_reasons,
                    );
                    if pattern_index != Some(0) {
                        push_reason(&mut incomplete_reasons, "witness-pattern-index-invalid");
                    }
                    let deterministic_key = result.normalized_solution_keys().iter().min();
                    if candidate_key.is_none()
                        || candidate_key != deterministic_key.map(String::as_str)
                    {
                        push_reason(&mut incomplete_reasons, "witness-not-deterministic");
                    }
                    if let (Some(candidate_key), Some(pattern_index)) =
                        (candidate_key, pattern_index)
                    {
                        witness = Some(PcAllSpinWitness {
                            candidate_key: candidate_key.to_owned(),
                            pattern_index,
                        });
                    }
                }
                Some(0) => {
                    if result.usize_field("b2b_preserving_solution_count") != Some(0)
                        || !result.normalized_solution_keys().is_empty()
                    {
                        push_reason(&mut incomplete_reasons, "zero-result-solution-set-mismatch");
                    }
                    require_bool(
                        result,
                        "b2b_preservation_witness_available",
                        false,
                        "unexpected-witness",
                        &mut incomplete_reasons,
                    );
                    require_field(
                        result,
                        "b2b_preservation_witness_kind",
                        "none",
                        "unexpected-witness",
                        &mut incomplete_reasons,
                    );
                }
                _ => {}
            }
        }

        let complete = incomplete_reasons.is_empty();
        if complete && matches!(projection, PcResultProjection::AllSpinSolution(_)) {
            preserves_back_to_back = preserving_queue_count.map(|count| count == 1);
        } else if !complete {
            witness = None;
        }

        Some(Self {
            projection,
            problem_preset,
            preserving_queue_count,
            original_queue_count,
            preservation_probability,
            count_complete,
            probability_complete,
            complete,
            preserves_back_to_back,
            witness,
            incomplete_reasons,
        })
    }

    pub const fn projection(&self) -> PcResultProjection {
        self.projection
    }

    pub fn problem_preset(&self) -> Option<&str> {
        self.problem_preset.as_deref()
    }

    pub const fn preserving_queue_count(&self) -> Option<usize> {
        self.preserving_queue_count
    }

    pub const fn original_queue_count(&self) -> Option<usize> {
        self.original_queue_count
    }

    pub fn preservation_probability(&self) -> Option<&str> {
        self.preservation_probability.as_deref()
    }

    pub const fn count_complete(&self) -> bool {
        self.count_complete
    }

    pub const fn probability_complete(&self) -> bool {
        self.probability_complete
    }

    pub const fn complete(&self) -> bool {
        self.complete
    }

    pub const fn preserves_back_to_back(&self) -> Option<bool> {
        self.preserves_back_to_back
    }

    pub fn witness(&self) -> Option<&PcAllSpinWitness> {
        self.witness.as_ref()
    }

    pub fn incomplete_reasons(&self) -> &[&'static str] {
        &self.incomplete_reasons
    }

    pub fn summary_fields(&self) -> Vec<(String, String)> {
        let profile = self
            .projection
            .spin_profile()
            .expect("All-Spin reports require a selected spin profile");
        let mut fields = vec![
            field(
                "pc_allspin_result_contract",
                self.projection
                    .contract_id()
                    .expect("All-Spin reports require a result contract"),
            ),
            field(
                "pc_allspin_mode",
                self.projection
                    .mode()
                    .expect("All-Spin reports require a projection mode"),
            ),
            field("pc_allspin_spin_profile", profile.as_str()),
            field(
                "pc_allspin_problem_preset",
                self.problem_preset.as_deref().unwrap_or("not-calculated"),
            ),
            field(
                "pc_allspin_initial_field_supplied",
                self.problem_preset.as_deref().map_or_else(
                    || "not-calculated".to_owned(),
                    |preset| (preset == "scenario-pc").to_string(),
                ),
            ),
            field("pc_allspin_target_field_supplied", false),
            field("pc_allspin_clear_contract", "inverse-lock-clear-to-empty"),
            field("pc_allspin_semantics", "clearra-explicit-spin-profile"),
            field("pc_allspin_compatibility", "sfinderbot-command-intent-only"),
            field("pc_allspin_complete", self.complete),
            field(
                "pc_allspin_incomplete_reason",
                if self.incomplete_reasons.is_empty() {
                    "none".to_owned()
                } else {
                    self.incomplete_reasons.join(",")
                },
            ),
            field(
                "pc_allspin_denominator_semantics",
                "original-materialized-queue",
            ),
            field("pc_allspin_evaluation_basis", "candidate-pattern-existence"),
            field("pc_allspin_path_multiplicity_counted", false),
            optional_field(
                "pc_allspin_preserving_queue_count",
                self.preserving_queue_count,
            ),
            optional_field("pc_allspin_original_queue_count", self.original_queue_count),
            field(
                "pc_allspin_preservation_probability",
                self.preservation_probability
                    .clone()
                    .unwrap_or_else(|| "not-calculated".to_owned()),
            ),
            field("pc_allspin_count_complete", self.count_complete),
            field("pc_allspin_probability_complete", self.probability_complete),
        ];

        if matches!(self.projection, PcResultProjection::AllSpinSolution(_)) {
            fields.extend([
                field(
                    "pc_allspin_preserves_b2b",
                    self.preserves_back_to_back
                        .map_or_else(|| "not-calculated".to_owned(), |value| value.to_string()),
                ),
                field(
                    "pc_allspin_witness_required",
                    self.preserving_queue_count == Some(1),
                ),
                field("pc_allspin_witness_available", self.witness.is_some()),
                field(
                    "pc_allspin_witness_deterministic",
                    self.witness
                        .as_ref()
                        .is_some_and(PcAllSpinWitness::deterministic),
                ),
                field(
                    "pc_allspin_witness_kind",
                    self.witness
                        .as_ref()
                        .map_or("none", |_| "candidate-pattern"),
                ),
                field(
                    "pc_allspin_witness_candidate_key",
                    self.witness
                        .as_ref()
                        .map_or("not-materialized", PcAllSpinWitness::candidate_key),
                ),
                field(
                    "pc_allspin_witness_pattern_index",
                    self.witness.as_ref().map_or_else(
                        || "not-materialized".to_owned(),
                        |witness| witness.pattern_index().to_string(),
                    ),
                ),
            ]);
        }
        fields
    }
}

pub(crate) fn project_pc_allspin_result(
    result: CoreExecutionResult,
    validated: ValidatedPcResultProjection,
) -> CoreExecutionResult {
    let Some(report) = PcAllSpinResultReport::from_execution_result_with_provenance(
        &result,
        validated.projection(),
        validated.provenance(),
    ) else {
        return result;
    };
    result.with_replaced_fields(report.summary_fields())
}

fn valid_probability(value: &str) -> Option<&str> {
    let parsed = value.parse::<f64>().ok()?;
    (parsed.is_finite() && (0.0..=1.0).contains(&parsed)).then_some(value)
}

fn require_field(
    result: &CoreExecutionResult,
    key: &str,
    expected: &str,
    reason: &'static str,
    reasons: &mut Vec<&'static str>,
) {
    if result.field(key) != Some(expected) {
        push_reason(reasons, reason);
    }
}

fn require_bool(
    result: &CoreExecutionResult,
    key: &str,
    expected: bool,
    reason: &'static str,
    reasons: &mut Vec<&'static str>,
) {
    if result.bool_field(key) != Some(expected) {
        push_reason(reasons, reason);
    }
}

fn push_reason(reasons: &mut Vec<&'static str>, reason: &'static str) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}

fn field(key: &str, value: impl ToString) -> (String, String) {
    (key.to_owned(), value.to_string())
}

fn optional_field(key: &str, value: Option<usize>) -> (String, String) {
    field(
        key,
        value.map_or_else(|| "not-calculated".to_owned(), |value| value.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use clearra_core_executor::CoreExecutionResult;
    use clearra_objectives::policy::score_objective_policy::SpinProfileSelection;

    use super::{
        project_pc_allspin_result, PcAllSpinResultReport, PcResultProblemProvenance,
        PcResultProjection, ValidatedPcResultProjection,
    };

    fn validated(
        projection: PcResultProjection,
        provenance: PcResultProblemProvenance,
    ) -> ValidatedPcResultProjection {
        ValidatedPcResultProjection::new_for_test(projection, provenance)
    }

    fn source_result(
        original: usize,
        preserving: usize,
        probability: &str,
        normalized_keys: &[&str],
        witness: Option<(&str, usize)>,
    ) -> CoreExecutionResult {
        let witness_available = witness.is_some();
        CoreExecutionResult::new(
            vec![
                ("problem_preset".to_owned(), "opening-pc".to_owned()),
                ("compiled_goal".to_owned(), "clear-to-empty".to_owned()),
                ("search_output_policy".to_owned(), "summary".to_owned()),
                (
                    "unique_solution_count".to_owned(),
                    normalized_keys.len().to_string(),
                ),
                (
                    "normalized_unique_solution_count".to_owned(),
                    normalized_keys.len().to_string(),
                ),
                ("solution_count_calculated".to_owned(), "true".to_owned()),
                ("solution_set_materialized".to_owned(), "true".to_owned()),
                (
                    "solution_keys_materialized_count".to_owned(),
                    normalized_keys.len().to_string(),
                ),
                ("solution_keys_complete".to_owned(), "true".to_owned()),
                ("solution_page_available".to_owned(), "false".to_owned()),
                (
                    "execution_constraint_preserve_b2b".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "execution_constraint_materialized".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "execution_constraint_spin_profile".to_owned(),
                    "all-spin-plus".to_owned(),
                ),
                (
                    "postprocess_scoring_requested".to_owned(),
                    "false".to_owned(),
                ),
                ("score_objective_mode".to_owned(), "disabled".to_owned()),
                (
                    "b2b_preservation_selection".to_owned(),
                    "existential".to_owned(),
                ),
                (
                    "b2b_preservation_denominator_semantics".to_owned(),
                    "original-materialized-queue".to_owned(),
                ),
                (
                    "b2b_preservation_pattern_universe_count".to_owned(),
                    original.to_string(),
                ),
                (
                    "b2b_preserving_pattern_count".to_owned(),
                    preserving.to_string(),
                ),
                (
                    "b2b_preservation_probability".to_owned(),
                    probability.to_owned(),
                ),
                (
                    "b2b_preservation_count_complete".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "b2b_preservation_probability_complete".to_owned(),
                    "true".to_owned(),
                ),
                (
                    "b2b_preservation_witness_available".to_owned(),
                    witness_available.to_string(),
                ),
                (
                    "b2b_preservation_witness_kind".to_owned(),
                    if witness_available {
                        "candidate-pattern"
                    } else {
                        "none"
                    }
                    .to_owned(),
                ),
                (
                    "b2b_preservation_witness_pattern_semantics".to_owned(),
                    "original-queue-index".to_owned(),
                ),
                (
                    "b2b_preservation_witness_candidate_key".to_owned(),
                    witness.map_or("", |(key, _)| key).to_owned(),
                ),
                (
                    "b2b_preservation_witness_pattern_index".to_owned(),
                    witness.map_or_else(String::new, |(_, index)| index.to_string()),
                ),
                (
                    "b2b_preservation_evaluation_basis".to_owned(),
                    "candidate-pattern-existence".to_owned(),
                ),
                (
                    "b2b_preservation_path_multiplicity_counted".to_owned(),
                    "false".to_owned(),
                ),
                (
                    "b2b_preserving_solution_count".to_owned(),
                    normalized_keys.len().to_string(),
                ),
            ],
            Vec::new(),
        )
        .with_normalized_solution_keys(
            normalized_keys
                .iter()
                .map(|key| (*key).to_owned())
                .collect(),
        )
    }

    #[test]
    fn exact_queue_requires_one_original_queue_and_deterministic_witness() {
        let result = source_result(1, 1, "1", &["ctk1:a", "ctk1:z"], Some(("ctk1:a", 0)));
        let report = PcAllSpinResultReport::from_execution_result(
            &result,
            PcResultProjection::AllSpinSolution(SpinProfileSelection::AllSpinPlus),
        )
        .expect("projection should produce a report");

        assert!(report.complete());
        assert_eq!(report.original_queue_count(), Some(1));
        assert_eq!(report.preserving_queue_count(), Some(1));
        assert_eq!(report.preserves_back_to_back(), Some(true));
        assert_eq!(
            report.witness().map(|witness| witness.candidate_key()),
            Some("ctk1:a")
        );
        assert_eq!(
            report.witness().map(|witness| witness.pattern_index()),
            Some(0)
        );

        let projected = project_pc_allspin_result(
            result,
            validated(
                PcResultProjection::AllSpinSolution(SpinProfileSelection::AllSpinPlus),
                PcResultProblemProvenance::Opening,
            ),
        );
        assert_eq!(
            projected.field("pc_allspin_result_contract"),
            Some("pc-b2b-preserving-witness.v1")
        );
        assert_eq!(
            projected.usize_field("pc_allspin_original_queue_count"),
            Some(1)
        );
        assert_eq!(
            projected.field("pc_allspin_witness_candidate_key"),
            Some("ctk1:a")
        );
    }

    #[test]
    fn exact_queue_fails_closed_for_non_singleton_universe_or_witness_drift() {
        let result = source_result(2, 1, "0.5", &["ctk1:a", "ctk1:z"], Some(("ctk1:z", 1)));
        let report = PcAllSpinResultReport::from_execution_result(
            &result,
            PcResultProjection::AllSpinSolution(SpinProfileSelection::AllSpinPlus),
        )
        .expect("projection should produce a report");

        assert!(!report.complete());
        assert_eq!(report.preserves_back_to_back(), None);
        assert!(report.witness().is_none());
        assert!(report
            .incomplete_reasons()
            .contains(&"exact-queue-universe-not-one"));
        assert!(report
            .incomplete_reasons()
            .contains(&"witness-not-deterministic"));
    }

    #[test]
    fn exact_zero_preservation_is_complete_without_a_witness() {
        let result = source_result(1, 0, "0", &[], None);
        let report = PcAllSpinResultReport::from_execution_result(
            &result,
            PcResultProjection::AllSpinSolution(SpinProfileSelection::AllSpinPlus),
        )
        .expect("projection should produce a report");

        assert!(report.complete());
        assert_eq!(report.preserves_back_to_back(), Some(false));
        assert!(report.witness().is_none());
    }

    #[test]
    fn pattern_probability_preserves_source_numerator_denominator_and_probability() {
        let result = source_result(4, 3, "0.625", &["ctk1:a"], Some(("ctk1:a", 0)));
        let report = PcAllSpinResultReport::from_execution_result(
            &result,
            PcResultProjection::AllSpinPreservationChance(SpinProfileSelection::AllSpinPlus),
        )
        .expect("projection should produce a report");

        assert!(report.complete());
        assert_eq!(report.original_queue_count(), Some(4));
        assert_eq!(report.preserving_queue_count(), Some(3));
        assert_eq!(report.preservation_probability(), Some("0.625"));
        assert!(report.witness().is_none());

        let projected = project_pc_allspin_result(
            result,
            validated(
                PcResultProjection::AllSpinPreservationChance(SpinProfileSelection::AllSpinPlus),
                PcResultProblemProvenance::Opening,
            ),
        );
        assert_eq!(
            projected.field("pc_allspin_result_contract"),
            Some("pc-b2b-preservation-probability.v1")
        );
        assert_eq!(
            projected.usize_field("pc_allspin_preserving_queue_count"),
            Some(3)
        );
        assert_eq!(
            projected.usize_field("pc_allspin_original_queue_count"),
            Some(4)
        );
        assert_eq!(
            projected.field("pc_allspin_preservation_probability"),
            Some("0.625")
        );
    }

    #[test]
    fn pattern_probability_rejects_count_extreme_and_probability_contradictions() {
        for (preserving, probability) in [(0, "0.5"), (4, "0.5"), (2, "0"), (2, "1")] {
            let result = source_result(4, preserving, probability, &[], None);
            let report = PcAllSpinResultReport::from_execution_result(
                &result,
                PcResultProjection::AllSpinPreservationChance(SpinProfileSelection::AllSpinPlus),
            )
            .expect("projection should produce a report");

            assert!(!report.complete(), "{preserving}/4/{probability}");
            assert!(
                report
                    .incomplete_reasons()
                    .contains(&"preservation-probability-count-mismatch"),
                "{preserving}/4/{probability}"
            );
            let projected = project_pc_allspin_result(
                result,
                validated(
                    PcResultProjection::AllSpinPreservationChance(
                        SpinProfileSelection::AllSpinPlus,
                    ),
                    PcResultProblemProvenance::Opening,
                ),
            );
            assert_eq!(
                projected.bool_field("pc_allspin_complete"),
                Some(false),
                "{preserving}/4/{probability}"
            );
        }

        for (preserving, probability) in [(0, "0"), (2, "0.25"), (4, "1")] {
            let result = source_result(4, preserving, probability, &[], None);
            let report = PcAllSpinResultReport::from_execution_result(
                &result,
                PcResultProjection::AllSpinPreservationChance(SpinProfileSelection::AllSpinPlus),
            )
            .expect("projection should produce a report");

            assert!(report.complete(), "{preserving}/4/{probability}");
        }
    }

    #[test]
    fn scenario_pc_provenance_is_typed_as_an_initial_field_not_a_target_field() {
        let result = source_result(1, 0, "0", &[], None).with_replaced_fields(vec![(
            "problem_preset".to_owned(),
            "scenario-pc".to_owned(),
        )]);
        let projected = project_pc_allspin_result(
            result,
            validated(
                PcResultProjection::AllSpinSolution(SpinProfileSelection::AllSpinPlus),
                PcResultProblemProvenance::InitialFieldScenario,
            ),
        );

        assert_eq!(projected.bool_field("pc_allspin_complete"), Some(true));
        assert_eq!(
            projected.field("pc_allspin_problem_preset"),
            Some("scenario-pc")
        );
        assert_eq!(
            projected.bool_field("pc_allspin_initial_field_supplied"),
            Some(true)
        );
        assert_eq!(
            projected.bool_field("pc_allspin_target_field_supplied"),
            Some(false)
        );
    }

    #[test]
    fn typed_command_provenance_completes_sparse_runtime_metadata_without_string_inference() {
        let source = source_result(1, 1, "1", &["ctk1:a"], Some(("ctk1:a", 0)));
        let normalized_keys = source.normalized_solution_keys().to_vec();
        let fields = source
            .summary_fields()
            .into_iter()
            .filter(|(key, _)| !matches!(key.as_str(), "problem_preset" | "compiled_goal"))
            .collect();
        let sparse = CoreExecutionResult::new(fields, Vec::new())
            .with_normalized_solution_keys(normalized_keys);
        let projected = project_pc_allspin_result(
            sparse,
            validated(
                PcResultProjection::AllSpinSolution(SpinProfileSelection::AllSpinPlus),
                PcResultProblemProvenance::Opening,
            ),
        );

        assert_eq!(projected.bool_field("pc_allspin_complete"), Some(true));
        assert_eq!(
            projected.field("pc_allspin_problem_preset"),
            Some("opening-pc")
        );
        assert_eq!(
            projected.bool_field("pc_allspin_initial_field_supplied"),
            Some(false)
        );
        assert_eq!(
            projected.field("pc_allspin_witness_candidate_key"),
            Some("ctk1:a")
        );
    }

    #[test]
    fn incomplete_probability_remains_incomplete_instead_of_becoming_zero() {
        let result = source_result(4, 0, "not-calculated", &[], None).with_replaced_fields(vec![(
            "b2b_preservation_probability_complete".to_owned(),
            "false".to_owned(),
        )]);
        let report = PcAllSpinResultReport::from_execution_result(
            &result,
            PcResultProjection::AllSpinPreservationChance(SpinProfileSelection::AllSpinPlus),
        )
        .expect("projection should produce a report");

        assert!(!report.complete());
        assert_eq!(report.preservation_probability(), None);
        assert!(!report.probability_complete());
        assert!(report
            .incomplete_reasons()
            .contains(&"preservation-probability-missing"));
        assert!(report
            .incomplete_reasons()
            .contains(&"preservation-probability-incomplete"));

        let projected = project_pc_allspin_result(
            result,
            validated(
                PcResultProjection::AllSpinPreservationChance(SpinProfileSelection::AllSpinPlus),
                PcResultProblemProvenance::Opening,
            ),
        );
        assert_eq!(projected.bool_field("pc_allspin_complete"), Some(false));
        assert_eq!(
            projected.field("pc_allspin_preservation_probability"),
            Some("not-calculated")
        );
    }

    #[test]
    fn standard_projection_does_not_modify_the_execution_result() {
        let result = source_result(1, 0, "0", &[], None);
        let projected = project_pc_allspin_result(
            result.clone(),
            validated(
                PcResultProjection::Standard,
                PcResultProblemProvenance::Opening,
            ),
        );
        assert_eq!(projected, result);
    }
}
