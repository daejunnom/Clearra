use crate::json::{
    backend_report_contract::pc_backend_report_contract,
    json_contract_helpers::{
        bool_or_false, field_value, nullable_device_label_value, nullable_number_value,
        nullable_string_value, number_or_null, pick_object, prefixed_object, push_existing,
        string_or_null, string_or_null_fallback, string_value_is, trace_key_values,
    },
    json_value::{JsonField, JsonValue},
    setup_json_contract::supply_contract,
};

mod backend_traversal {
    use super::*;

    pub(super) fn backend_traversal(fields: &[JsonField]) -> JsonValue {
        match nullable_string_value(field_value(fields, "selected_model")) {
            JsonValue::String(value) if value == "bfs-frontier" => JsonValue::string("bfs"),
            value => value,
        }
    }
}
mod pc_backend_contract {
    use super::*;

    pub(super) fn pc_backend_contract(fields: &[JsonField]) -> JsonValue {
        JsonValue::object([
            (
                "requested",
                string_or_null_fallback(fields, "backend_requested", "requested_backend"),
            ),
            (
                "selected",
                string_or_null_fallback(fields, "backend_selected", "selected_backend"),
            ),
            ("compute", string_or_null(fields, "compute_device")),
            ("traversal", backend_traversal(fields)),
            (
                "selection_reason",
                string_or_null(fields, "backend_selection_reason"),
            ),
            (
                "fallback_used",
                bool_or_false(fields, "backend_fallback_used"),
            ),
            (
                "fallback_reason",
                nullable_string_value(field_value(fields, "backend_fallback_reason")),
            ),
            (
                "workers_requested",
                nullable_number_value(field_value(fields, "workers_requested")),
            ),
            ("workers_used", number_or_null(fields, "workers_used")),
            (
                "deterministic",
                bool_or_false(fields, "execution_deterministic"),
            ),
            ("gpu_confirmed", bool_or_false(fields, "gpu_confirmed")),
            ("cpu_confirmed", bool_or_false(fields, "cpu_confirmed")),
            (
                "candidate_backend",
                string_or_null(fields, "candidate_backend"),
            ),
            ("buildup_backend", string_or_null(fields, "buildup_backend")),
            ("gpu", pc_backend_gpu_contract(fields)),
        ])
    }
}
mod pc_backend_gpu_contract {
    use super::*;

    pub(super) fn pc_backend_gpu_contract(fields: &[JsonField]) -> JsonValue {
        let selected = nullable_string_value(
            field_value(fields, "backend_selected")
                .or_else(|| field_value(fields, "selected_backend")),
        );
        let compute = nullable_string_value(field_value(fields, "compute_device"));
        let gpu_selected = string_value_is(&selected, "gpu") && string_value_is(&compute, "gpu");

        JsonValue::object([
            (
                "device_selected",
                JsonValue::Bool(string_value_is(&compute, "gpu")),
            ),
            (
                "device_label",
                nullable_device_label_value(field_value(fields, "gpu_device")),
            ),
            (
                "backend",
                if gpu_selected {
                    JsonValue::string("native-gpu")
                } else {
                    JsonValue::Null
                },
            ),
            (
                "unavailable_reason",
                nullable_string_value(field_value(fields, "gpu_unavailable_reason")),
            ),
        ])
    }
}
mod pc_contract {
    use super::*;

    pub(crate) fn pc_contract(fields: &[JsonField]) -> JsonValue {
        JsonValue::object([
            ("search", pc_search_contract(fields)),
            ("backend", pc_backend_contract(fields)),
            ("backend_report", pc_backend_report_contract(fields)),
            ("memory_report", pc_memory_report_contract(fields)),
            ("execution_report", pc_execution_report_contract(fields)),
            (
                "counts",
                pick_object(
                    fields,
                    &[
                        "total_solution_count",
                        "unique_solution_count",
                        "actual_solution_set_contract",
                        "normalized_solution_key_algorithm",
                        "normalized_solution_set_hash_algorithm",
                        "normalized_unique_solution_count",
                        "actual_normalized_unique_solution_count",
                        "normalized_solution_set_hash",
                        "actual_normalized_solution_set_hash",
                        "state_count",
                        "multiplicity_count",
                        "state_count_available",
                        "multiplicity_count_available",
                    ],
                ),
            ),
            (
                "counting",
                pick_object(
                    fields,
                    &[
                        "count_mode",
                        "count_requested",
                        "count_complete",
                        "count_truncated_reason",
                    ],
                ),
            ),
            ("trace", pc_trace_contract(fields)),
            ("coverage", pc_coverage_contract(fields)),
            ("spin_target", pc_spin_target_contract(fields)),
            ("scoring", pc_scoring_contract(fields)),
            ("continuation", prefixed_object(fields, "continuation_")),
            ("replay", prefixed_object(fields, "scenario_replay_")),
            ("rule", pc_rule_contract(fields)),
            ("supply", supply_contract(fields)),
            ("objective", prefixed_object(fields, "objective_")),
            (
                "checkpoint_schedule",
                pick_object(
                    fields,
                    &[
                        "checkpoint_schedule_source",
                        "checkpoint_schedule_label",
                        "checkpoint_schedule_partitions",
                        "checkpoint_schedule_checkpoint_count",
                        "checkpoint_results",
                        "partitions",
                        "checkpoints",
                    ],
                ),
            ),
            (
                "remaining",
                pick_object(
                    fields,
                    &[
                        "remaining_queue_len",
                        "remaining_queue_preview",
                        "remaining_hold",
                        "best_remaining_queue_len",
                        "next_pc_available",
                        "continuation_available",
                        "continue_available",
                        "continuation_available_complete",
                        "next_pc_candidate",
                        "continuation_token_available",
                        "continuation_token_unavailable_reason",
                        "continuation_basis",
                        "continuation_queue_consumed",
                        "continuation_token",
                        "continue_hint",
                        "replay_hint",
                    ],
                ),
            ),
        ])
    }
}
mod pc_coverage_contract {
    use super::*;

    pub(super) fn pc_coverage_contract(fields: &[JsonField]) -> JsonValue {
        pick_object(
            fields,
            &[
                "coverage_result",
                "coverage_row_view",
                "coverage_row_count",
                "coverage_pattern_count",
                "coverage_probability",
                "covered_pattern_count",
                "probability_complete",
                "supply_covered_pattern_count",
                "supply_total_pattern_count",
                "supply_weighted_pattern_count",
                "supply_materialized_probability_mass",
                "supply_probability_complete",
            ],
        )
    }
}
mod pc_execution_report_contract {
    use super::*;

    pub(super) fn pc_execution_report_contract(fields: &[JsonField]) -> JsonValue {
        JsonValue::object([
            (
                "model",
                pick_object(
                    fields,
                    &[
                        "search_execution_report",
                        "packing_result",
                        "packing_candidate_view",
                        "buildup_result",
                        "build_variant_view",
                        "coverage_result",
                        "coverage_row_view",
                        "objective_result",
                        "replay_trace",
                        "packing_candidate_is_solution",
                    ],
                ),
            ),
            (
                "packing",
                pick_object(
                    fields,
                    &["packing_candidate_count", "packing_candidate_is_solution"],
                ),
            ),
            (
                "buildup",
                pick_object(
                    fields,
                    &[
                        "solution_found",
                        "total_solution_count",
                        "unique_solution_count",
                        "actual_solution_set_contract",
                        "normalized_solution_key_algorithm",
                        "normalized_solution_set_hash_algorithm",
                        "normalized_unique_solution_count",
                        "actual_normalized_unique_solution_count",
                        "normalized_solution_set_hash",
                        "actual_normalized_solution_set_hash",
                        "count_complete",
                        "count_truncated_reason",
                    ],
                ),
            ),
            ("coverage", pc_coverage_contract(fields)),
            ("spin_target", pc_spin_target_contract(fields)),
            ("objective", prefixed_object(fields, "objective_")),
            ("scoring", pc_scoring_contract(fields)),
            ("replay", pc_trace_contract(fields)),
            ("backend", pc_backend_contract(fields)),
        ])
    }
}
mod pc_memory_report_contract {
    use super::*;

    pub(super) fn pc_memory_report_contract(fields: &[JsonField]) -> JsonValue {
        JsonValue::object([
            (
                "memory_leak_report_clean",
                bool_or_false(fields, "memory_leak_report_clean"),
            ),
            ("live_scopes", number_or_null(fields, "live_scopes")),
            (
                "live_allocations",
                number_or_null(fields, "live_allocations"),
            ),
            (
                "live_gpu_buffers",
                number_or_null(fields, "live_gpu_buffers"),
            ),
            (
                "pending_release_queue",
                number_or_null(fields, "pending_release_queue"),
            ),
            (
                "pending_gpu_buffer_releases",
                number_or_null(fields, "pending_gpu_buffer_releases"),
            ),
            ("double_releases", number_or_null(fields, "double_releases")),
            ("canary_failures", number_or_null(fields, "canary_failures")),
            (
                "poison_detections",
                number_or_null(fields, "poison_detections"),
            ),
            (
                "memory_pressure_level",
                string_or_null(fields, "memory_pressure_level"),
            ),
        ])
    }
}
mod pc_rule_contract {
    use super::*;

    pub(super) fn pc_rule_contract(fields: &[JsonField]) -> JsonValue {
        pick_object(
            fields,
            &[
                "board_profile",
                "piece_set_profile",
                "bag_profile",
                "rule_profile",
                "effective_kick_model",
                "verified_kick_profile",
                "kick_profile",
                "rule_extension_reason",
                "requires_180",
            ],
        )
    }
}
mod pc_scoring_contract {
    use super::*;

    pub(super) fn pc_scoring_contract(fields: &[JsonField]) -> JsonValue {
        pick_object(
            fields,
            &[
                "score_post_processing",
                "score_requested",
                "score_objective_mode",
                "score_initial_b2b",
                "score_b2b_chain_rule",
                "score_all_clear_b2b_extra_increment",
                "score_hard_drop_included",
                "score_soft_drop_included",
                "score_level_multiplier",
                "score_level_system_enabled",
                "score_spin_piece_scope",
                "score_same_shape_policy",
                "score_core_hot_path",
                "score_postprocess_owner",
                "score_profile",
                "score_profile_id",
                "score_model_id",
                "attack_model_id",
                "spin_rule_id",
                "spin_award_policy",
                "drop_score_policy",
                "level_policy",
                "combo_policy",
                "b2b_policy",
                "pc_bonus_policy",
                "trace_requirement",
                "score_accuracy_level",
                "score_accuracy_reason",
                "score_profile_accuracy_mode",
                "score_profile_specific_exact",
                "score_event_basis",
                "score_interpretation_basis",
                "score_evaluation_trace_count",
                "score_evaluation_complete",
                "score_evaluation_basis",
                "score_evaluation_scope",
                "score_best_score",
                "score_best_attack",
                "score_representative_score",
                "score_representative_attack",
                "score_event_count",
                "score_matrix_materialized",
                "score_matrix_complete",
                "score_matrix_cell_count",
                "score_matrix_pattern_count",
                "score_matrix_profile_id",
                "score_matrix_accuracy_level",
                "score_matrix_incomplete_reason",
                "score_probability_before",
                "score_probability_after",
                "score_does_not_change_probability_union",
                "score_best_complete",
                "score_summary_complete",
                "score_summary_incomplete_reason",
                "score_all_universe_patterns_covered",
                "score_pattern_optimal_count",
                "score_covered_probability",
                "score_unconditional_expected_score",
                "score_unconditional_expected_attack",
                "score_covered_pattern_conditional_average_score",
                "placement_event_available",
                "clear_event_available",
                "drop_event_basis_available",
                "spin_event_basis_available",
            ],
        )
    }
}
mod pc_search_contract {
    use super::*;

    pub(super) fn pc_search_contract(fields: &[JsonField]) -> JsonValue {
        pick_object(
            fields,
            &[
                "status",
                "lines",
                "completion_goal",
                "exact_target_policy",
                "cleared_lines",
                "solution_found",
                "route",
                "solver_backend",
                "requested_backend",
                "selected_backend",
                "backend_requested",
                "backend_selected",
                "effective_backend",
                "selected_model",
                "compute_device",
                "search_result_model",
                "search_execution_report",
                "chain_labels",
                "chain_class",
                "checkpoint_results",
                "backend_report",
                "packing_result",
                "packing_candidate_view",
                "packing_candidate_count",
                "buildup_result",
                "build_variant_view",
                "build_variant_count",
                "coverage_result",
                "coverage_row_view",
                "coverage_row_count",
                "coverage_probability",
                "objective_result",
                "replay_trace",
                "backend_selection_reason",
                "backend_fallback_allowed",
                "backend_fallback_used",
                "backend_fallback_reason",
                "execution_workers",
                "workers_requested",
                "workers_used",
                "execution_deterministic",
                "execution_max_frontier_states",
                "execution_max_memory_mib",
                "gpu_device",
                "gpu_unavailable_reason",
                "state_count_available",
                "multiplicity_count_available",
                "searched_nodes",
                "search_nodes",
                "budget_exceeded",
                "budget_exceeded_count",
                "search_unsupported_reason",
                "two_line_capable",
                "two_line_fast_path_available",
                "two_line_fallback_reason",
                "placed_pieces",
            ],
        )
    }
}
mod pc_spin_target_contract {
    use super::*;

    pub(super) fn pc_spin_target_contract(fields: &[JsonField]) -> JsonValue {
        pick_object(
            fields,
            &[
                "spin_target_id",
                "spin_target_request",
                "spin_target_predicate",
                "target_probability_threshold",
                "score_profile_id",
                "trace_requirement",
                "spin_classifier_id",
                "spin_classifier_exact",
                "spin_trace_completeness",
                "spin_probability_complete",
                "spin_probability_diagnostic_code",
                "spin_coverage_reducer",
                "spin_coverage_row_kind",
                "spin_probability",
                "spin_covered_pattern_count",
                "spin_pattern_count",
            ],
        )
    }
}
mod pc_trace_contract {
    use super::*;

    pub(super) fn pc_trace_contract(fields: &[JsonField]) -> JsonValue {
        let mut members = Vec::new();
        for key in [
            "retained_trace_count",
            "retained_trace_key_count",
            "solution_trace_count",
            "unique_solution_trace_count",
            "actual_solution_set_contract",
            "normalized_solution_key_algorithm",
            "normalized_solution_set_hash_algorithm",
            "normalized_unique_solution_count",
            "actual_normalized_unique_solution_count",
            "normalized_solution_set_hash",
            "actual_normalized_solution_set_hash",
            "solution_path_count",
            "trace_steps",
            "trace_available",
            "solution_trace_available",
            "sample_trace_available",
            "solution_trace_mode",
            "trace_retention_truncated",
            "trace_retention_reason",
            "retained_trace_limit",
            "min_queue_consumed",
            "max_queue_consumed",
            "sample_queue_consumed",
            "placed_piece_count",
        ] {
            push_existing(fields, &mut members, key, key);
        }
        if let Some(value) = field_value(fields, "retained_trace_keys") {
            match value {
                JsonValue::Array(_) => members.push(("retained_trace_keys".to_owned(), value)),
                JsonValue::String(keys) => members.push((
                    "retained_trace_keys".to_owned(),
                    JsonValue::array(trace_key_values(&keys)),
                )),
                _ => members.push(("retained_trace_keys".to_owned(), value)),
            }
        }
        JsonValue::object(members)
    }
}

use backend_traversal::backend_traversal;
use pc_backend_contract::pc_backend_contract;
use pc_backend_gpu_contract::pc_backend_gpu_contract;
pub(crate) use pc_contract::pc_contract;
use pc_coverage_contract::pc_coverage_contract;
use pc_execution_report_contract::pc_execution_report_contract;
use pc_memory_report_contract::pc_memory_report_contract;
use pc_rule_contract::pc_rule_contract;
use pc_scoring_contract::pc_scoring_contract;
use pc_search_contract::pc_search_contract;
use pc_spin_target_contract::pc_spin_target_contract;
use pc_trace_contract::pc_trace_contract;
