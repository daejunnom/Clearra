use crate::{
    json::json_contract::JsonContract,
    model::render_field_value::{RenderField, RenderFieldValue},
    text::{
        backend_summary_text::BackendSummaryText, diagnostic_field_policy::DiagnosticFieldPolicy,
        human_summary_field_policy::HumanSummaryFieldPolicy,
        text_output_profile::TextOutputProfile, text_writer::TextWriter,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderMessage {
    kind: String,
    fields: Vec<RenderField>,
}

impl RenderMessage {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            fields: Vec::new(),
        }
    }
}
impl RenderMessage {
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields
            .push(RenderField::new(key, RenderFieldValue::string(value)));
        self
    }
}
impl RenderMessage {
    pub fn with_value(
        mut self,
        key: impl Into<String>,
        value: impl Into<RenderFieldValue>,
    ) -> Self {
        self.fields.push(RenderField::new(key, value));
        self
    }
}
impl RenderMessage {
    pub fn kind(&self) -> &str {
        &self.kind
    }
}
impl RenderMessage {
    pub fn fields(&self) -> &[RenderField] {
        &self.fields
    }
}
impl RenderMessage {
    pub fn text_lines(&self) -> Vec<String> {
        self.text_lines_with_profile(TextOutputProfile::HumanSummary)
    }
}
impl RenderMessage {
    pub fn text_lines_with_profile(&self, profile: TextOutputProfile) -> Vec<String> {
        std::iter::once(TextWriter::line("kind", &self.kind))
            .chain(backend_summary_lines(&self.fields, profile))
            .chain(
                self.fields
                    .iter()
                    .filter(|field| include_in_text_profile(&self.kind, field.key(), profile))
                    .map(|field| TextWriter::line(field.key(), text_field_value(field))),
            )
            .collect()
    }
}

fn text_field_value(field: &RenderField) -> String {
    let raw = field.value().as_text();
    if field.key() != "probability"
        && !field.key().ends_with("_probability")
        && !field.key().ends_with("_probability_mass")
    {
        return raw;
    }
    let Ok(probability) = raw.parse::<f64>() else {
        return raw;
    };
    if !probability.is_finite() || !(0.0..=1.0).contains(&probability) {
        return raw;
    }
    let rendered = format!("{:.12}", probability * 100.0);
    format!("{}%", rendered.trim_end_matches('0').trim_end_matches('.'))
}
impl RenderMessage {
    pub fn json_contract(&self) -> JsonContract {
        JsonContract::from_render_message(&self.kind, &self.fields)
    }
}
impl RenderMessage {
    pub fn fumen_pages(&self) -> Vec<String> {
        let fields = std::iter::once(("kind".to_owned(), self.kind.clone()))
            .chain(
                self.fields
                    .iter()
                    .filter(|field| include_in_fumen_page(field.key()))
                    .map(|field| (field.key().to_owned(), field.value().as_text())),
            )
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>();
        vec![fields.join("\n")]
    }
}

fn backend_summary_lines(
    fields: &[RenderField],
    profile: TextOutputProfile,
) -> impl Iterator<Item = String> {
    if !has_backend_summary_fields(fields) {
        return Vec::new().into_iter();
    }

    match profile {
        TextOutputProfile::HumanSummary => {
            BackendSummaryText::default_lines_from_render_fields(fields)
        }
        TextOutputProfile::Verbose => BackendSummaryText::verbose_lines_from_render_fields(fields),
        TextOutputProfile::Diagnostics => Vec::new(),
    }
    .into_iter()
}

fn has_backend_summary_fields(fields: &[RenderField]) -> bool {
    fields.iter().any(|field| {
        matches!(
            field.key(),
            "backend_selected"
                | "selected_backend"
                | "gpu_worker_state"
                | "gpu_trust_state"
                | "gpu_unavailable_reason"
                | "backend_fallback_reason"
                | "memory_leak_report_clean"
        )
    })
}

fn include_in_text_profile(kind: &str, key: &str, profile: TextOutputProfile) -> bool {
    match profile {
        TextOutputProfile::HumanSummary => HumanSummaryFieldPolicy::include_field(kind, key),
        TextOutputProfile::Verbose => true,
        TextOutputProfile::Diagnostics => DiagnosticFieldPolicy::include_field(kind, key),
    }
}

fn include_in_fumen_page(key: &str) -> bool {
    matches!(
        key,
        "action"
            | "actual_solution_set_contract"
            | "actual_normalized_solution_set_hash"
            | "actual_normalized_unique_solution_count"
            | "backend_fallback_reason"
            | "backend_requested"
            | "backend_selected"
            | "buildup_backend"
            | "candidate_backend"
            | "checkpoints"
            | "checkpoint_schedule_checkpoint_count"
            | "continue_available"
            | "continuation_token_available"
            | "continuation_token_unavailable_reason"
            | "count_complete"
            | "coverage_probability"
            | "cpu_confirmed"
            | "gpu_confirmed"
            | "lines"
            | "next_pc_available"
            | "normalized_solution_key_algorithm"
            | "normalized_solution_set_hash"
            | "normalized_unique_solution_count"
            | "objective_applied"
            | "objective_execution"
            | "objective_search_mode"
            | "queue_len"
            | "queue_mode"
            | "retained_trace_count"
            | "route"
            | "sample_trace_available"
            | "solution_found"
            | "solver_backend"
            | "status"
            | "supply_expansion_truncated"
            | "supply_pattern_count"
            | "supply_probability_complete"
            | "supply_probability_model"
            | "total_solution_count"
            | "trace_retention_reason"
            | "trace_retention_truncated"
            | "two_line_capable"
            | "two_line_fallback_reason"
            | "two_line_fast_path_available"
            | "unique_solution_count"
    )
}

#[cfg(test)]
#[path = "render_message_tests.rs"]
mod tests;
