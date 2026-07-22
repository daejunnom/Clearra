use clearra_supply::{
    bag::bag_profile::BagProfile,
    normalize::observed_window_analyzer::analyze_observed_window_with_bag_profile,
    queue::observed_queue::ObservedQueue,
};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

use super::supply_diagnostic_builder::{
    ambiguity_diagnostic, boundary_compatible_diagnostic, duplicate_diagnostic,
};

pub(super) fn validate_observed_queue_with_profile(
    queue: &ObservedQueue,
    bag_profile: &BagProfile,
) -> DiagnosticReport {
    let analysis = analyze_observed_window_with_bag_profile(queue, bag_profile);
    let boundary_report = analysis.boundary_report();
    let mut report = DiagnosticReport::new();

    if let Some(witness) = boundary_report.duplicate_witness() {
        report.push(duplicate_diagnostic(witness, "supply.observed_queue"));
        return report;
    }

    if !boundary_report.is_compatible() {
        report.push(
            Diagnostic::new(
                DiagnosticCode::ESupplyInvalidBagBoundary,
                "observed queue is incompatible with the configured bag boundary model",
            )
            .with_location(EvidenceLocation::new("supply.observed_queue"))
            .with_evidence(ValidationEvidence::new("bag_profile", bag_profile.id()))
            .with_evidence(ValidationEvidence::new(
                "candidate_count",
                boundary_report.candidates().len().to_string(),
            ))
            .with_suggested_next_step(SuggestedNextStep::new(
                "Check the observed queue or provide an explicit fixed queue.",
            )),
        );
        return report;
    }

    if let Some(ambiguity) = analysis.ambiguity_report() {
        report.push(ambiguity_diagnostic(ambiguity));
    } else {
        report.push(boundary_compatible_diagnostic(
            boundary_report,
            "supply.observed_queue",
        ));
    }

    report
}
