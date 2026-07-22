use clearra_core_domain::{ids::piece_id::PieceDefinitionId, piece::rotation::RotationState};

use crate::{
    custom::{
        CustomOperationTableSchema, CustomPieceDefinition, CustomPieceRotation,
        PieceDisplayMetadata, PieceSpawnBounds, PieceSymmetryClass,
        CUSTOM_OPERATION_TABLE_SCHEMA_VERSION,
    },
    registry::piece_registry::ShapeCell,
};

#[test]
fn custom_operation_table_schema_preserves_piece_area_and_rotation_states() {
    let definition = custom_piece_with_reversed_rotations("custom:tri-v1");

    let table = CustomOperationTableSchema::from_definition(&definition).expect("table");

    assert_eq!(table.piece_id().as_str(), "custom:tri-v1");
    assert_eq!(table.piece_area(), 3);
    assert_eq!(
        table.schema_version(),
        CUSTOM_OPERATION_TABLE_SCHEMA_VERSION
    );
    assert_eq!(
        table.rotation_states(),
        vec![RotationState::Zero, RotationState::Right]
    );
    assert_eq!(table.operations()[0].operation_id(), 0);
    assert_eq!(table.operations()[0].piece_area(), 3);
    assert_eq!(table.operations()[0].bounds().width(), 2);
    assert_eq!(table.operations()[0].bounds().height(), 2);
    assert!(table.operations()[0]
        .stable_key()
        .starts_with("custom:tri-v1:r0:"));
}

#[test]
fn custom_operation_table_fingerprint_uses_stable_piece_definition_id() {
    let left = CustomOperationTableSchema::from_definition(&custom_piece_with_reversed_rotations(
        "custom:tri-v1",
    ))
    .expect("left");
    let right = CustomOperationTableSchema::from_definition(&custom_piece_with_reversed_rotations(
        "custom:tri-v2",
    ))
    .expect("right");

    assert_ne!(
        left.piece_definition_fingerprint(),
        right.piece_definition_fingerprint()
    );
}

fn custom_piece_with_reversed_rotations(id: &str) -> CustomPieceDefinition {
    CustomPieceDefinition::new(
        PieceDefinitionId::new(id),
        "Triomino",
        vec![
            CustomPieceRotation::new(
                RotationState::Right,
                vec![
                    ShapeCell::new(0, 0),
                    ShapeCell::new(0, 1),
                    ShapeCell::new(1, 1),
                ],
            ),
            CustomPieceRotation::new(
                RotationState::Zero,
                vec![
                    ShapeCell::new(0, 0),
                    ShapeCell::new(1, 0),
                    ShapeCell::new(0, 1),
                ],
            ),
        ],
        PieceSpawnBounds::new(0, 2, 0, 2).expect("bounds"),
        PieceDisplayMetadata::default(),
        PieceSymmetryClass::MirrorX,
        "cells:0,0;1,0;0,1",
    )
    .expect("custom piece")
}
