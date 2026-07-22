use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::registry::{MixedPieceSet, MixedPieceSetEntry};

use super::*;

#[test]
fn mixed_bag_profile_references_piece_set_stable_ids_with_multiplicity_and_weight() {
    let piece_set = MixedPieceSet::new(
        "standard-subset",
        "Standard subset",
        vec![
            MixedPieceSetEntry::Standard(PieceKind::I),
            MixedPieceSetEntry::Standard(PieceKind::O),
        ],
    )
    .expect("piece set");

    let bag = MixedBagProfile::new(
        "double-i-bag",
        &piece_set,
        vec![
            MixedBagEntry::new(PieceDefinitionId::new("std:I"), 2, 3),
            MixedBagEntry::new(PieceDefinitionId::new("std:O"), 1, 1),
        ],
        BagBoundaryModels::all_mvp3_models(),
    )
    .expect("bag profile");

    assert_eq!(bag.piece_set_id(), "standard-subset");
    assert!(bag.mixed_bag_schema_validates());
    assert_eq!(bag.bag_size(), 3);
    assert_eq!(bag.total_weight(), 4);
    assert!(bag.boundary_models().observed_window());
}

#[test]
fn mixed_bag_profile_rejects_piece_ids_not_owned_by_piece_set() {
    let piece_set = MixedPieceSet::new(
        "standard-subset",
        "Standard subset",
        vec![MixedPieceSetEntry::Standard(PieceKind::I)],
    )
    .expect("piece set");

    let result = MixedBagProfile::new(
        "bad",
        &piece_set,
        vec![MixedBagEntry::new(
            PieceDefinitionId::new("custom:tri-v1"),
            1,
            1,
        )],
        BagBoundaryModels::all_mvp3_models(),
    );

    assert_eq!(
        result,
        Err(MixedBagProfileError::UnknownPieceId {
            piece_id: PieceDefinitionId::new("custom:tri-v1")
        })
    );
}
