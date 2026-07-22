use clearra_core_domain::piece::piece_kind::PieceKind;

use super::*;

#[test]
fn detects_duplicate_inside_fixed_bag() {
    let witness = duplicate_for_boundary_offset(&[PieceKind::I, PieceKind::O, PieceKind::I], 7, 0)
        .expect("duplicate I");

    assert_eq!(witness.piece(), PieceKind::I);
    assert_eq!(witness.first_index(), 0);
    assert_eq!(witness.duplicate_index(), 2);
}

#[test]
fn allows_same_piece_across_boundary() {
    assert_eq!(
        duplicate_for_boundary_offset(&[PieceKind::I, PieceKind::I], 7, 6),
        None
    );
}

#[test]
fn multiplicity_profile_allows_repeated_piece_until_profile_count_is_exceeded() {
    let profile = BagProfile::new(
        "double-i",
        vec![
            crate::bag::bag_profile::BagProfileEntry::new(PieceKind::I, 2, 1),
            crate::bag::bag_profile::BagProfileEntry::new(PieceKind::O, 1, 1),
        ],
    )
    .expect("bag profile");

    assert_eq!(
        duplicate_for_boundary_offset_with_profile(&[PieceKind::I, PieceKind::I], &profile, 0),
        None
    );
    assert_eq!(
        duplicate_for_boundary_offset_with_profile(
            &[PieceKind::I, PieceKind::I, PieceKind::I],
            &profile,
            0
        )
        .map(|witness| witness.piece()),
        Some(PieceKind::I)
    );
}
