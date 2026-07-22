use super::*;

#[test]
fn standard_7_bag_is_a_multiset_profile_with_one_of_each_piece() {
    let profile = BagProfile::standard_7();

    assert_eq!(profile.id(), "standard-7-bag");
    assert_eq!(profile.bag_size(), 7);
    assert_eq!(profile.multiplicity_for(PieceKind::I), 1);
    assert_eq!(profile.total_weight(), 7);
}

#[test]
fn arbitrary_multiset_bag_can_repeat_piece_kinds() {
    let profile = BagProfile::new(
        "double-i-bag",
        vec![
            BagProfileEntry::new(PieceKind::I, 2, 3),
            BagProfileEntry::new(PieceKind::O, 1, 1),
        ],
    )
    .expect("custom standard-piece bag");

    assert_eq!(profile.bag_size(), 3);
    assert_eq!(profile.multiplicity_for(PieceKind::I), 2);
    assert_eq!(profile.total_weight(), 4);
}

#[test]
fn repeated_entry_identity_is_rejected_because_multiplicity_owns_repetition() {
    let result = BagProfile::new(
        "bad",
        vec![
            BagProfileEntry::new(PieceKind::I, 1, 1),
            BagProfileEntry::new(PieceKind::I, 1, 1),
        ],
    );

    assert_eq!(
        result,
        Err(BagProfileError::DuplicatePiece {
            piece: PieceKind::I
        })
    );
}
