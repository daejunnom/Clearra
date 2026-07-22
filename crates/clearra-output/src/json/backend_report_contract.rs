use crate::json::{
    backend_gpu_worker_contract::backend_gpu_worker_contract,
    json_contract_helpers::{
        bool_or_false, field_value, nullable_string_value, string_or_null, string_or_null_fallback,
    },
    json_value::{JsonField, JsonValue},
};

pub(crate) fn pc_backend_report_contract(fields: &[JsonField]) -> JsonValue {
    JsonValue::object([
        (
            "backend_requested",
            string_or_null_fallback(fields, "backend_requested", "requested_backend"),
        ),
        (
            "backend_selected",
            string_or_null_fallback(fields, "backend_selected", "selected_backend"),
        ),
        (
            "candidate_backend",
            string_or_null(fields, "candidate_backend"),
        ),
        ("buildup_backend", string_or_null(fields, "buildup_backend")),
        ("gpu_available", bool_or_false(fields, "gpu_available")),
        (
            "gpu_disabled_reason",
            nullable_string_value(field_value(fields, "gpu_disabled_reason")),
        ),
        ("gpu_trust_state", string_or_null(fields, "gpu_trust_state")),
        (
            "cpu_reference_matched",
            bool_or_false(fields, "cpu_reference_matched"),
        ),
        (
            "fallback_reason",
            nullable_string_value(field_value(fields, "backend_fallback_reason")),
        ),
        ("fallback_used", bool_or_false(fields, "fallback_used")),
        (
            "gpu_failure_class",
            nullable_string_value(field_value(fields, "gpu_failure_class")),
        ),
        (
            "gpu_failure_stage",
            nullable_string_value(field_value(fields, "gpu_failure_stage")),
        ),
        (
            "fallback_backend",
            field_value(fields, "fallback_backend").unwrap_or_else(|| JsonValue::string("none")),
        ),
        (
            "backend_fallback_reason",
            nullable_string_value(field_value(fields, "backend_fallback_reason")),
        ),
        (
            "discarded_partial_gpu_result",
            bool_or_false(fields, "discarded_partial_gpu_result"),
        ),
        (
            "cpu_confirm_required",
            bool_or_false(fields, "cpu_confirm_required"),
        ),
        (
            "deterministic_reference_matched",
            bool_or_false(fields, "deterministic_reference_matched"),
        ),
        ("hybrid_status", string_or_null(fields, "hybrid_status")),
        (
            "hybrid_disabled_reason",
            nullable_string_value(field_value(fields, "hybrid_disabled_reason")),
        ),
        (
            "memory_pressure_level",
            string_or_null(fields, "memory_pressure_level"),
        ),
        ("backpressure", string_or_null(fields, "backpressure")),
        ("gpu_worker", backend_gpu_worker_contract(fields)),
    ])
}
