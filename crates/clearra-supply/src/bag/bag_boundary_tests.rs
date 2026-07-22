use clearra_core_domain::piece::piece_kind::PieceKind;

use super::*;

#[test]
fn fixed_queue_uses_zero_boundary_offset() {
    let report = standard_7_bag_fixed_boundary_report(&[
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::L,
    ]);

    assert_eq!(report.candidates(), &[BagBoundaryCandidate::new(0)]);
    assert!(report.duplicate_witness().is_none());
}

#[test]
fn observed_queue_keeps_multiple_boundary_candidates() {
    let report = standard_7_bag_observed_boundary_report(&[PieceKind::I, PieceKind::O]);

    assert!(report.is_compatible());
    assert!(report.candidates().len() > 1);
    assert!(report.is_ambiguous());
}

#[test]
fn impossible_observed_window_gets_duplicate_witness() {
    let report =
        standard_7_bag_observed_boundary_report(&[PieceKind::I, PieceKind::I, PieceKind::I]);

    assert!(!report.is_compatible());
    assert_eq!(
        report.duplicate_witness().map(|witness| witness.piece()),
        Some(PieceKind::I)
    );
}

#[test]
fn arbitrary_multiset_profile_drives_boundary_compatibility() {
    let profile = BagProfile::new(
        "double-i-bag",
        vec![
            crate::bag::bag_profile::BagProfileEntry::new(PieceKind::I, 2, 1),
            crate::bag::bag_profile::BagProfileEntry::new(PieceKind::O, 1, 1),
        ],
    )
    .expect("profile");

    let compatible = BagBoundaryReport::analyze_fixed_queue_with_profile(
        &[PieceKind::I, PieceKind::I, PieceKind::O],
        &profile,
    );
    assert!(compatible.is_compatible());

    let incompatible = BagBoundaryReport::analyze_fixed_queue_with_profile(
        &[PieceKind::I, PieceKind::I, PieceKind::I],
        &profile,
    );
    assert!(!incompatible.is_compatible());
    assert_eq!(
        incompatible
            .duplicate_witness()
            .map(|witness| witness.piece()),
        Some(PieceKind::I)
    );
}
