use clearra_app::AppResponse;

use super::{field_value, first_field};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiCoveragePanel {
    label_i18n_key: &'static str,
    pattern_universe_id: String,
    covered_pattern_count: String,
    probability: String,
    probability_complete: bool,
    truncation_reason: String,
}

impl GuiCoveragePanel {
    pub fn from_response(response: &AppResponse) -> Self {
        Self {
            label_i18n_key: "ui.result.coverage",
            pattern_universe_id: field_value(response, "pattern_universe_id")
                .unwrap_or_else(|| "unknown".to_owned()),
            covered_pattern_count: field_value(response, "covered_pattern_count")
                .unwrap_or_else(|| "0".to_owned()),
            probability: first_field(response, &["coverage_probability", "probability"])
                .unwrap_or_else(|| "none".to_owned()),
            probability_complete: first_field(
                response,
                &["probability_complete", "count_complete"],
            )
            .and_then(|value| value.parse().ok())
            .unwrap_or(false),
            truncation_reason: first_field(
                response,
                &[
                    "truncation_reason",
                    "observed_queue_truncation_reason",
                    "trace_retention_reason",
                ],
            )
            .unwrap_or_else(|| "none".to_owned()),
        }
    }
}
impl GuiCoveragePanel {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl GuiCoveragePanel {
    pub fn pattern_universe_id(&self) -> &str {
        &self.pattern_universe_id
    }
}
impl GuiCoveragePanel {
    pub fn covered_pattern_count(&self) -> &str {
        &self.covered_pattern_count
    }
}
impl GuiCoveragePanel {
    pub fn probability(&self) -> &str {
        &self.probability
    }
}
impl GuiCoveragePanel {
    pub const fn probability_complete(&self) -> bool {
        self.probability_complete
    }
}
impl GuiCoveragePanel {
    pub fn truncation_reason(&self) -> &str {
        &self.truncation_reason
    }
}
