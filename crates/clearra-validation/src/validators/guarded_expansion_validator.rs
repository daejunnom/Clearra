use clearra_rules::custom::CustomKickExactnessGuard;

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::validation_evidence::ValidationEvidence,
};

pub fn validate_custom_kick_before_c_execution(
    guard: CustomKickExactnessGuard,
) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    if let Some(reason) = guard.disabled_reason() {
        report.push(
            Diagnostic::new(
                DiagnosticCode::ERuleUnsupportedMvp,
                "unverified custom kick profile is rejected before C execution",
            )
            .with_evidence(ValidationEvidence::new("reason", reason))
            .with_evidence(ValidationEvidence::new(
                "source_kind",
                guard.source_kind().as_str(),
            ))
            .with_evidence(ValidationEvidence::new(
                "supports_exact_180",
                guard.supports_exact_180().to_string(),
            ))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Import and verify the custom kick profile before enabling it for runtime search.",
            )),
        );
    }
    report
}

#[cfg(test)]
mod tests {
    use clearra_rules::custom::CustomKickExactnessGuard;

    use crate::diagnostic::diagnostic_code::DiagnosticCode;

    use super::*;

    #[test]
    fn unverified_custom_kick_rejected_before_c_execution() {
        let report =
            validate_custom_kick_before_c_execution(CustomKickExactnessGuard::unverified_custom());

        assert!(report.has_errors());
        assert!(report.contains_code(DiagnosticCode::ERuleUnsupportedMvp));
        assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
            .evidence()
            .iter()
            .any(|evidence| evidence.key() == "reason"
                && evidence.value() == "unverified_custom_kick_rejected_before_c_execution")));
    }
}
