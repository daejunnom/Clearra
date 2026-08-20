use std::{fs, path::PathBuf};

use clearra_output::{
    model::{RenderFieldValue, RenderMessage},
    RenderFormat, RenderFormatDispatcher,
};

fn output_golden_path(file_name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden/output")
        .join(file_name)
}

fn read_output_golden(file_name: &str) -> String {
    fs::read_to_string(output_golden_path(file_name))
        .expect("output golden fixture")
        .trim()
        .to_owned()
}

fn render_json(message: &RenderMessage) -> String {
    RenderFormatDispatcher::render(message, RenderFormat::Json).expect("JSON golden")
}

fn spin_probability_message() -> RenderMessage {
    RenderMessage::new("spin-probability")
        .with_value("spin_target_id", "tsd")
        .with_value("spin_target_name", "T-spin Double")
        .with_value("covered_pattern_count", 42usize)
        .with_value("pattern_count", 64usize)
        .with_value("pattern_universe_id", "bag-universe:v1:opening")
        .with_value("pattern_weight_model_id", "7bag-uniform:v1")
        .with_value("probability", RenderFieldValue::number("0.65625"))
        .with_value("probability_complete", true)
        .with_value(
            "materialized_probability_mass",
            RenderFieldValue::number("1.0"),
        )
        .with_value("renormalized", false)
        .with_value("truncation_reason", RenderFieldValue::Null)
        .with_value("spin_accuracy", "exact")
        .with_value("trace_completeness", "complete")
        .with_value("score_profile_id", "tetrio")
}

fn score_expectation_message() -> RenderMessage {
    RenderMessage::new("score-expectation")
        .with_value("score_profile_id", "tetrio")
        .with_value("score_accuracy", "pattern-complete")
        .with_value("trace_completeness", "complete")
        .with_value("evaluation_scope", "full-pattern-universe-expected")
        .with_value(
            "retained_trace_average_score",
            RenderFieldValue::number("1200"),
        )
        .with_value(
            "covered_pattern_conditional_average_score",
            RenderFieldValue::number("980"),
        )
        .with_value(
            "unconditional_expected_score",
            RenderFieldValue::number("640"),
        )
        .with_value("best_score_by_pattern_available", true)
        .with_value("score_does_not_change_probability_union", true)
}

fn special_spin_diagnostic_message() -> RenderMessage {
    RenderMessage::new("special-spin-diagnostic")
        .with_value("special_spin_case_id", "fin")
        .with_value("verification_state", "descriptor-only")
        .with_value("kick_evidence_required", true)
        .with_value("kick_evidence_available", false)
        .with_value("classification_accuracy", "incomplete")
        .with_value("disabled_reason", "E_SPIN_PROFILE_UNVERIFIED")
}

mod case_json_spin_probability_includes_universe_identity {
    use super::*;

    #[test]
    fn json_spin_probability_includes_universe_identity() {
        let rendered = render_json(&spin_probability_message());

        assert_eq!(rendered, read_output_golden("spin_probability_result.json"));
        assert!(rendered.contains("\"pattern_universe_id\":\"bag-universe:v1:opening\""));
        assert!(rendered.contains("\"pattern_weight_model_id\":\"7bag-uniform:v1\""));
    }
}

mod case_json_score_result_distinguishes_evaluation_scope {
    use super::*;

    #[test]
    fn json_score_result_distinguishes_evaluation_scope() {
        let rendered = render_json(&score_expectation_message());

        assert_eq!(
            rendered,
            read_output_golden("score_expectation_result.json")
        );
        assert!(rendered.contains("\"evaluation_scope\":\"full-pattern-universe-expected\""));
        assert!(rendered.contains("\"covered_pattern_conditional_average_score\":980"));
    }
}

mod case_json_special_spin_diagnostic_reports_disabled_reason {
    use super::*;

    #[test]
    fn json_special_spin_diagnostic_reports_disabled_reason() {
        let rendered = render_json(&special_spin_diagnostic_message());

        assert_eq!(
            rendered,
            read_output_golden("special_spin_disabled_reason.json")
        );
        assert!(rendered.contains("\"disabled_reason\":\"E_SPIN_PROFILE_UNVERIFIED\""));
    }
}

mod case_json_probability_not_renormalized_after_observed_truncation {
    use super::*;

    #[test]
    fn json_probability_not_renormalized_after_observed_truncation() {
        let rendered = render_json(
            &RenderMessage::new("spin-probability")
                .with_value("spin_target_id", "tsd")
                .with_value("pattern_universe_id", "bag-universe:v1:opening")
                .with_value("pattern_weight_model_id", "7bag-uniform:v1")
                .with_value("probability", RenderFieldValue::number("0.5"))
                .with_value("probability_complete", false)
                .with_value(
                    "materialized_probability_mass",
                    RenderFieldValue::number("0.875"),
                )
                .with_value("renormalized", false)
                .with_value("truncation_reason", "observed_queue_truncated")
                .with_value("spin_accuracy", "exact")
                .with_value("trace_completeness", "pattern-truncated"),
        );

        assert!(rendered.contains("\"probability_complete\":false"));
        assert!(rendered.contains("\"materialized_probability_mass\":0.875"));
        assert!(rendered.contains("\"renormalized\":false"));
        assert!(rendered.contains("\"truncation_reason\":\"observed_queue_truncated\""));
    }
}

mod case_observed_queue_truncation_is_not_renormalized {
    use super::*;

    #[test]
    fn observed_queue_truncation_is_not_renormalized() {
        let rendered = render_json(
            &RenderMessage::new("spin-probability")
                .with_value("spin_target_id", "tsd")
                .with_value("pattern_universe_id", "bag-universe:v1:opening")
                .with_value("pattern_weight_model_id", "7bag-uniform:v1")
                .with_value("probability", RenderFieldValue::number("0.5"))
                .with_value("probability_complete", false)
                .with_value(
                    "materialized_probability_mass",
                    RenderFieldValue::number("0.875"),
                )
                .with_value("renormalized", false)
                .with_value("truncation_reason", "observed_queue_truncated"),
        );

        assert!(rendered.contains("\"probability_complete\":false"));
        assert!(rendered.contains("\"materialized_probability_mass\":0.875"));
        assert!(rendered.contains("\"renormalized\":false"));
        assert!(!rendered.contains("\"renormalized\":true"));
    }
}

mod case_observed_queue_truncation_not_renormalized {
    use super::*;

    #[test]
    fn observed_queue_truncation_not_renormalized() {
        let rendered = render_json(
            &RenderMessage::new("spin-probability")
                .with_value("spin_target_id", "tsd")
                .with_value("pattern_universe_id", "bag-universe:v1:opening")
                .with_value("pattern_weight_model_id", "7bag-uniform:v1")
                .with_value("probability", RenderFieldValue::number("0.5"))
                .with_value("probability_complete", false)
                .with_value(
                    "materialized_probability_mass",
                    RenderFieldValue::number("0.875"),
                )
                .with_value("renormalized", false)
                .with_value("truncation_reason", "observed_queue_truncated"),
        );

        assert!(rendered.contains("\"probability_complete\":false"));
        assert!(rendered.contains("\"renormalized\":false"));
        assert!(!rendered.contains("\"renormalized\":true"));
    }
}

mod case_output_distinguishes_total_solution_count_and_retained_trace_count {
    use super::*;

    #[test]
    fn output_distinguishes_total_solution_count_and_retained_trace_count() {
        let rendered = render_json(
            &RenderMessage::new("pc-scenario")
                .with_value("solution_found", true)
                .with_value("total_solution_count", 42usize)
                .with_value("unique_solution_count", 42usize)
                .with_value("retained_trace_count", 2usize)
                .with_value("count_complete", false)
                .with_value("count_truncated_reason", "buildup_variant_limit")
                .with_value("trace_retention_truncated", true)
                .with_value("trace_retention_reason", "retained_trace_limit")
                .with_value("coverage_probability", RenderFieldValue::number("0.75"))
                .with_value("probability_complete", false),
        );

        assert!(rendered.contains("\"total_solution_count\":42"));
        assert!(rendered.contains("\"unique_solution_count\":42"));
        assert!(rendered.contains("\"retained_trace_count\":2"));
        assert!(rendered.contains("\"count_complete\":false"));
        assert!(rendered.contains("\"count_truncated_reason\":\"buildup_variant_limit\""));
        assert!(rendered.contains("\"trace_retention_truncated\":true"));
        assert!(!rendered.contains("\"total_solution_count\":2"));
    }
}

mod case_json_output_contains_count_and_trace_separation {
    use super::*;

    #[test]
    fn json_output_contains_count_and_trace_separation() {
        let rendered = render_json(
            &RenderMessage::new("pc")
                .with_value("total_solution_count", 12usize)
                .with_value("unique_solution_count", 8usize)
                .with_value("retained_trace_count", 2usize)
                .with_value("count_complete", true)
                .with_value("trace_retention_truncated", true)
                .with_value("trace_retention_reason", "retained_trace_limit"),
        );

        assert!(rendered.contains("\"total_solution_count\":12"));
        assert!(rendered.contains("\"unique_solution_count\":8"));
        assert!(rendered.contains("\"retained_trace_count\":2"));
        assert!(rendered.contains("\"count_complete\":true"));
        assert!(rendered.contains("\"trace_retention_truncated\":true"));
        assert!(!rendered.contains("\"total_solution_count\":2"));
    }
}

mod case_text_output_marks_representative_trace {
    use super::*;

    #[test]
    fn text_output_marks_representative_trace() {
        let rendered = RenderFormatDispatcher::render(
            &RenderMessage::new("pc")
                .with_value("solution_found", true)
                .with_value("total_solution_count", 12usize)
                .with_value("retained_trace_count", 1usize)
                .with_value("trace_retention_truncated", true)
                .with_value("trace_retention_reason", "representative-retained-trace"),
            RenderFormat::Text,
        )
        .expect("text golden");

        assert!(rendered.contains("total_solution_count: 12"));
        assert!(rendered.contains("retained_trace_count: 1"));
        assert!(rendered.contains("trace_retention_truncated: true"));
        assert!(rendered.contains("trace_retention_reason: representative-retained-trace"));
        assert!(!rendered
            .lines()
            .any(|line| line == "total_solution_count: 1"));
    }
}

mod case_verbose_output_contains_backend_report {
    use super::*;

    #[test]
    fn verbose_output_contains_backend_report() {
        let rendered = RenderFormatDispatcher::render(
            &RenderMessage::new("pc")
                .with_value("backend_selected", "cpu")
                .with_value("backend_report", "attached")
                .with_value("backend_fallback_reason", "gpu_feature_disabled")
                .with_value("gpu_trust_state", "fallback-used")
                .with_value("gpu_worker_state", "unavailable"),
            RenderFormat::TextVerbose,
        )
        .expect("verbose golden");

        assert!(rendered.contains("backend_report: attached"));
        assert!(rendered.contains("backend_fallback_reason: gpu_feature_disabled"));
        assert!(rendered.contains("gpu_trust_state: fallback-used"));
    }
}

mod case_diagnostics_output_contains_evidence {
    use super::*;

    #[test]
    fn diagnostics_output_contains_evidence() {
        let rendered = RenderFormatDispatcher::render(
            &RenderMessage::new("diagnostic")
                .with_value("diagnostic_code", "W_BACKEND_FALLBACK_USED")
                .with_value(
                    "diagnostic_evidence",
                    "backend_fallback_reason=gpu_feature_disabled",
                )
                .with_value(
                    "suggested_next_step",
                    "Use --backend cpu or enable GPU support.",
                ),
            RenderFormat::TextDiagnostics,
        )
        .expect("diagnostic golden");

        assert!(rendered.contains("diagnostic_code: W_BACKEND_FALLBACK_USED"));
        assert!(
            rendered.contains("diagnostic_evidence: backend_fallback_reason=gpu_feature_disabled")
        );
        assert!(rendered.contains("suggested_next_step: Use --backend cpu or enable GPU support."));
    }
}

mod case_retained_trace_truncation_does_not_mark_count_incomplete {
    use super::*;

    #[test]
    fn retained_trace_truncation_does_not_mark_count_incomplete() {
        let rendered = render_json(
            &RenderMessage::new("pc")
                .with_value("total_solution_count", 12usize)
                .with_value("retained_trace_count", 1usize)
                .with_value("count_complete", true)
                .with_value("count_truncated_reason", "none")
                .with_value("trace_retention_truncated", true)
                .with_value("trace_retention_reason", "retained_trace_limit"),
        );

        assert!(rendered.contains("\"count_complete\":true"));
        assert!(rendered.contains("\"count_truncated_reason\":\"none\""));
        assert!(rendered.contains("\"trace_retention_truncated\":true"));
        assert!(rendered.contains("\"trace_retention_reason\":\"retained_trace_limit\""));
    }
}

mod case_resource_report_in_json {
    use super::*;

    #[test]
    fn resource_report_in_json() {
        let rendered = render_json(
            &RenderMessage::new("pc")
                .with_value("resource_truncated", true)
                .with_value("resource_truncation_reason", "frontier_budget_exceeded")
                .with_value("resource_peak_frontier_states", 513usize)
                .with_value("resource_peak_candidate_rows", 42usize)
                .with_value("resource_peak_hash_buckets", 17usize)
                .with_value("resource_peak_gpu_bytes", 0usize)
                .with_value("resource_peak_cpu_bytes", 4096usize)
                .with_value("resource_build_worker_backlog_peak", 3usize)
                .with_value("resource_coverage_rows_emitted", 2usize)
                .with_value("resource_probability_complete", false),
        );

        assert!(rendered.contains("\"resource_report\""));
        assert!(rendered.contains("\"truncated\":true"));
        assert!(rendered.contains("\"truncation_reason\":\"frontier_budget_exceeded\""));
        assert!(rendered.contains("\"peak_frontier_states\":513"));
        assert!(rendered.contains("\"probability_complete\":false"));
    }
}

mod case_resource_report_in_verbose_text {
    use super::*;

    #[test]
    fn resource_report_in_verbose_text() {
        let rendered = RenderFormatDispatcher::render(
            &RenderMessage::new("pc")
                .with_value("resource_truncated", true)
                .with_value("resource_truncation_reason", "candidate_budget_exceeded")
                .with_value("resource_probability_complete", false),
            RenderFormat::TextVerbose,
        )
        .expect("resource golden");

        assert!(rendered.contains("resource_truncated: true"));
        assert!(rendered.contains("resource_truncation_reason: candidate_budget_exceeded"));
        assert!(rendered.contains("resource_probability_complete: false"));
    }
}

mod case_json_retained_trace_average_not_labeled_expected_score {
    use super::*;

    #[test]
    fn json_retained_trace_average_not_labeled_expected_score() {
        let rendered = render_json(&score_expectation_message());

        assert!(rendered.contains("\"retained_trace_average_score\":1200"));
        assert!(rendered.contains("\"unconditional_expected_score\":640"));
        assert!(!rendered.contains("\"unconditional_expected_score\":1200"));
    }
}
