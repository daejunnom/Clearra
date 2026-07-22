use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

pub(crate) fn invalid_execution_policy(
    location: &'static str,
    message: &'static str,
    reason: &'static str,
) -> Diagnostic {
    Diagnostic::new(DiagnosticCode::EPcQueryInvalid, message)
        .with_location(EvidenceLocation::new(location))
        .with_evidence(ValidationEvidence::new("reason", reason))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Use --backend auto or --backend cpu for the currently supported PC search backend.",
        ))
}
