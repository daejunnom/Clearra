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

use super::*;

#[test]
fn area_multiset_feasibility_uses_piece_area_multiset_marker() {
    assert!(area_multiset_feasibility_uses_piece_area_multiset());
}

#[test]
fn generic_feasibility_does_not_use_missing_cells_mod_four() {
    let piece_set = MixedPieceSet::new(
        "mixed",
        "Mixed",
        vec![
            MixedPieceSetEntry::Standard(PieceKind::I),
            MixedPieceSetEntry::Custom(custom_piece()),
        ],
    )
    .expect("piece set");

    let feasibility = AreaMultisetFeasibility::from_mixed_piece_set(&piece_set).expect("areas");

    assert_eq!(feasibility.active_piece_area_multiset(), &[4, 3]);
    assert!(feasibility.can_fill_exactly(3));
    assert!(feasibility.can_fill_exactly(7));
    assert!(!feasibility.can_fill_exactly(5));
}

#[test]
fn bag_area_multiset_respects_piece_multiplicity() {
    let piece_set = MixedPieceSet::new(
        "mixed",
        "Mixed",
        vec![
            MixedPieceSetEntry::Standard(PieceKind::I),
            MixedPieceSetEntry::Custom(custom_piece()),
        ],
    )
    .expect("piece set");
    let bag = MixedBagProfile::new(
        "mixed-bag",
        &piece_set,
        vec![
            MixedBagEntry::new(PieceDefinitionId::new("std:I"), 1, 1),
            MixedBagEntry::new(PieceDefinitionId::new("custom:tri-v1"), 2, 1),
        ],
        BagBoundaryModels::all_mvp3_models(),
    )
    .expect("bag");

    let feasibility =
        AreaMultisetFeasibility::from_mixed_bag_profile(&piece_set, &bag).expect("areas");

    assert_eq!(feasibility.active_piece_area_multiset(), &[4, 3, 3]);
    assert!(feasibility.can_fill_exactly(10));
    assert!(!feasibility.can_fill_exactly(11));
}

#[test]
fn area_decomposition_is_necessary_condition_not_solver() {
    let feasibility = AreaMultisetFeasibility::new([4, 3]).expect("areas");

    let decision = feasibility.check_component_area(7);

    assert_eq!(decision, AreaFeasibilityDecision::SearchMayContinue);
    assert!(!decision.is_solution_found());
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
