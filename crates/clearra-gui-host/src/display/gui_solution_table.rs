use clearra_app::AppResponse;

use super::{bool_field, field_value, first_field};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiSolutionRow {
    solution_id: String,
    piece_order: String,
    variant_id: String,
    score_summary: String,
    trace_available: bool,
}

impl GuiSolutionRow {
    fn from_response(response: &AppResponse) -> Self {
        Self {
            solution_id: first_field(response, &["solution_id", "next_pc_candidate"])
                .unwrap_or_else(|| "solution-0".to_owned()),
            piece_order: first_field(response, &["piece_order", "remaining_queue_preview"])
                .unwrap_or_else(|| "none".to_owned()),
            variant_id: first_field(response, &["variant_id", "build_variant_id"])
                .unwrap_or_else(|| "representative".to_owned()),
            score_summary: first_field(response, &["score_summary", "score_best_score"])
                .unwrap_or_else(|| "none".to_owned()),
            trace_available: bool_field(response, "sample_trace_available")
                || field_value(response, "retained_trace_count")
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0)
                    > 0,
        }
    }
}
impl GuiSolutionRow {
    pub fn solution_id(&self) -> &str {
        &self.solution_id
    }
}
impl GuiSolutionRow {
    pub fn piece_order(&self) -> &str {
        &self.piece_order
    }
}
impl GuiSolutionRow {
    pub fn variant_id(&self) -> &str {
        &self.variant_id
    }
}
impl GuiSolutionRow {
    pub fn score_summary(&self) -> &str {
        &self.score_summary
    }
}
impl GuiSolutionRow {
    pub const fn trace_available(&self) -> bool {
        self.trace_available
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiSolutionTable {
    label_i18n_key: &'static str,
    rows: Vec<GuiSolutionRow>,
}

impl GuiSolutionTable {
    pub fn from_response(response: &AppResponse) -> Self {
        let has_solution = bool_field(response, "solution_found")
            || first_field(response, &["total_solution_count", "solution_count"])
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0)
                > 0;
        let rows = if has_solution {
            vec![GuiSolutionRow::from_response(response)]
        } else {
            Vec::new()
        };
        Self {
            label_i18n_key: "ui.result.solutions",
            rows,
        }
    }
}
impl GuiSolutionTable {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl GuiSolutionTable {
    pub fn rows(&self) -> &[GuiSolutionRow] {
        &self.rows
    }
}
