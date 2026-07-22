use clearra_core_domain::{
    ids::piece_id::PieceDefinitionId,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_piece_registry::{
    custom::{
        CustomPieceDefinition, CustomPieceRotation, PieceDisplayMetadata, PieceSourceProvenance,
        PieceSpawnBounds, PieceSpawnOffset, PieceSymmetryClass,
    },
    registry::{
        piece_registry::ShapeCell, MixedPieceSet, MixedPieceSetEntry, PieceRegistryBridge,
        PieceRegistryRuntimePath, PieceSetDefinition,
    },
};

#[test]
fn custom_piece_schema_validates() {
    let definition = custom_piece("custom:tri-v1")
        .with_spawn_offsets(vec![PieceSpawnOffset::new(RotationState::Zero, 4, 20)])
        .with_source_provenance(PieceSourceProvenance::new(
            Some("human-verified fixture".to_owned()),
            None,
            Some("project-test-fixture".to_owned()),
        ));

    assert_eq!(definition.piece_definition_id().as_str(), "custom:tri-v1");
    assert_eq!(definition.display_name(), "Triomino");
    assert_eq!(definition.area(), 3);
    assert_eq!(definition.rotation_states(), vec![RotationState::Zero]);
    assert_eq!(definition.cells_by_rotation()[0].1.len(), 3);
    assert_eq!(definition.bounds_by_rotation()[0].max_x(), 1);
    assert_eq!(definition.spawn_offsets()[0].x(), 4);
    assert_eq!(definition.color_hint(), Some("#ffcc00"));
    assert_eq!(definition.symmetry_class(), PieceSymmetryClass::MirrorX);
    assert_eq!(
        definition.source_provenance().source_label(),
        Some("human-verified fixture")
    );
}

#[test]
fn custom_piece_runtime_not_connected_until_runtime_exists() {
    let piece_set = MixedPieceSet::standard_plus_custom(
        "mixed-standard-tri",
        "Mixed standard tri",
        vec![custom_piece("custom:tri-v1")],
    )
    .expect("piece set");

    let bridge = PieceRegistryBridge::from_mixed_piece_set(&piece_set).expect("bridge");

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
}

#[test]
fn custom_piece_schema_validates_but_runtime_guarded() {
    custom_piece_schema_validates();
    custom_piece_runtime_not_connected_until_runtime_exists();
}

#[test]
fn standard_tetromino_fast_path_unchanged() {
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

    assert!(piece_set.standard_fast_path_compatible());
    assert!(definition.standard_fast_path_compatible());
    assert_eq!(definition.mixed_area_multiset(), &[4, 4, 4, 4, 4, 4, 4]);
}

#[test]
fn piece_definition_id_included_in_cache_keys() {
    let piece_set = MixedPieceSet::standard_plus_custom(
        "mixed-standard-tri",
        "Mixed standard tri",
        vec![custom_piece("custom:tri-v1")],
    )
    .expect("piece set");
    let definition = PieceSetDefinition::from_mixed_piece_set(&piece_set);
    let bridge = PieceRegistryBridge::from_mixed_piece_set(&piece_set).expect("bridge");

    assert_eq!(
        definition.piece_definition_id_fingerprint(),
        bridge.piece_definition_id_fingerprint()
    );
    assert_eq!(
        definition.piece_area_multiset_fingerprint(),
        bridge.piece_area_multiset_fingerprint()
    );
    assert_eq!(
        definition.piece_set_profile_id(),
        bridge.piece_set_profile_id()
    );
}

#[test]
fn generic_cache_key_includes_piece_definition_id() {
    piece_definition_id_included_in_cache_keys();
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
