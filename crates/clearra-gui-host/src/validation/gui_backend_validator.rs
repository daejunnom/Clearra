use crate::{
    model::{GuiBackendChoice, GuiBackendForm},
    validation::{GuiValidationDiagnostic, GuiValidationSummary},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GuiBackendValidator;

impl GuiBackendValidator {
    pub fn validate(form: &GuiBackendForm) -> GuiValidationSummary {
        let mut summary = GuiValidationSummary::new();
        let parsed = GuiBackendChoice::parse(form.backend_id());

        if parsed.is_none() {
            summary.push(GuiValidationDiagnostic::invalid_form(
                "backend",
                format!(
                    "unknown GUI backend option '{}'; expected auto, cpu, gpu, or hybrid",
                    form.backend_id()
                ),
            ));
            return summary;
        }

        if form.workers() == 0 {
            summary.push(GuiValidationDiagnostic::invalid_form(
                "workers",
                "GUI backend form requires at least one worker",
            ));
        }
        if form.memory_budget_mb() == 0 {
            summary.push(GuiValidationDiagnostic::invalid_form(
                "memory_budget_mb",
                "GUI backend form requires a nonzero memory budget",
            ));
        }
        if form.candidate_budget() == 0 {
            summary.push(GuiValidationDiagnostic::invalid_form(
                "candidate_budget",
                "GUI backend form requires a nonzero candidate budget",
            ));
        }
        if form.pattern_budget() == 0 {
            summary.push(GuiValidationDiagnostic::invalid_form(
                "pattern_budget",
                "GUI backend form requires a nonzero pattern budget",
            ));
        }

        summary
    }
}
