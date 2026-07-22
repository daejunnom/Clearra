use clearra_core_domain::{ids::piece_id::PieceDefinitionId, piece::piece_kind::PieceKind};
use clearra_supply::{
    bag::bag_profile::{BagProfile, BagProfileEntry},
    mixed::{
        BagBoundaryEvidence, CustomBagEntry, CustomBagProfile, SupplyProfile, SupplyProfileKind,
        SupplyProvenance,
    },
    queue::{
        bag_aligned_pattern::BagAlignedPattern, fixed_sequence::FixedSequence,
        observed_queue::ObservedQueue,
    },
};

use crate::diagnostic::{
    diagnostic_code::{DiagnosticCode, DiagnosticSeverity},
    diagnostic_report::DiagnosticReport,
};

use super::{
    validate_bag_aligned_pattern, validate_bag_aligned_pattern_with_bag_profile,
    validate_custom_bag_profile_mvp3_guard, validate_fixed_sequence, validate_observed_queue,
    validate_supply_profile_mvp3_guard,
};

#[test]
fn fixed_sequence_duplicate_is_allowed_because_boundary_is_not_implied() {
    let queue = FixedSequence::new(vec![PieceKind::I, PieceKind::O, PieceKind::I]);
    let report = validate_fixed_sequence(&queue);

    assert_report_contains(
        &report,
        DiagnosticCode::ISupplyFixedSequenceAccepted,
        DiagnosticSeverity::Info,
    );
    assert!(!report.has_errors());
}

#[test]
fn bag_aligned_pattern_duplicate_is_an_error() {
    let pattern = BagAlignedPattern::new(vec![PieceKind::I, PieceKind::O, PieceKind::I]);
    let report = validate_bag_aligned_pattern(&pattern);

    assert_report_contains(
        &report,
        DiagnosticCode::ESupplyInvalidDuplicate,
        DiagnosticSeverity::Error,
    );
    assert!(report.has_errors());
}

#[test]
fn invariant_observed_supply_ambiguity_is_warning_not_error() {
    let observed_window_ambiguity_reported = true;
    let queue = ObservedQueue::new(vec![PieceKind::I, PieceKind::O]);
    let report = validate_observed_queue(&queue);

    assert_report_contains(
        &report,
        DiagnosticCode::WSupplyAmbiguousObservedWindow,
        DiagnosticSeverity::Warning,
    );
    assert!(!report.has_errors());
    assert!(observed_window_ambiguity_reported);
}

#[test]
fn observed_queue_impossible_duplicate_is_an_error() {
    let queue = ObservedQueue::new(vec![PieceKind::I, PieceKind::I, PieceKind::I]);
    let report = validate_observed_queue(&queue);

    assert_report_contains(
        &report,
        DiagnosticCode::ESupplyInvalidDuplicate,
        DiagnosticSeverity::Error,
    );
    assert!(report.has_errors());
}

#[test]
fn bag_aligned_pattern_without_duplicate_gets_info() {
    let pattern = BagAlignedPattern::new(vec![
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::L,
    ]);
    let report = validate_bag_aligned_pattern(&pattern);

    assert_report_contains(
        &report,
        DiagnosticCode::ISupplyBoundaryCompatible,
        DiagnosticSeverity::Info,
    );
    assert!(!report.has_errors());
}

#[test]
fn custom_multiset_bag_profile_allows_repetition_up_to_multiplicity() {
    let bag_profile = BagProfile::new(
        "double-i-bag",
        vec![
            BagProfileEntry::new(PieceKind::I, 2, 1),
            BagProfileEntry::new(PieceKind::O, 1, 1),
        ],
    )
    .expect("bag profile");

    let accepted = validate_bag_aligned_pattern_with_bag_profile(
        &BagAlignedPattern::new(vec![PieceKind::I, PieceKind::I, PieceKind::O]),
        &bag_profile,
    );
    assert_report_contains(
        &accepted,
        DiagnosticCode::ISupplyBoundaryCompatible,
        DiagnosticSeverity::Info,
    );

    let rejected = validate_bag_aligned_pattern_with_bag_profile(
        &BagAlignedPattern::new(vec![PieceKind::I, PieceKind::I, PieceKind::I]),
        &bag_profile,
    );
    assert_report_contains(
        &rejected,
        DiagnosticCode::ESupplyInvalidDuplicate,
        DiagnosticSeverity::Error,
    );
}

#[test]
fn custom_bag_runtime_not_connected_until_runtime_exists() {
    let profile = CustomBagProfile::new(
        "tri-bag",
        "mixed-standard-tri",
        vec![CustomBagEntry::new(
            PieceDefinitionId::new("custom:tri-v1"),
            2,
            1,
        )],
    )
    .expect("custom bag profile");
    let report = validate_custom_bag_profile_mvp3_guard(&profile);

    assert!(profile.custom_bag_schema_valid());
    assert_report_contains(
        &report,
        DiagnosticCode::ECustomBagUnsupportedMvp,
        DiagnosticSeverity::Error,
    );
}

#[test]
fn supply_profile_guard_reports_custom_bag_runtime_not_connected() {
    let custom = CustomBagProfile::new(
        "tri-bag",
        "mixed-standard-tri",
        vec![CustomBagEntry::new(
            PieceDefinitionId::new("custom:tri-v1"),
            1,
            1,
        )],
    )
    .expect("custom bag profile");
    let provenance = SupplyProvenance::new(
        custom.bag_profile_id(),
        custom.piece_set_id(),
        None,
        BagBoundaryEvidence::NotEvaluated,
        false,
        false,
    )
    .expect("provenance");
    let profile = SupplyProfile::custom_bag_profile(&custom, provenance);
    let report = validate_supply_profile_mvp3_guard(&profile);

    assert_report_contains(
        &report,
        DiagnosticCode::ECustomBagUnsupportedMvp,
        DiagnosticSeverity::Error,
    );
}

#[test]
fn custom_bag_not_silent_standard_fallback() {
    let custom = CustomBagProfile::new(
        "tri-bag",
        "mixed-standard-tri",
        vec![CustomBagEntry::new(
            PieceDefinitionId::new("custom:tri-v1"),
            1,
            1,
        )],
    )
    .expect("custom bag profile");
    let provenance = SupplyProvenance::new(
        custom.bag_profile_id(),
        custom.piece_set_id(),
        None,
        BagBoundaryEvidence::NotEvaluated,
        false,
        false,
    )
    .expect("provenance");
    let profile = SupplyProfile::custom_bag_profile(&custom, provenance);
    let report = validate_supply_profile_mvp3_guard(&profile);

    assert!(matches!(
        profile.kind(),
        SupplyProfileKind::UnsupportedExtension(_)
    ));
    assert_ne!(profile.kind(), &SupplyProfileKind::Standard7Bag);
    assert_eq!(
        profile.runtime_guard_reason(),
        Some("custom_bag_runtime_not_connected")
    );
    assert_report_contains(
        &report,
        DiagnosticCode::ECustomBagUnsupportedMvp,
        DiagnosticSeverity::Error,
    );
}

fn assert_report_contains(
    report: &DiagnosticReport,
    code: DiagnosticCode,
    severity: DiagnosticSeverity,
) {
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| { diagnostic.code() == code && diagnostic.severity() == severity }));
}
