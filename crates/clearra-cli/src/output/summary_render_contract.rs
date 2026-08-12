use std::collections::BTreeSet;

use clearra_output::model::is_json_number;

use super::{bool_field, number_field, string_array_field, string_field, RenderField};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SummaryRenderContract;

impl SummaryRenderContract {
    pub fn render_fields<I, K, V>(fields: I) -> Vec<RenderField>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        fields
            .into_iter()
            .map(|(key, value)| render_field(key.into(), value.into()))
            .collect()
    }
}

fn render_field(key: String, value: String) -> RenderField {
    if key == "retained_trace_keys" {
        return retained_trace_keys_field(key, value);
    }
    if bool_keys().contains(key.as_str()) {
        return bool_value_field(key, value);
    }
    if number_keys().contains(key.as_str()) && is_json_number(&value) {
        return number_field(key, value);
    }
    if string_keys().contains(key.as_str()) {
        return string_field(key, value);
    }
    string_field(key, value)
}

fn retained_trace_keys_field(key: String, value: String) -> RenderField {
    if value.is_empty() || value == "none" {
        return string_array_field(key, std::iter::empty::<String>());
    }
    string_array_field(key, value.split(',').map(ToOwned::to_owned))
}

fn bool_value_field(key: String, value: String) -> RenderField {
    match value.as_str() {
        "true" => bool_field(key, true),
        "false" => bool_field(key, false),
        _ => string_field(key, value),
    }
}

fn bool_keys() -> BTreeSet<&'static str> {
    [
        "actual_unsupported",
        "allow_hold",
        "backend_fallback_allowed",
        "backend_fallback_used",
        "budget_exceeded",
        "b2b_enabled",
        "combo_enabled",
        "continue_available",
        "continuation_available_complete",
        "continuation_enough_queue_for_next_pc",
        "continuation_token_available",
        "cover_reports_c_coverage_row_count",
        "cover_reports_union_probability",
        "count_complete",
        "count_requested",
        "cpu_confirmed",
        "expected_checked",
        "expected_match",
        "expected_unsupported",
        "execution_deterministic",
        "expansion_truncated",
        "gpu_backend_available",
        "gpu_confirmed",
        "hold_enabled",
        "interactive_prompt",
        "multiplicity_count_available",
        "native_c_core_executed",
        "native_c_core_linked",
        "next_pc_available",
        "objective_applied",
        "objective_complete",
        "objective_search_complete",
        "objective_score_does_not_modify_coverage_probability",
        "objective_score_probability_no_double_count",
        "path_distinguishes_retained_trace_from_total_count",
        "path_reports_representative_trace",
        "post_pc_evaluation_attached",
        "postprocess_execution_complete",
        "percent_reports_covered_pattern_count",
        "percent_reports_probability_complete",
        "percent_reports_total_pattern_count",
        "probability_complete",
        "placement_event_available",
        "requires_180",
        "requires_lock_reachability",
        "requires_spawn_reachability",
        "renormalized",
        "retained_representative_trace",
        "retained_trace_keys_match",
        "retained_trace_keys_checked",
        "sample_trace_available",
        "score_aggregation_attached",
        "score_core_hot_path",
        "score_does_not_change_probability_union",
        "score_evaluation_complete",
        "score_best_complete",
        "score_all_universe_patterns_covered",
        "score_hard_drop_included",
        "score_soft_drop_included",
        "score_level_system_enabled",
        "score_post_processing",
        "score_requested",
        "score_summary_complete",
        "score_matrix_complete",
        "score_matrix_materialized",
        "score_profile_specific_exact",
        "search_backend_supported",
        "solution_count_calculated",
        "solution_found",
        "solution_keys_complete",
        "solution_page_available",
        "solution_probabilities_requested",
        "solution_probability_complete",
        "solution_set_materialized",
        "solution_trace_available",
        "spin_event_basis_available",
        "state_count_available",
        "slot_assignment_count_is_not_success_probability",
        "supports_180",
        "supply_expansion_truncated",
        "supply_probability_complete",
        "trace_available",
        "trace_retention_truncated",
        "transition_complete",
        "truncated",
        "two_line_capable",
        "two_line_fast_path_available",
        "unsupported_matched",
        "verified_profile",
        "verified_kick_profile",
        "clear_event_available",
        "drop_event_basis_available",
        "failed_pattern_count_complete",
        "failed_pattern_examples_truncated",
    ]
    .into_iter()
    .collect()
}

fn number_keys() -> BTreeSet<&'static str> {
    [
        "board_height",
        "board_width",
        "boundary_candidates",
        "budget_exceeded_count",
        "build_variant_count",
        "checkpoints",
        "checkpoint_schedule_checkpoint_count",
        "cleared_lines",
        "continuation_queue_consumed",
        "continuation_max_remaining_pieces",
        "continuation_min_remaining_queue",
        "continuation_min_required_pieces",
        "continuations",
        "coverage_probability",
        "coverage_row_count",
        "c_buildup_coverage_row_count",
        "covered_pattern_count",
        "dag_edges",
        "dag_nodes",
        "diagnostic_count",
        "duplicate_transition_count",
        "exact_pieces",
        "failed_pattern_count",
        "failed_pattern_examples_materialized",
        "failed_pattern_limit",
        "failed_queue_probability",
        "execution_max_frontier_states",
        "execution_max_candidates",
        "execution_max_patterns",
        "execution_max_memory_mib",
        "execution_workers",
        "expected_retained_trace_key_count",
        "expected_total_solution_count",
        "family_count",
        "kick_verification_cases",
        "kick_verification_failures",
        "issue_count",
        "jstris_180_transitions",
        "lines",
        "materialized_pattern_count",
        "materialized_probability_mass",
        "missing_transition_count",
        "min_queue_consumed",
        "min_remaining_queue",
        "minimum_len",
        "max_queue_consumed",
        "multiplicity_count",
        "no_kick_transitions",
        "objective_coverage_matrix_rows",
        "objective_solution_traces",
        "objective_unique_solution_traces",
        "occupied_cells",
        "page_count",
        "partitions",
        "packing_candidate_count",
        "paths",
        "pattern_count",
        "piece_window",
        "placed_piece_count",
        "placed_pieces",
        "postprocess_execution_count",
        "postprocess_pattern_weight_count",
        "post_pc_solution_count",
        "profile_count",
        "probability",
        "queue_len",
        "remaining_queue_len",
        "retained_path_results",
        "retained_trace_count",
        "retained_trace_key_count",
        "retained_trace_limit",
        "sample_queue_consumed",
        "score_best_attack",
        "score_best_score",
        "score_covered_pattern_conditional_average_score",
        "score_covered_probability",
        "score_event_count",
        "score_initial_b2b",
        "score_all_clear_b2b_extra_increment",
        "score_level_multiplier",
        "score_matrix_cell_count",
        "score_matrix_pattern_count",
        "score_representative_attack",
        "score_representative_score",
        "score_pattern_optimal_count",
        "score_unconditional_expected_attack",
        "score_unconditional_expected_score",
        "score_evaluation_trace_count",
        "score_probability_after",
        "score_probability_before",
        "searched_nodes",
        "search_nodes",
        "srs_i_transitions",
        "srs_jlstz_transitions",
        "srs_plus_180_transitions",
        "state_count",
        "solution_path_count",
        "solution_keys_materialized_count",
        "solution_probability_count",
        "solution_trace_count",
        "successful_checkpoints",
        "successful_paths",
        "supply_boundary_candidates",
        "supply_covered_pattern_count",
        "supply_materialized_probability_mass",
        "supply_pattern_count",
        "supply_total_pattern_count",
        "supply_weighted_pattern_count",
        "tiling_variant_count",
        "total_pattern_count",
        "total_solution_count",
        "trace_steps",
        "transition_count",
        "unique_solution_count",
        "unique_solution_trace_count",
        "unsupported_annotation_count",
        "validation_error_count",
        "weighted_pattern_count",
        "weighted_probability",
        "workers_used",
    ]
    .into_iter()
    .collect()
}

fn string_keys() -> BTreeSet<&'static str> {
    [
        "backend_selection_reason",
        "objective_incomplete_reason",
        "objective_search_incomplete_reason",
        "score_basis",
        "score_accuracy_level",
        "score_accuracy_reason",
        "score_b2b_chain_rule",
        "score_evaluation_scope",
        "score_evaluation_basis",
        "score_event_basis",
        "score_interpretation_basis",
        "score_matrix_accuracy_level",
        "score_matrix_incomplete_reason",
        "score_matrix_profile_id",
        "score_profile",
        "score_profile_accuracy_mode",
        "score_objective_mode",
        "score_postprocess_owner",
        "score_same_shape_policy",
        "score_spin_piece_scope",
        "score_summary_incomplete_reason",
        "search_result_model",
        "solution_trace_mode",
        "success_probability_source",
        "truncation_reason",
        "trace_retention_reason",
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
#[path = "summary_render_contract_tests.rs"]
mod tests;
