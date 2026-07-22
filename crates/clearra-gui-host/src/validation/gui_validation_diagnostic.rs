use clearra_validation::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        suggested_next_step::SuggestedNextStep,
    },
    evidence::validation_evidence::ValidationEvidence,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GuiValidationDiagnostic;

impl GuiValidationDiagnostic {
    pub fn invalid_form(field: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(DiagnosticCode::EGuiFormInvalid, message)
            .with_evidence(ValidationEvidence::new("gui_field", field))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Fix the GUI form value before running clearra-app.",
            ))
    }
}
impl GuiValidationDiagnostic {
    pub fn unsafe_file_path(path_display: &str, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(DiagnosticCode::EGuiFilePathUnsafe, message)
            .with_evidence(ValidationEvidence::new("gui_file_path", path_display))
            .with_evidence(ValidationEvidence::new("redacted_path", path_display))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Choose a non-sensitive .json fixture path.",
            ))
    }
}
impl GuiValidationDiagnostic {
    pub fn backend_fallback_required(backend: &str, reason: &str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::WGuiBackendFallbackRequired,
            format!("GUI backend '{backend}' requires fallback: {reason}"),
        )
        .with_evidence(ValidationEvidence::new("gui_backend", backend))
        .with_evidence(ValidationEvidence::new("disabled_reason", reason))
    }
}
impl GuiValidationDiagnostic {
    pub fn render_unsupported(reason: &str) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::EGuiRenderUnsupported,
            format!("GUI render option is unsupported: {reason}"),
        )
        .with_evidence(ValidationEvidence::new("render_unsupported_reason", reason))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Select a validated PNG-atlas skin or disable bitmap export.",
        ))
    }
}
