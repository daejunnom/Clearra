use crate::json::{
    json_contract_helpers::{
        bool_or_false, field_value, nullable_number_value, nullable_string_value, number_or_null,
        string_or_null,
    },
    json_value::{JsonField, JsonValue},
};

pub(crate) fn backend_gpu_worker_contract(fields: &[JsonField]) -> JsonValue {
    JsonValue::object([
        ("state", string_or_null(fields, "gpu_worker_state")),
        ("trust_state", string_or_null(fields, "gpu_trust_state")),
        (
            "failure_class",
            nullable_string_value(field_value(fields, "gpu_failure_class")),
        ),
        (
            "failure_stage",
            nullable_string_value(field_value(fields, "gpu_failure_stage")),
        ),
        (
            "memory_ticket_id",
            number_or_null(fields, "gpu_memory_ticket_id"),
        ),
        ("fence_epoch", number_or_null(fields, "gpu_fence_epoch")),
        ("scope_epoch", number_or_null(fields, "gpu_scope_epoch")),
        ("byte_budget", number_or_null(fields, "gpu_byte_budget")),
        (
            "cpu_confirm_required",
            bool_or_false(fields, "cpu_confirm_required"),
        ),
        (
            "can_source_exact_probability",
            bool_or_false(fields, "gpu_can_source_exact_probability"),
        ),
        (
            "fallback_reason",
            nullable_string_value(
                field_value(fields, "gpu_worker_fallback_reason")
                    .or_else(|| field_value(fields, "backend_fallback_reason")),
            ),
        ),
        (
            "discarded_partial_result",
            bool_or_false(fields, "discarded_partial_gpu_result"),
        ),
        (
            "unavailable_reason",
            nullable_string_value(
                field_value(fields, "gpu_worker_unavailable_reason")
                    .or_else(|| field_value(fields, "gpu_unavailable_reason")),
            ),
        ),
        ("backpressure", gpu_worker_backpressure_contract(fields)),
    ])
}

fn gpu_worker_backpressure_contract(fields: &[JsonField]) -> JsonValue {
    JsonValue::object([
        (
            "gpu_queue_depth",
            nullable_number_value(field_value(fields, "gpu_backpressure_gpu_queue_depth")),
        ),
        (
            "cpu_worker_queue_depth",
            nullable_number_value(field_value(
                fields,
                "gpu_backpressure_cpu_worker_queue_depth",
            )),
        ),
        (
            "readback_pending_batches",
            nullable_number_value(field_value(
                fields,
                "gpu_backpressure_readback_pending_batches",
            )),
        ),
        (
            "build_variant_buffer_pressure",
            nullable_number_value(field_value(
                fields,
                "gpu_backpressure_build_variant_buffer_pressure",
            )),
        ),
        (
            "coverage_row_buffer_pressure",
            nullable_number_value(field_value(
                fields,
                "gpu_backpressure_coverage_row_buffer_pressure",
            )),
        ),
        (
            "throttled_backend",
            string_or_null(fields, "gpu_backpressure_throttled_backend"),
        ),
        (
            "throttle_reason",
            string_or_null(fields, "gpu_backpressure_throttle_reason"),
        ),
    ])
}

#[cfg(test)]
#[path = "backend_gpu_worker_contract_tests.rs"]
mod tests;
