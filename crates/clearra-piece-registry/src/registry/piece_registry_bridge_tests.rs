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
fn piece_registry_bridge_keeps_standard_fast_path_unaffected() {
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

    let bridge = PieceRegistryBridge::from_mixed_piece_set(&piece_set).expect("bridge");

    assert!(bridge.standard_fast_path_unaffected());
    assert_eq!(
        bridge.runtime_path().as_str(),
        PieceRegistryRuntimePath::StandardFastPath.as_str()
    );
    assert!(bridge.custom_operation_tables().is_empty());
    assert_eq!(bridge.generic_operation_descriptors().len(), 1);
    assert_eq!(
        bridge.generic_operation_descriptors()[0].operation_count(),
        28
    );
    assert_eq!(bridge.unsupported_reason(), None);
    assert_eq!(bridge.mixed_unsupported_reason(), None);
    assert_eq!(bridge.piece_area_multiset(), &[4, 4, 4, 4, 4, 4, 4]);
}

#[test]
fn piece_registry_bridge_exposes_custom_operation_schema_and_guard_reason() {
    let piece_set = MixedPieceSet::standard_plus_custom(
        "mixed-standard-tri",
        "Standard plus tri",
        vec![custom_piece("custom:tri-v1")],
    )
    .expect("piece set");

    let bridge = PieceRegistryBridge::from_mixed_piece_set(&piece_set).expect("bridge");

    assert!(!bridge.standard_fast_path_unaffected());
    assert_eq!(
        bridge.runtime_path(),
        PieceRegistryRuntimePath::UnsupportedExtension
    );
    assert_eq!(
        bridge.unsupported_reason(),
        Some("custom_piece_runtime_not_connected")
    );
    assert_eq!(
        bridge.mixed_unsupported_reason(),
        Some("mixed_piece_runtime_not_connected")
    );
    assert_eq!(bridge.custom_operation_tables().len(), 1);
    assert_eq!(bridge.generic_operation_descriptors().len(), 1);
    assert_eq!(
        bridge.generic_operation_descriptors()[0].candidate_runtime_guard_reason(),
        Some("custom_candidate_runtime_unsupported")
    );
    assert_eq!(bridge.custom_operation_tables()[0].piece_area(), 3);
    assert!(bridge
        .stable_piece_ids()
        .contains(&PieceDefinitionId::new("custom:tri-v1")));
}

#[test]
fn piece_definition_id_fingerprint_is_order_independent_and_id_sensitive() {
    let left = vec![
        PieceDefinitionId::new("custom:tri-v1"),
        PieceDefinitionId::new("std:I"),
    ];
    let reversed = vec![
        PieceDefinitionId::new("std:I"),
        PieceDefinitionId::new("custom:tri-v1"),
    ];
    let different = vec![
        PieceDefinitionId::new("std:I"),
        PieceDefinitionId::new("custom:tri-v2"),
    ];

    assert_eq!(
        piece_definition_id_fingerprint(&left),
        piece_definition_id_fingerprint(&reversed)
    );
    assert_ne!(
        piece_definition_id_fingerprint(&left),
        piece_definition_id_fingerprint(&different)
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
        PieceDisplayMetadata::default(),
        PieceSymmetryClass::MirrorX,
        "cells:0,0;1,0;0,1",
    )
    .expect("custom piece")
}
