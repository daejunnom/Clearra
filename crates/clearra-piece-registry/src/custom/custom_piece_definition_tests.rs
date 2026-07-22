use clearra_core_domain::piece::rotation::RotationState;

use super::*;

#[test]
fn custom_piece_definition_carries_mvp3_schema_without_runtime_search_support() {
    let definition = triomino_definition();

    assert_eq!(definition.id().as_str(), "custom:tri-v1");
    assert_eq!(definition.piece_definition_id().as_str(), "custom:tri-v1");
    assert_eq!(definition.label(), "Triomino");
    assert_eq!(definition.display_name(), "Triomino");
    assert_eq!(definition.area(), 3);
    assert_eq!(definition.cell_count(), 3);
    assert_eq!(
        definition.rotation_states(),
        vec![RotationState::Zero, RotationState::Right]
    );
    assert_eq!(definition.cells_by_rotation().len(), 2);
    assert_eq!(definition.bounds_by_rotation()[0].max_x(), 1);
    assert_eq!(definition.spawn_bounds().max_x(), 2);
    assert!(definition.spawn_offsets().is_empty());
    assert_eq!(definition.display().color(), Some("#ffcc00"));
    assert_eq!(definition.color_hint(), Some("#ffcc00"));
    assert_eq!(definition.symmetry().as_str(), "mirror-x");
    assert_eq!(definition.symmetry_class(), PieceSymmetryClass::MirrorX);
    assert_eq!(definition.source_provenance().source_label(), None);
    assert_eq!(definition.canonical_key(), "cells:0,0;1,0;0,1");
}

#[test]
fn custom_piece_definition_rejects_unstable_or_inconsistent_shapes() {
    let duplicate_rotation = CustomPieceDefinition::new(
        PieceDefinitionId::new("custom:bad"),
        "Bad",
        vec![
            CustomPieceRotation::new(
                RotationState::Zero,
                vec![ShapeCell::new(0, 0), ShapeCell::new(1, 0)],
            ),
            CustomPieceRotation::new(
                RotationState::Zero,
                vec![ShapeCell::new(0, 0), ShapeCell::new(0, 1)],
            ),
        ],
        PieceSpawnBounds::new(0, 1, 0, 1).expect("bounds"),
        PieceDisplayMetadata::default(),
        PieceSymmetryClass::None,
        "bad",
    );

    assert_eq!(
        duplicate_rotation,
        Err(CustomPieceDefinitionError::DuplicateRotationState {
            state: RotationState::Zero
        })
    );

    let inconsistent_area = CustomPieceDefinition::new(
        PieceDefinitionId::new("custom:bad2"),
        "Bad 2",
        vec![
            CustomPieceRotation::new(RotationState::Zero, vec![ShapeCell::new(0, 0)]),
            CustomPieceRotation::new(
                RotationState::Right,
                vec![ShapeCell::new(0, 0), ShapeCell::new(1, 0)],
            ),
        ],
        PieceSpawnBounds::new(0, 1, 0, 1).expect("bounds"),
        PieceDisplayMetadata::default(),
        PieceSymmetryClass::None,
        "bad2",
    );

    assert_eq!(
        inconsistent_area,
        Err(CustomPieceDefinitionError::InconsistentRotationArea {
            state: RotationState::Right,
            expected: 1,
            actual: 2
        })
    );
}

fn triomino_definition() -> CustomPieceDefinition {
    CustomPieceDefinition::new(
        PieceDefinitionId::new("custom:tri-v1"),
        "Triomino",
        vec![
            CustomPieceRotation::new(
                RotationState::Zero,
                vec![
                    ShapeCell::new(0, 0),
                    ShapeCell::new(1, 0),
                    ShapeCell::new(0, 1),
                ],
            ),
            CustomPieceRotation::new(
                RotationState::Right,
                vec![
                    ShapeCell::new(0, 0),
                    ShapeCell::new(0, 1),
                    ShapeCell::new(1, 1),
                ],
            ),
        ],
        PieceSpawnBounds::new(0, 2, 0, 2).expect("bounds"),
        PieceDisplayMetadata::new(Some("#ffcc00".to_owned()), Some("R".to_owned())),
        PieceSymmetryClass::MirrorX,
        "cells:0,0;1,0;0,1",
    )
    .expect("custom triomino")
}
