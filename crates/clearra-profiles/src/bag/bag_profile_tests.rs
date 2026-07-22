use super::*;

#[test]
fn bag_profile_ids_expose_stable_canonical_strings() {
    assert_eq!(BagProfileId::Standard7Bag.as_str(), "standard-7-bag");
}

#[test]
fn standard_7_bag_profile_is_exposed_as_a_multiset_profile() {
    const PIECES: [PieceKind; 1] = [PieceKind::I];
    const ENTRIES: [BagProfileEntry; 1] = [BagProfileEntry::new(PieceKind::I, 2, 3)];
    let profile = BagProfile::new(
        BagProfileId::Standard7Bag,
        PieceSetProfileId::StandardTetrominoes,
        &PIECES,
        &ENTRIES,
    );

    assert_eq!(profile.bag_size(), 2);
    assert_eq!(profile.multiplicity_for(PieceKind::I), 2);
    assert_eq!(profile.total_weight(), 3);
}
