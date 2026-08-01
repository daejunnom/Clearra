use clearra_core_domain::objective::objective_kind::ObjectiveKind;
use clearra_coverage::cover::{CoverSelection, CoverSelectionLimit};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ObjectiveValidator;

impl ObjectiveValidator {
    pub fn validate_kind(kind: ObjectiveKind) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();
        match kind {
            ObjectiveKind::All
            | ObjectiveKind::Unique
            | ObjectiveKind::MinimumCover
            | ObjectiveKind::Tiling => {
                report.push(
                    Diagnostic::new(
                        DiagnosticCode::IObjectiveMvpSupported,
                        "objective is supported in MVP1",
                    )
                    .with_location(EvidenceLocation::new("objective.kind"))
                    .with_evidence(ValidationEvidence::new("objective", format!("{:?}", kind))),
                );
            }
        }
        report
    }
}
impl ObjectiveValidator {
    pub fn cover_selection_diagnostics(selection: &CoverSelection) -> Vec<Diagnostic> {
        match selection.limit() {
            CoverSelectionLimit::None => Vec::new(),
            CoverSelectionLimit::ExactSearchRowLimitExceeded { row_count, limit } => {
                vec![Diagnostic::new(
                    DiagnosticCode::WMinimumCoverGreedyFallback,
                    format!(
                        "Minimum cover exact search was skipped for {row_count} rows because the exact row limit is {limit}; the result is a greedy fallback and may not be minimal."
                    ),
                )]
            }
        }
    }
}

pub fn validate_objective_kind(kind: ObjectiveKind) -> DiagnosticReport {
    ObjectiveValidator::validate_kind(kind)
}

#[cfg(test)]
#[path = "objective_validator_tests.rs"]
mod tests;
