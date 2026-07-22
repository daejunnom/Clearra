use clearra_piece_registry::registry::{MixedBagProfile, MixedPieceSet, MixedPieceSetEntry};

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::piece_set_diagnostic_builder::{
    custom_bag_runtime_guard_diagnostic, custom_piece_runtime_guard_diagnostic,
    standard_mixed_bag_profile_supported_diagnostic,
    standard_only_mixed_piece_set_supported_diagnostic,
};

pub(super) fn validate_mixed_piece_set_mvp3_guard_impl(
    piece_set: &MixedPieceSet,
) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    if piece_set.contains_custom() {
        let custom_ids = piece_set
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                MixedPieceSetEntry::Custom(definition) => Some(definition.id().as_str()),
                MixedPieceSetEntry::Standard(_) => None,
            })
            .collect::<Vec<_>>()
            .join(",");
        report.push(custom_piece_runtime_guard_diagnostic(piece_set, custom_ids));
        return report;
    }

    report.push(standard_only_mixed_piece_set_supported_diagnostic(
        piece_set,
    ));
    report
}

pub(super) fn validate_mixed_bag_profile_mvp3_guard_impl(
    piece_set: &MixedPieceSet,
    bag_profile: &MixedBagProfile,
) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    let has_custom_bag_entry = bag_profile
        .entries()
        .iter()
        .any(|entry| !entry.piece_id().as_str().starts_with("std:"));

    if piece_set.contains_custom() || has_custom_bag_entry {
        report.push(custom_bag_runtime_guard_diagnostic(piece_set, bag_profile));
        return report;
    }

    report.push(standard_mixed_bag_profile_supported_diagnostic(bag_profile));
    report
}
