use crate::{json::JsonField, model::RenderField};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BackendSummaryText;

impl BackendSummaryText {
    pub fn default_lines(fields: &[JsonField]) -> Vec<String> {
        vec![
            format!("backend: {}", field_or(fields, "backend_selected", "cpu")),
            format!("gpu: {}", gpu_default_summary(fields)),
            format!("memory: {}", memory_default_summary(fields)),
        ]
    }
}
impl BackendSummaryText {
    pub fn default_lines_from_render_fields(fields: &[RenderField]) -> Vec<String> {
        vec![
            format!(
                "backend: {}",
                render_field_or(fields, "backend_selected", "cpu")
            ),
            format!("gpu: {}", gpu_default_summary_from_render_fields(fields)),
            format!(
                "memory: {}",
                memory_default_summary_from_render_fields(fields)
            ),
        ]
    }
}
impl BackendSummaryText {
    pub fn verbose_lines(fields: &[JsonField]) -> Vec<String> {
        let keys = [
            "backend_requested",
            "backend_selected",
            "candidate_backend",
            "buildup_backend",
            "gpu_available",
            "gpu_disabled_reason",
            "gpu_worker_state",
            "gpu_trust_state",
            "cpu_confirm_required",
            "cpu_reference_matched",
            "fallback_used",
            "fallback_backend",
            "backend_fallback_reason",
            "hybrid_status",
            "hybrid_disabled_reason",
            "memory_pressure_level",
            "backpressure",
            "gpu_memory_ticket_id",
            "gpu_fence_epoch",
            "gpu_scope_epoch",
            "gpu_byte_budget",
            "gpu_backpressure_gpu_queue_depth",
            "gpu_backpressure_cpu_worker_queue_depth",
            "gpu_backpressure_readback_pending_batches",
            "gpu_backpressure_build_variant_buffer_pressure",
            "gpu_backpressure_coverage_row_buffer_pressure",
            "gpu_backpressure_throttled_backend",
            "gpu_backpressure_throttle_reason",
        ];

        keys.iter()
            .filter_map(|key| field_text_value(fields, key).map(|value| format!("{key}: {value}")))
            .collect()
    }
}
impl BackendSummaryText {
    pub fn verbose_lines_from_render_fields(fields: &[RenderField]) -> Vec<String> {
        let keys = [
            "backend_requested",
            "backend_selected",
            "candidate_backend",
            "buildup_backend",
            "gpu_available",
            "gpu_disabled_reason",
            "gpu_worker_state",
            "gpu_trust_state",
            "cpu_confirm_required",
            "cpu_reference_matched",
            "fallback_used",
            "fallback_backend",
            "backend_fallback_reason",
            "hybrid_status",
            "hybrid_disabled_reason",
            "memory_pressure_level",
            "backpressure",
            "gpu_memory_ticket_id",
            "gpu_fence_epoch",
            "gpu_scope_epoch",
            "gpu_byte_budget",
            "gpu_backpressure_gpu_queue_depth",
            "gpu_backpressure_cpu_worker_queue_depth",
            "gpu_backpressure_readback_pending_batches",
            "gpu_backpressure_build_variant_buffer_pressure",
            "gpu_backpressure_coverage_row_buffer_pressure",
            "gpu_backpressure_throttled_backend",
            "gpu_backpressure_throttle_reason",
        ];

        keys.iter()
            .filter_map(|key| {
                render_field_value(fields, key).map(|value| format!("{key}: {value}"))
            })
            .collect()
    }
}

fn gpu_default_summary(fields: &[JsonField]) -> String {
    let reason = field_value(fields, "gpu_worker_fallback_reason")
        .or_else(|| field_value(fields, "gpu_disabled_reason"))
        .or_else(|| field_value(fields, "gpu_unavailable_reason"))
        .or_else(|| field_value(fields, "backend_fallback_reason"));
    if let Some(reason) = meaningful_reason(reason) {
        return format!("unavailable ({reason})");
    }

    match field_value(fields, "gpu_worker_state") {
        Some("available") => "available".to_owned(),
        Some(state) => state.to_owned(),
        None => "not-used".to_owned(),
    }
}

fn memory_default_summary(fields: &[JsonField]) -> String {
    match field_value(fields, "memory_leak_report_clean") {
        Some("true") => "clean".to_owned(),
        Some("false") => "not-clean".to_owned(),
        _ => "not-reported".to_owned(),
    }
}

fn field_or<'a>(fields: &'a [JsonField], key: &str, fallback: &'a str) -> &'a str {
    field_value(fields, key).unwrap_or(fallback)
}

fn field_value<'a>(fields: &'a [JsonField], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|field| field.key() == key)
        .and_then(|field| {
            if let crate::json::JsonValue::String(value) = field.value() {
                Some(value.as_str())
            } else {
                None
            }
        })
}

fn field_text_value(fields: &[JsonField], key: &str) -> Option<String> {
    fields
        .iter()
        .find(|field| field.key() == key)
        .and_then(|field| match field.value() {
            crate::json::JsonValue::String(value) | crate::json::JsonValue::Number(value) => {
                Some(value.clone())
            }
            crate::json::JsonValue::Bool(value) => Some(value.to_string()),
            crate::json::JsonValue::Null
            | crate::json::JsonValue::Object(_)
            | crate::json::JsonValue::Array(_) => None,
        })
}

fn gpu_default_summary_from_render_fields(fields: &[RenderField]) -> String {
    let reason = render_field_value(fields, "gpu_worker_fallback_reason")
        .or_else(|| render_field_value(fields, "gpu_disabled_reason"))
        .or_else(|| render_field_value(fields, "gpu_unavailable_reason"))
        .or_else(|| render_field_value(fields, "backend_fallback_reason"));
    if let Some(reason) = meaningful_owned_reason(reason) {
        return format!("unavailable ({reason})");
    }

    match render_field_value(fields, "gpu_worker_state") {
        Some(state) if state == "available" => "available".to_owned(),
        Some(state) => state,
        None => "not-used".to_owned(),
    }
}

fn memory_default_summary_from_render_fields(fields: &[RenderField]) -> String {
    match render_field_value(fields, "memory_leak_report_clean").as_deref() {
        Some("true") => "clean".to_owned(),
        Some("false") => "not-clean".to_owned(),
        _ => "not-reported".to_owned(),
    }
}

fn render_field_or(fields: &[RenderField], key: &str, fallback: &str) -> String {
    render_field_value(fields, key).unwrap_or_else(|| fallback.to_owned())
}

fn render_field_value(fields: &[RenderField], key: &str) -> Option<String> {
    fields
        .iter()
        .find(|field| field.key() == key)
        .map(|field| field.value().as_text())
}

fn meaningful_reason(reason: Option<&str>) -> Option<&str> {
    reason.filter(|value| !matches!(*value, "none" | "not_requested" | "not-requested"))
}

fn meaningful_owned_reason(reason: Option<String>) -> Option<String> {
    reason.filter(|value| !matches!(value.as_str(), "none" | "not_requested" | "not-requested"))
}

#[cfg(test)]
#[path = "backend_summary_text_tests.rs"]
mod tests;
