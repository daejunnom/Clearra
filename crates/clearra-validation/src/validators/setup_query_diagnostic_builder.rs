use clearra_setup_search::query::SetupSearchQuery;

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

pub(super) fn setup_supported_diagnostic(query: &SetupSearchQuery) -> Diagnostic {
    let probability_filter = query.probability_filter();
    Diagnostic::new(
        DiagnosticCode::ISetupQueryMvpSupported,
        "setup search query is valid for the MVP1 setup coverage path",
    )
    .with_location(EvidenceLocation::new("setup.query"))
    .with_evidence(ValidationEvidence::new(
        "hold_enabled",
        query.hold_policy().is_enabled().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "min_probability",
        probability_filter
            .min_probability()
            .map(|value| value.get().to_string())
            .unwrap_or_else(|| "none".to_owned()),
    ))
    .with_evidence(ValidationEvidence::new(
        "max_probability",
        probability_filter
            .max_probability()
            .map(|value| value.get().to_string())
            .unwrap_or_else(|| "none".to_owned()),
    ))
    .with_evidence(ValidationEvidence::new(
        "max_results",
        query.limits().max_results().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "post_pc_retained_trace_limit",
        query.limits().post_pc_retained_trace_limit().to_string(),
    ))
}

pub(super) fn invalid_setup_query(
    location: &'static str,
    message: &'static str,
    reason: &'static str,
) -> Diagnostic {
    Diagnostic::new(DiagnosticCode::ESetupQueryInvalid, message)
        .with_location(EvidenceLocation::new(location))
        .with_evidence(ValidationEvidence::new("reason", reason))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Use the standard MVP1 setup defaults or lower the requested setup budget.",
        ))
}
