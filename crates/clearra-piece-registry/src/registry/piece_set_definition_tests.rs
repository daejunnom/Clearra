use clearra_core_domain::{
    ids::piece_id::PieceDefinitionId,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};

use crate::{
    custom::{
        CustomPieceDefinition, CustomPieceRotation, PieceDisplayMetadata, PieceSpawnBounds,
        PieceSymmetryClass,
    },
    registry::piece_registry::ShapeCell,
};

use super::*;

#[test]
fn piece_set_definition_preserves_custom_identity_for_cache_keys() {
    let piece_set = MixedPieceSet::standard_plus_custom(
        "mixed-standard-tri",
        "Mixed standard tri",
        vec![custom_piece("custom:tri-v1")],
    )
    .expect("piece set");

    let definition = PieceSetDefinition::from_mixed_piece_set(&piece_set);

    assert_eq!(definition.piece_set_id(), "mixed-standard-tri");
    assert!(!definition.standard_fast_path_compatible());
    assert_eq!(definition.mixed_area_multiset().last(), Some(&3));
    assert_ne!(definition.piece_definition_id_fingerprint(), 0);
    assert_ne!(definition.piece_area_multiset_fingerprint(), 0);
    assert_ne!(definition.piece_set_profile_id(), 0);
}

#[test]
fn standard_piece_set_definition_keeps_fast_path_compatible() {
    let piece_set = MixedPieceSet::new(
        "standard-seven",
        "Standard seven",
        PieceKind::STANDARD_TETROMINOES
            .iter()
            .copied()
            .map(MixedPieceSetEntry::Standard)
            .collect(),
    )
    .expect("piece set");

    let definition = PieceSetDefinition::from_mixed_piece_set(&piece_set);

    assert!(definition.standard_fast_path_compatible());
    assert_eq!(definition.mixed_area_multiset(), &[4, 4, 4, 4, 4, 4, 4]);
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
