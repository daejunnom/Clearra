use clearra_app::AppResponse;

use super::{bool_field, field_value, first_field, response_status, result_kind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiSummaryPanel {
    label_i18n_key: &'static str,
    kind: String,
    status: String,
    preset: String,
    solution_count: String,
    coverage_probability: String,
    continuation_available: bool,
}

impl GuiSummaryPanel {
    pub fn from_response(response: &AppResponse) -> Self {
        Self {
            label_i18n_key: "ui.result.summary",
            kind: result_kind(response),
            status: response_status(response).to_owned(),
            preset: field_value(response, "problem_preset").unwrap_or_else(|| "none".to_owned()),
            solution_count: first_field(response, &["total_solution_count", "solution_count"])
                .unwrap_or_else(|| "0".to_owned()),
            coverage_probability: first_field(response, &["coverage_probability", "probability"])
                .unwrap_or_else(|| "none".to_owned()),
            continuation_available: bool_field(response, "continuation_token_available")
                || bool_field(response, "next_pc_available")
                || bool_field(response, "continue_available"),
        }
    }
}
impl GuiSummaryPanel {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl GuiSummaryPanel {
    pub fn kind(&self) -> &str {
        &self.kind
    }
}
impl GuiSummaryPanel {
    pub fn status(&self) -> &str {
        &self.status
    }
}
impl GuiSummaryPanel {
    pub fn preset(&self) -> &str {
        &self.preset
    }
}
impl GuiSummaryPanel {
    pub fn solution_count(&self) -> &str {
        &self.solution_count
    }
}
impl GuiSummaryPanel {
    pub fn coverage_probability(&self) -> &str {
        &self.coverage_probability
    }
}
impl GuiSummaryPanel {
    pub const fn continuation_available(&self) -> bool {
        self.continuation_available
    }
}
