use clearra_core_domain::{ids::piece_id::PieceDefinitionId, piece::rotation::RotationState};

use crate::{
    custom::{
        CustomOperationTableSchema, CustomPieceDefinition, CustomPieceRotation,
        PieceDisplayMetadata, PieceSpawnBounds, PieceSymmetryClass,
    },
    registry::piece_registry::ShapeCell,
};

use super::*;

#[test]
fn standard_operation_table_unchanged_marker() {
    let descriptor =
        GenericOperationTableDescriptor::from_standard(StandardTetrominoOperationTable::new());

    assert!(standard_operation_table_unchanged());
    assert_eq!(
        descriptor.table_kind(),
        GenericOperationTableKind::StandardTetromino
    );
    assert_eq!(descriptor.operation_count(), 28);
    assert_eq!(descriptor.rotation_state_count(), 4);
    assert_eq!(descriptor.candidate_runtime_guard_reason(), None);
}

#[test]
fn custom_operation_table_schema_validates() {
    let schema = CustomOperationTableSchema::from_definition(&custom_piece()).expect("schema");
    let table = CustomPieceOperationTable::new(schema.clone());

    assert_eq!(schema.piece_area(), 3);
    assert_eq!(table.schema().operations().len(), 1);
}

#[test]
fn generic_operation_descriptor_can_be_built() {
    let schema = CustomOperationTableSchema::from_definition(&custom_piece()).expect("schema");
    let table = CustomPieceOperationTable::new(schema);
    let descriptor = GenericOperationTableDescriptor::from_custom_table(&table);

    assert_eq!(
        descriptor.table_kind(),
        GenericOperationTableKind::CustomPiece
    );
    assert_eq!(descriptor.piece_area(), 3);
    assert_eq!(descriptor.operation_count(), 1);
    assert_eq!(
        descriptor.candidate_runtime_guard_reason(),
        Some(CUSTOM_CANDIDATE_RUNTIME_UNSUPPORTED)
    );
    assert_eq!(
        descriptor.reachability_runtime_guard_reason(),
        Some(CUSTOM_REACHABILITY_RUNTIME_UNSUPPORTED)
    );
}

fn custom_piece() -> CustomPieceDefinition {
    CustomPieceDefinition::new(
        PieceDefinitionId::new("custom:tri-v1"),
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
