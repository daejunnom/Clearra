use clearra_two_line::capability::{
    two_line_capability::TwoLineCapability, two_line_fallback_reason::TwoLineFallbackReason,
};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

pub fn two_line_capability_report(capability: TwoLineCapability) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();

    match capability.fallback_reason() {
        None => report.push(
            Diagnostic::new(
                DiagnosticCode::IFastPathTwoLineEnabled,
                "two-line fast-path conditions are compatible; executor availability is checked during search dispatch",
            )
            .with_location(EvidenceLocation::new("capability.two_line")),
        ),
        Some(reason) => report.push(fallback_diagnostic(reason)),
    }

    report
}

fn fallback_diagnostic(reason: TwoLineFallbackReason) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        DiagnosticCode::WFastPathTwoLineDisabled,
        "two-line fast path is disabled; request should use generic search",
    )
    .with_location(EvidenceLocation::new("capability.two_line"))
    .with_evidence(ValidationEvidence::new("reason", reason.code()))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Use generic search for this request or adjust the profile to standard 10-wide 2L conditions.",
    ));

    diagnostic = match reason {
        TwoLineFallbackReason::ValidationFailed => {
            diagnostic.with_evidence(ValidationEvidence::new("validation_passed", "false"))
        }
        TwoLineFallbackReason::FastPathTableUnavailable => {
            diagnostic.with_evidence(ValidationEvidence::new("two_line_table", "unavailable"))
        }
        TwoLineFallbackReason::FastPathRunnerUnavailable => {
            diagnostic.with_evidence(ValidationEvidence::new("two_line_runner", "unavailable"))
        }
        TwoLineFallbackReason::UnsupportedBoardProfile { actual } => diagnostic.with_evidence(
            ValidationEvidence::new("board_profile", format!("{actual:?}")),
        ),
        TwoLineFallbackReason::UnsupportedBoardWidth { width } => {
            diagnostic.with_evidence(ValidationEvidence::new("width", width.to_string()))
        }
        TwoLineFallbackReason::UnsupportedTargetLines { lines } => {
            diagnostic.with_evidence(ValidationEvidence::new("target_lines", lines.to_string()))
        }
        TwoLineFallbackReason::UnsupportedHoldDisabled => {
            diagnostic.with_evidence(ValidationEvidence::new("hold_enabled", "false"))
        }
        TwoLineFallbackReason::UnsupportedPieceSet { actual } => {
            diagnostic.with_evidence(ValidationEvidence::new("piece_set", format!("{actual:?}")))
        }
        TwoLineFallbackReason::UnsupportedRuleProfile { actual } => diagnostic.with_evidence(
            ValidationEvidence::new("rule_profile", format!("{actual:?}")),
        ),
    };

    diagnostic
}

#[cfg(test)]
#[path = "two_line_capability_report_tests.rs"]
mod tests;
