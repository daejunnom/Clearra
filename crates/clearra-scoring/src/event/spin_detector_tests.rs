use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::{
    layout::board64_layout::Board64Layout, placement::placement_mask::PlacementMask,
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_replay::{
    board::board64_state::Board64State,
    trace::{BoardAfterStep, HoldDecision, LineClearEvent, PieceDecision},
};

use super::*;

#[test]
fn corner_based_t_spin_requires_last_action_evidence() {
    let step = t_step_with_blocked_corners(&[(1, 0), (3, 0), (1, 2)]);

    let spin = SpinDetector::detect(step, SpinRuleId::TSpinCornerBased, 1);

    assert_eq!(spin, None);
}

#[test]
fn corner_based_t_spin_does_not_flag_open_t_line_clear() {
    let step = t_step_with_blocked_corners(&[(1, 0), (3, 0)]);

    let spin = SpinDetector::detect(step, SpinRuleId::TSpinCornerBased, 1);

    assert_eq!(spin, None);
    assert!(SpinDetector::detect(step, SpinRuleId::TSpinSimple, 1).is_some());
}

#[test]
fn classifier_does_not_infer_rotation_from_final_orientation() {
    let step = t_step_with_blocked_corners(&[(1, 0), (3, 0), (1, 2)]);

    let spin = SpinDetector::detect_with_classifier(
        step,
        &TSpinCornerRule,
        &ScoreProfile::new("classifier", "Classifier"),
        1,
    );

    assert_eq!(spin, None);
}

fn t_step_with_blocked_corners(blockers: &[(u16, u16)]) -> PlacementStep {
    let layout = Board64Layout::new(
        clearra_core_domain::board::board_size::BoardSize::new(6, 4).expect("board size"),
    )
    .expect("layout");
    let registry = standard_tetromino_registry();
    let t = registry.get(PieceKind::T).expect("T piece");
    let placement = PlacementMask::new(layout, t, RotationState::Zero, 1, 1).expect("T mask");
    let before = Board64State::new(layout, blocker_mask(layout, blockers)).expect("board");
    let after_placement =
        Board64State::new(layout, before.occupied() | placement.mask()).expect("place T");

    PlacementStep::new(
        0,
        PieceDecision::new(PieceKind::T, 0, 1, None, None, HoldDecision::None),
        placement,
        before,
        BoardAfterStep::new(after_placement, after_placement),
        LineClearEvent::new(1),
    )
}

fn blocker_mask(layout: Board64Layout, blockers: &[(u16, u16)]) -> u64 {
    blockers.iter().fold(0_u64, |mask, (x, y)| {
        mask | (1_u64 << (u64::from(*y) * u64::from(layout.width()) + u64::from(*x)))
    })
}
