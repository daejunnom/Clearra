use crate::json::{
    json_contract_helpers::{indexed_results, pick_object, push_existing},
    json_value::{JsonField, JsonValue},
};
pub(crate) fn setup_contract(fields: &[JsonField]) -> JsonValue {
    JsonValue::object([
        (
            "search",
            pick_object(
                fields,
                &[
                    "status",
                    "execution_scope",
                    "enumeration_strategy",
                    "shape_family_enumeration_complete",
                    "tiling_variant_enumeration_complete",
                    "build_variant_enumeration_complete",
                    "post_pc_mode",
                    "post_pc_evaluation_attached",
                    "setup_foundation_reason",
                    "queue_mode",
                    "queue_len",
                    "result_count",
                    "family_count",
                    "shape_family_id",
                    "shape_family_count",
                    "tiling_variant_count",
                    "build_variant_count",
                    "covered_pattern_count",
                    "coverage_probability",
                    "queue_prefix",
                    "queue_prefix_len",
                    "hold_required",
                    "hold_piece",
                    "bag_boundary_offsets",
                    "bag_boundary_ambiguous",
                    "requires_180",
                    "requires_180_evidence",
                    "rule_profile_evidence",
                    "post_pc_solution_count",
                    "score_basis",
                    "backend_report",
                    "raw_coverage_export_path",
                ],
            ),
        ),
        ("supply", supply_contract(fields)),
        ("raw_metrics", setup_raw_metrics_contract(fields)),
        ("raw_coverage", setup_raw_coverage_contract(fields)),
        ("results", indexed_results(fields, "result_")),
    ])
}

fn setup_raw_metrics_contract(fields: &[JsonField]) -> JsonValue {
    let mut members = Vec::new();
    push_existing(
        fields,
        &mut members,
        "setup_raw_metrics_schema_version",
        "schema_version",
    );
    push_existing(fields, &mut members, "metrics_kind", "metrics_kind");
    for key in [
        "shape_family_id",
        "shape_family_count",
        "tiling_variant_count",
        "build_variant_count",
        "covered_pattern_count",
        "coverage_probability",
        "queue_prefix",
        "queue_prefix_len",
        "hold_required",
        "hold_piece",
        "bag_boundary_offsets",
        "bag_boundary_ambiguous",
        "requires_180",
        "requires_180_evidence",
        "rule_profile_evidence",
        "post_pc_solution_count",
        "score_basis",
        "score_aggregation_attached",
        "backend_report",
        "raw_coverage_export_path",
        "setup_raw_metrics",
        "setup_raw_coverage_export",
        "coverage_overlap_report",
        "build_variant_metrics",
        "diagnostic_evidence",
    ] {
        push_existing(fields, &mut members, key, key);
    }
    JsonValue::object(members)
}

fn setup_raw_coverage_contract(fields: &[JsonField]) -> JsonValue {
    let mut members = Vec::new();
    push_existing(
        fields,
        &mut members,
        "raw_coverage_schema_version",
        "schema_version",
    );
    push_existing(
        fields,
        &mut members,
        "raw_coverage_export_kind",
        "export_kind",
    );
    for key in [
        "pattern_universe_id",
        "pattern_weight_model_id",
        "pattern_count",
        "rows",
        "family_unions",
        "overlap_report",
        "raw_coverage_export_path",
        "covered_pattern_count",
        "coverage_probability",
    ] {
        push_existing(fields, &mut members, key, key);
    }
    JsonValue::object(members)
}

pub(crate) fn spin_probability_contract(fields: &[JsonField]) -> JsonValue {
    pick_object(
        fields,
        &[
            "spin_target_id",
            "spin_target_name",
            "covered_pattern_count",
            "pattern_count",
            "pattern_universe_id",
            "pattern_weight_model_id",
            "probability",
            "probability_complete",
            "materialized_probability_mass",
            "renormalized",
            "truncation_reason",
            "spin_accuracy",
            "trace_completeness",
            "score_profile_id",
        ],
    )
}

pub(crate) fn score_expectation_contract(fields: &[JsonField]) -> JsonValue {
    pick_object(
        fields,
        &[
            "score_profile_id",
            "score_accuracy",
            "trace_completeness",
            "evaluation_scope",
            "retained_trace_average_score",
            "covered_pattern_conditional_average_score",
            "unconditional_expected_score",
            "best_score_by_pattern_available",
            "score_does_not_change_probability_union",
        ],
    )
}

pub(crate) fn special_spin_diagnostic_contract(fields: &[JsonField]) -> JsonValue {
    pick_object(
        fields,
        &[
            "special_spin_case_id",
            "verification_state",
            "kick_evidence_required",
            "kick_evidence_available",
            "classification_accuracy",
            "disabled_reason",
        ],
    )
}

pub(crate) fn supply_contract(fields: &[JsonField]) -> JsonValue {
    let mut members = Vec::new();
    for key in [
        "queue_mode",
        "queue_len",
        "pattern_count",
        "total_pattern_count",
        "covered_pattern_count",
        "weighted_pattern_count",
        "materialized_probability_mass",
        "probability",
        "weighted_probability",
        "probability_model",
        "probability_complete",
        "expansion_truncated",
        "boundary_candidates",
        "c_buildup_coverage_row_count",
    ] {
        push_existing(fields, &mut members, key, key);
    }
    for field in fields {
        if let Some(stripped) = field.key().strip_prefix("supply_") {
            members.push((stripped.to_owned(), field.value().clone()));
        }
    }
    JsonValue::object(members)
}
