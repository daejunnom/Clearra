use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::supply_diagnostic_builder::fixed_sequence_diagnostic;

pub(super) fn validate_fixed_sequence_at(
    sequence: &FixedSequence,
    path: &'static str,
) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    report.push(fixed_sequence_diagnostic(sequence, path));
    report
}
