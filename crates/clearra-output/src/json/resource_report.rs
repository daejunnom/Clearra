use crate::json::{
    json_contract_helpers::{field_value, number_or_null, string_or_null, string_value_is},
    json_value::JsonValue,
};

use super::JsonField;

const RESOURCE_KEYS: &[&str] = &[
    "resource_truncated",
    "resource_truncation_reason",
    "resource_peak_frontier_states",
    "resource_peak_candidate_rows",
    "resource_peak_hash_buckets",
    "resource_peak_gpu_bytes",
    "resource_peak_cpu_bytes",
    "resource_build_worker_backlog_peak",
    "resource_coverage_rows_emitted",
    "resource_probability_complete",
    "count_complete",
    "count_truncated_reason",
    "probability_complete",
    "supply_expansion_truncated",
    "supply_probability_complete",
    "supply_materialized_probability_mass",
    "materialized_probability_mass",
    "renormalized",
];

pub(crate) fn resource_report_object(fields: &[JsonField]) -> Option<JsonValue> {
    if !RESOURCE_KEYS
        .iter()
        .any(|key| field_value(fields, key).is_some())
    {
        return None;
    }

    Some(JsonValue::object([
        ("truncated", JsonValue::Bool(resource_truncated(fields))),
        (
            "truncation_reason",
            resource_truncation_reason(fields).unwrap_or(JsonValue::Null),
        ),
        (
            "peak_frontier_states",
            number_or_null(fields, "resource_peak_frontier_states"),
        ),
        (
            "peak_candidate_rows",
            number_or_null(fields, "resource_peak_candidate_rows"),
        ),
        (
            "peak_hash_buckets",
            number_or_null(fields, "resource_peak_hash_buckets"),
        ),
        (
            "peak_gpu_bytes",
            number_or_null(fields, "resource_peak_gpu_bytes"),
        ),
        (
            "peak_cpu_bytes",
            number_or_null(fields, "resource_peak_cpu_bytes"),
        ),
        (
            "build_worker_backlog_peak",
            number_or_null(fields, "resource_build_worker_backlog_peak"),
        ),
        (
            "coverage_rows_emitted",
            number_or_null(fields, "resource_coverage_rows_emitted"),
        ),
        (
            "probability_complete",
            JsonValue::Bool(resource_probability_complete(fields)),
        ),
        ("count_complete", JsonValue::Bool(count_complete(fields))),
        (
            "count_truncated_reason",
            string_or_null(fields, "count_truncated_reason"),
        ),
        (
            "materialized_probability_mass",
            materialized_probability_mass(fields),
        ),
        ("renormalized", JsonValue::Bool(renormalized(fields))),
    ]))
}

fn resource_truncated(fields: &[JsonField]) -> bool {
    bool_field(fields, "resource_truncated").unwrap_or(false)
        || !count_complete(fields)
        || bool_field(fields, "probability_complete") == Some(false)
        || observed_universe_truncated(fields)
}

fn resource_probability_complete(fields: &[JsonField]) -> bool {
    let explicit = bool_field(fields, "resource_probability_complete")
        .or_else(|| bool_field(fields, "probability_complete"))
        .unwrap_or(true);
    explicit && count_complete(fields) && !resource_truncated(fields)
}

fn count_complete(fields: &[JsonField]) -> bool {
    bool_field(fields, "count_complete").unwrap_or(true)
}

fn resource_truncation_reason(fields: &[JsonField]) -> Option<JsonValue> {
    if let Some(reason) = field_value(fields, "resource_truncation_reason") {
        if !string_value_is(&reason, "none") {
            return Some(reason);
        }
    }
    if !count_complete(fields) {
        return Some(
            field_value(fields, "count_truncated_reason")
                .unwrap_or_else(|| JsonValue::string("count_incomplete")),
        );
    }
    if observed_universe_truncated(fields) {
        return Some(JsonValue::string("observed_universe_truncated"));
    }
    if bool_field(fields, "probability_complete") == Some(false) {
        return Some(JsonValue::string("probability_incomplete"));
    }
    if bool_field(fields, "resource_truncated") == Some(true) {
        return Some(JsonValue::string("resource_truncated"));
    }
    None
}

fn observed_universe_truncated(fields: &[JsonField]) -> bool {
    bool_field(fields, "supply_expansion_truncated") == Some(true)
        || bool_field(fields, "supply_probability_complete") == Some(false)
            && match field_value(fields, "queue_mode") {
                Some(queue_mode) => string_value_is(&queue_mode, "observed"),
                None => true,
            }
}

fn materialized_probability_mass(fields: &[JsonField]) -> JsonValue {
    number_or_null(fields, "supply_materialized_probability_mass")
        .or_else_number(|| number_or_null(fields, "materialized_probability_mass"))
}

fn renormalized(fields: &[JsonField]) -> bool {
    bool_field(fields, "renormalized").unwrap_or(false)
}

fn bool_field(fields: &[JsonField], key: &str) -> Option<bool> {
    match field_value(fields, key) {
        Some(JsonValue::Bool(value)) => Some(value),
        Some(JsonValue::String(value)) if value == "true" => Some(true),
        Some(JsonValue::String(value)) if value == "false" => Some(false),
        _ => None,
    }
}

trait JsonNumberFallback {
    fn or_else_number<F>(self, fallback: F) -> JsonValue
    where
        F: FnOnce() -> JsonValue;
}

impl JsonNumberFallback for JsonValue {
    fn or_else_number<F>(self, fallback: F) -> JsonValue
    where
        F: FnOnce() -> JsonValue,
    {
        match self {
            JsonValue::Null => fallback(),
            value => value,
        }
    }
}
