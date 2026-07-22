use crate::{
    model::GuiRenderForm,
    validation::{GuiValidationDiagnostic, GuiValidationSummary},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GuiRenderValidator;

impl GuiRenderValidator {
    pub fn validate(form: &GuiRenderForm) -> GuiValidationSummary {
        let mut summary = GuiValidationSummary::new();

        if form.render_enabled() && form.skin_id().trim().is_empty() {
            summary.push(GuiValidationDiagnostic::render_unsupported(
                "skin_id_missing",
            ));
        }

        if form.exact_render_required() && !form.render_enabled() {
            summary.push(GuiValidationDiagnostic::render_unsupported(
                "exact_render_requires_render_enabled",
            ));
        }
        if form.render_enabled() && form.unsupported_reason().is_some() {
            summary.push(GuiValidationDiagnostic::render_unsupported(
                form.unsupported_reason().expect("checked above"),
            ));
        }

        summary
    }
}
