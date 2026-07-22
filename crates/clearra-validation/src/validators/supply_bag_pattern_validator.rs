use clearra_supply::{
    bag::{bag_boundary::BagBoundaryReport, bag_profile::BagProfile},
    queue::bag_aligned_pattern::BagAlignedPattern,
};

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::supply_diagnostic_builder::{boundary_compatible_diagnostic, duplicate_diagnostic};

pub(super) fn validate_bag_aligned_pattern_with_profile(
    pattern: &BagAlignedPattern,
    bag_profile: &BagProfile,
) -> DiagnosticReport {
    let boundary_report =
        BagBoundaryReport::analyze_fixed_queue_with_profile(pattern.pieces(), bag_profile);
    let mut report = DiagnosticReport::new();

    if let Some(witness) = boundary_report.duplicate_witness() {
        report.push(duplicate_diagnostic(witness, "supply.bag_aligned_pattern"));
    } else {
        report.push(boundary_compatible_diagnostic(
            &boundary_report,
            "supply.bag_aligned_pattern",
        ));
    }

    report
}
