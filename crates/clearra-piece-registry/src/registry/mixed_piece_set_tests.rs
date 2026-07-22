use clearra_core_domain::piece::rotation::RotationState;

use crate::{
    custom::{
        CustomPieceDefinition, CustomPieceRotation, PieceDisplayMetadata, PieceSpawnBounds,
        PieceSymmetryClass,
    },
    registry::piece_registry::ShapeCell,
};

use super::*;

#[test]
fn mixed_piece_set_keeps_stable_piece_ids_independent_of_entry_order() {
    let custom = custom_piece("custom:tri-v1");
    let piece_set = MixedPieceSet::standard_plus_custom(
        "mixed-standard-tri",
        "Standard plus triomino",
        vec![custom],
    )
    .expect("mixed piece set");

    assert_eq!(piece_set.id(), "mixed-standard-tri");
    assert_eq!(piece_set.len(), 8);
    assert!(piece_set.contains_custom());
    assert!(!piece_set.standard_fast_path_compatible());
    assert_eq!(piece_set.custom_piece_count(), 1);
    assert_eq!(piece_set.mixed_area_multiset()[7], 3);
    assert_eq!(piece_set.stable_piece_ids()[0].as_str(), "std:I");
    assert_eq!(piece_set.stable_piece_ids()[7].as_str(), "custom:tri-v1");
}

#[test]
fn mixed_piece_set_rejects_duplicate_stable_piece_ids() {
    let result = MixedPieceSet::new(
        "bad",
        "Bad",
        vec![
            MixedPieceSetEntry::Custom(custom_piece("custom:dup")),
            MixedPieceSetEntry::Custom(custom_piece("custom:dup")),
        ],
    );

    assert_eq!(
        result,
        Err(MixedPieceSetError::DuplicateStablePieceId {
            id: "custom:dup".to_owned()
        })
    );
}

fn custom_piece(id: &str) -> CustomPieceDefinition {
    CustomPieceDefinition::new(
        PieceDefinitionId::new(id),
        "Triomino",
        vec![CustomPieceRotation::new(
            RotationState::Zero,
            vec![
                ShapeCell::new(0, 0),
                ShapeCell::new(1, 0),
                ShapeCell::new(0, 1),
            ],
        )],
        PieceSpawnBounds::new(0, 2, 0, 2).expect("bounds"),
        PieceDisplayMetadata::new(Some("#ffcc00".to_owned()), Some("R".to_owned())),
        PieceSymmetryClass::MirrorX,
        "cells:0,0;1,0;0,1",
    )
    .expect("custom piece")
}
