use std::{collections::BTreeSet, fs, path::PathBuf};

use clearra_core_domain::{
    ids::piece_id::PieceDefinitionId,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_piece_registry::{
    custom::{
        CustomOperationTableSchema, CustomPieceDefinition, CustomPieceRotation,
        PieceDisplayMetadata, PieceSpawnBounds, PieceSymmetryClass,
    },
    registry::piece_registry::ShapeCell,
    registry::{
        BagBoundaryModels, MixedBagEntry, MixedBagProfile, MixedPieceSet, MixedPieceSetEntry,
        PieceRegistryBridge, PieceRegistryRuntimePath,
    },
};
use clearra_validation::{
    diagnostic::diagnostic_code::DiagnosticCode,
    validators::piece_set_validator::{
        validate_mixed_bag_profile_mvp3_guard, validate_mixed_piece_set_mvp3_guard,
    },
};
use serde_json::Value;

#[test]
fn mvp3_custom_piece_fixture_defines_stable_mixed_piece_set_but_runtime_is_guarded() {
    let fixture = read_fixture("tests/fixtures/pieces/mixed_custom_piece_set.json");
    assert_eq!(fixture["schema_version"].as_u64(), Some(3));

    let piece_set_value = &fixture["piece_set"];
    let custom_value = piece_set_value["pieces"]
        .as_array()
        .expect("pieces array")
        .iter()
        .find(|piece| piece["kind"].as_str() == Some("custom"))
        .expect("custom piece");
    let custom_piece = custom_piece_from_fixture(custom_value);
    assert_eq!(custom_piece.id().as_str(), "custom:tri-v1");
    assert_eq!(
        custom_piece.area(),
        custom_value["area"].as_u64().unwrap() as usize
    );
    assert_eq!(custom_piece.spawn_bounds().max_x(), 2);
    assert_eq!(custom_piece.display().glyph(), Some("R"));
    assert_eq!(custom_piece.symmetry(), PieceSymmetryClass::MirrorX);
    assert_eq!(custom_piece.canonical_key(), "cells:0,0;1,0;0,1");
    let operation_table =
        CustomOperationTableSchema::from_definition(&custom_piece).expect("operation table");
    assert_eq!(operation_table.piece_id().as_str(), "custom:tri-v1");
    assert_eq!(operation_table.piece_area(), 3);
    assert_eq!(
        operation_table.rotation_states(),
        vec![RotationState::Zero, RotationState::Right]
    );

    let piece_set = MixedPieceSet::standard_plus_custom(
        piece_set_value["id"].as_str().expect("piece set id"),
        piece_set_value["label"].as_str().expect("piece set label"),
        vec![custom_piece],
    )
    .expect("mixed piece set");
    assert_eq!(piece_set.stable_piece_ids()[0].as_str(), "std:I");
    assert_eq!(piece_set.stable_piece_ids()[7].as_str(), "custom:tri-v1");
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
    assert_eq!(bridge.custom_operation_tables().len(), 1);
    assert_ne!(bridge.piece_definition_id_fingerprint(), 0);
    assert_ne!(bridge.piece_area_multiset_fingerprint(), 0);
    assert_ne!(bridge.piece_set_profile_id(), 0);

    let bag = &fixture["bag"];
    assert_eq!(
        bag["piece_set_id"].as_str(),
        Some(piece_set.id()),
        "bag/profile/supply fixture must reference the mixed piece set by stable id"
    );
    let stable_ids = piece_set
        .stable_piece_ids()
        .into_iter()
        .map(|id| id.as_str().to_owned())
        .collect::<BTreeSet<_>>();
    let bag_profile = mixed_bag_from_fixture(bag, &piece_set);
    assert_eq!(bag_profile.bag_size(), 4);
    assert_eq!(bag_profile.total_weight(), 3);
    assert!(bag_profile.boundary_models().fixed_sequence());
    assert!(bag_profile.boundary_models().observed_window());
    assert!(bag_profile.boundary_models().bag_aligned_pattern());

    for entry in bag["entries"].as_array().expect("bag entries") {
        let piece_id = entry["piece_id"].as_str().expect("piece id");
        assert!(
            stable_ids.contains(piece_id),
            "bag profile entries must reference stable piece ids from the mixed piece set"
        );
        assert!(entry["multiplicity"].as_u64().expect("multiplicity") > 0);
        assert!(entry["weight"].as_u64().expect("weight") > 0);
    }

    let report = validate_mixed_piece_set_mvp3_guard(&piece_set);
    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ECustomPieceUnsupportedMvp));
    assert_eq!(
        fixture["expected"]["diagnostic_code"].as_str(),
        Some(DiagnosticCode::ECustomPieceUnsupportedMvp.as_str())
    );

    let bag_report = validate_mixed_bag_profile_mvp3_guard(&piece_set, &bag_profile);
    assert!(bag_report.has_errors());
    assert!(bag_report.contains_code(DiagnosticCode::ECustomBagUnsupportedMvp));
    assert_eq!(
        fixture["expected"]["bag_diagnostic_code"].as_str(),
        Some(DiagnosticCode::ECustomBagUnsupportedMvp.as_str())
    );
}

#[test]
fn stable_piece_definition_ids_are_not_registry_order_indices() {
    let entries = vec![
        MixedPieceSetEntry::Custom(custom_piece("custom:tri-v1")),
        MixedPieceSetEntry::Standard(PieceKind::I),
    ];
    let piece_set =
        MixedPieceSet::new("custom-first", "Custom first", entries).expect("mixed piece set");

    assert_eq!(piece_set.stable_piece_ids()[0].as_str(), "custom:tri-v1");
    assert_eq!(piece_set.stable_piece_ids()[1].as_str(), "std:I");
}

#[test]
fn standard_fast_path_is_unaffected_by_custom_piece_schema_foundation() {
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
        bridge.runtime_path(),
        PieceRegistryRuntimePath::StandardFastPath
    );
    assert!(bridge.custom_operation_tables().is_empty());
    assert_eq!(bridge.unsupported_reason(), None);
}

fn custom_piece_from_fixture(value: &Value) -> CustomPieceDefinition {
    let rotations = value["rotations"]
        .as_array()
        .expect("rotations")
        .iter()
        .map(|rotation| {
            CustomPieceRotation::new(
                parse_rotation(rotation["state"].as_str().expect("rotation state")),
                rotation["cells"]
                    .as_array()
                    .expect("cells")
                    .iter()
                    .map(|cell| {
                        ShapeCell::new(
                            cell["x"].as_i64().expect("x") as i8,
                            cell["y"].as_i64().expect("y") as i8,
                        )
                    })
                    .collect(),
            )
        })
        .collect();
    let spawn = &value["spawn_bounds"];

    CustomPieceDefinition::new(
        PieceDefinitionId::new(value["id"].as_str().expect("custom id")),
        value["label"].as_str().expect("label"),
        rotations,
        PieceSpawnBounds::new(
            spawn["min_x"].as_i64().expect("min_x") as i8,
            spawn["max_x"].as_i64().expect("max_x") as i8,
            spawn["min_y"].as_i64().expect("min_y") as i8,
            spawn["max_y"].as_i64().expect("max_y") as i8,
        )
        .expect("spawn bounds"),
        PieceDisplayMetadata::new(
            value["display"]["color"].as_str().map(ToOwned::to_owned),
            value["display"]["glyph"].as_str().map(ToOwned::to_owned),
        ),
        parse_symmetry(value["symmetry"].as_str().expect("symmetry")),
        value["canonical_key"].as_str().expect("canonical key"),
    )
    .expect("custom piece")
}

fn mixed_bag_from_fixture(value: &Value, piece_set: &MixedPieceSet) -> MixedBagProfile {
    let boundary_models = &value["boundary_models"];
    MixedBagProfile::new(
        value["profile_id"].as_str().expect("profile id"),
        piece_set,
        value["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .map(|entry| {
                MixedBagEntry::new(
                    PieceDefinitionId::new(entry["piece_id"].as_str().expect("piece id")),
                    entry["multiplicity"].as_u64().expect("multiplicity") as usize,
                    entry["weight"].as_u64().expect("weight") as u32,
                )
            })
            .collect(),
        BagBoundaryModels::new(
            boundary_models["fixed_sequence"]
                .as_bool()
                .expect("fixed_sequence"),
            boundary_models["observed_window"]
                .as_bool()
                .expect("observed_window"),
            boundary_models["bag_aligned_pattern"]
                .as_bool()
                .expect("bag_aligned_pattern"),
        ),
    )
    .expect("mixed bag")
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

fn parse_rotation(value: &str) -> RotationState {
    match value {
        "zero" => RotationState::Zero,
        "right" => RotationState::Right,
        "two" => RotationState::Two,
        "left" => RotationState::Left,
        other => panic!("unknown rotation state {other}"),
    }
}

fn parse_symmetry(value: &str) -> PieceSymmetryClass {
    match value {
        "none" => PieceSymmetryClass::None,
        "mirror-x" => PieceSymmetryClass::MirrorX,
        "mirror-y" => PieceSymmetryClass::MirrorY,
        "rotate-180" => PieceSymmetryClass::Rotate180,
        "full" => PieceSymmetryClass::Full,
        other => panic!("unknown symmetry {other}"),
    }
}

fn read_fixture(relative_path: &str) -> Value {
    let contents = fs::read_to_string(workspace_root().join(relative_path)).expect("fixture");
    serde_json::from_str(&contents).expect("fixture json")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}
