use clearra_core_domain::{
    ids::piece_id::PieceDefinitionId,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_piece_registry::{
    custom::{
        CustomPieceDefinition, CustomPieceRotation, PieceDisplayMetadata, PieceSpawnBounds,
        PieceSymmetryClass,
    },
    registry::piece_registry::ShapeCell,
    registry::{
        BagBoundaryModels, MixedBagEntry, MixedBagProfile, MixedPieceSet, MixedPieceSetEntry,
    },
};
use clearra_profiles::pieces::standard_tetrominoes::standard_tetromino_piece_set_profile;
use clearra_setup_search::query::PieceBudget;

use crate::diagnostic::diagnostic_code::DiagnosticCode;

use super::{
    validate_mixed_bag_profile_mvp3_guard, validate_mixed_piece_set_mvp3_guard,
    validate_piece_budget, validate_piece_set, validate_piece_set_profile,
};

#[test]
fn standard_piece_profile_is_supported() {
    let report = validate_piece_set_profile(standard_tetromino_piece_set_profile());

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IPieceSetMvpSupported));
}

#[test]
fn duplicate_piece_set_is_rejected() {
    let report = validate_piece_set(&[PieceKind::I, PieceKind::I]);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::EPieceSetUnsupportedMvp));
}

#[test]
fn oversized_budget_is_rejected() {
    let budget = PieceBudget::standard_7_bag(8);
    let report = validate_piece_budget(&budget);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::EPieceSetUnsupportedMvp));
}

#[test]
fn custom_piece_registry_is_recognized_but_blocked_before_search_runtime() {
    let piece_set = MixedPieceSet::standard_plus_custom(
        "mixed-standard-tri",
        "Standard plus triomino",
        vec![custom_piece()],
    )
    .expect("mixed piece set");
    let report = validate_mixed_piece_set_mvp3_guard(&piece_set);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ECustomPieceUnsupportedMvp));
    let evidence_values = report
        .diagnostics()
        .iter()
        .flat_map(|diagnostic| diagnostic.evidence())
        .map(|evidence| evidence.value())
        .collect::<Vec<_>>();
    assert!(evidence_values.contains(&"custom_candidate_runtime_unsupported"));
    assert!(evidence_values.contains(&"custom_reachability_runtime_unsupported"));
}

#[test]
fn standard_only_mixed_piece_registry_is_supported_by_guard() {
    let piece_set = MixedPieceSet::new(
        "standard-only",
        "Standard only",
        PieceKind::STANDARD_TETROMINOES
            .iter()
            .copied()
            .map(MixedPieceSetEntry::Standard)
            .collect(),
    )
    .expect("standard only piece set");
    let report = validate_mixed_piece_set_mvp3_guard(&piece_set);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IPieceSetMvpSupported));
}

#[test]
fn custom_bag_profile_is_guarded_before_piece_definition_id_supply_runtime() {
    let piece_set = MixedPieceSet::standard_plus_custom(
        "mixed-standard-tri",
        "Standard plus triomino",
        vec![custom_piece()],
    )
    .expect("mixed piece set");
    let bag = MixedBagProfile::new(
        "mixed-bag",
        &piece_set,
        vec![
            MixedBagEntry::new(PieceDefinitionId::new("std:I"), 1, 1),
            MixedBagEntry::new(PieceDefinitionId::new("custom:tri-v1"), 2, 1),
        ],
        BagBoundaryModels::all_mvp3_models(),
    )
    .expect("bag profile");

    let report = validate_mixed_bag_profile_mvp3_guard(&piece_set, &bag);

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ECustomBagUnsupportedMvp));
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
        PieceDisplayMetadata::new(Some("#ffcc00".to_owned()), Some("R".to_owned())),
        PieceSymmetryClass::MirrorX,
        "cells:0,0;1,0;0,1",
    )
    .expect("custom piece")
}
