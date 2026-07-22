use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::{
    layout::board64_layout::Board64Layout, placement::placement_mask::PlacementMask,
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_replay::{
    board::board64_state::Board64State,
    trace::{
        BoardAfterStep, HoldDecision, LineClearEvent, PieceDecision, PlacementStep, SolutionTrace,
    },
};

use crate::{
    builtin::{jstris_ultra, tetrio_score},
    event::{clear_event::ClearEvent, score_event::ScoreEvent},
    profile::{DropScorePolicy, ScoreModelId, ScoreProfile},
    state::ComboState,
};

use super::*;

#[test]
fn score_model_evaluator_scores_solution_trace_as_post_processing() {
    let trace = sample_perfect_clear_trace();

    let evaluation = ScoreModelEvaluator::evaluate_trace(&tetrio_score(), &trace);

    assert_eq!(evaluation.profile_id(), "tetrio");
    assert_eq!(evaluation.event_count(), trace.len());
    assert!(evaluation.final_state().score() > 0);
    assert!(evaluation.final_state().attack() > 0);
}

#[test]
fn score_model_evaluator_scores_replay_trace_as_post_processing() {
    let solution_trace = sample_perfect_clear_trace();
    let replay = clearra_replay::ReplayTrace::new(
        "variant",
        solution_trace.clone(),
        Vec::new(),
        clearra_replay::ColoredCellOwnership::from_trace(&solution_trace).expect("ownership"),
        true,
        true,
    );

    let evaluation = ScoreModelEvaluator::evaluate_replay_trace(&tetrio_score(), &replay);

    assert_eq!(evaluation.profile_id(), "tetrio");
    assert_eq!(evaluation.event_count(), replay.solution_trace().len());
    assert!(evaluation.final_state().score() > 0);
}

#[test]
fn score_profile_evaluates_replay() {
    let solution_trace = sample_perfect_clear_trace();
    let replay = clearra_replay::ReplayTrace::new(
        "profile-replay",
        solution_trace.clone(),
        Vec::new(),
        clearra_replay::ColoredCellOwnership::from_trace(&solution_trace).expect("ownership"),
        true,
        true,
    );

    let evaluation = ScoreModelEvaluator::evaluate_replay_trace(&tetrio_score(), &replay);

    assert_eq!(evaluation.profile_id(), "tetrio");
    assert!(evaluation.final_state().score() > 0);
}

#[test]
fn score_model_evaluator_uses_profile_specific_score_tables() {
    let event = ScoreEvent::new(
        0,
        ClearEvent::new(4, true),
        None,
        ComboState::default(),
        ComboState::default(),
        false,
        false,
    );
    let guideline =
        ScoreProfile::new("guideline", "Guideline").with_score_model(ScoreModelId::Guideline);

    let guideline_score =
        ScoreModelEvaluator::evaluate_event(&guideline, ScoreState::default(), event).score();
    let jstris_score =
        ScoreModelEvaluator::evaluate_event(&jstris_ultra(), ScoreState::default(), event).score();
    let tetrio_score =
        ScoreModelEvaluator::evaluate_event(&tetrio_score(), ScoreState::default(), event).score();

    assert_ne!(guideline_score, jstris_score);
    assert_ne!(jstris_score, tetrio_score);
}

#[test]
fn score_model_evaluator_adds_hard_drop_2_soft_drop_1_from_replay_drop_events() {
    let solution_trace = sample_perfect_clear_trace();
    let replay = clearra_replay::ReplayTrace::new(
        "drop-variant",
        solution_trace.clone(),
        vec![clearra_replay::ReplayEvent::Drop(
            clearra_replay::replay::ReplayDropEvent::new(0, 5, 2),
        )],
        clearra_replay::ColoredCellOwnership::from_trace(&solution_trace).expect("ownership"),
        true,
        true,
    );
    let profile = ScoreProfile::new("drop-only", "Drop Only")
        .with_drop_score_policy(DropScorePolicy::HardDrop2SoftDrop1);

    let evaluation = ScoreModelEvaluator::evaluate_replay_trace(&profile, &replay);

    assert_eq!(evaluation.final_state().score(), 6);
}

fn sample_perfect_clear_trace() -> SolutionTrace {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let registry = standard_tetromino_registry();
    let piece = registry.get(PieceKind::O).expect("O piece");
    let placement =
        PlacementMask::new(layout, piece, RotationState::Zero, 0, 0).expect("placement");
    let before = Board64State::empty(layout);
    let after_placement = Board64State::new(layout, placement.mask()).expect("after placement");
    let after_clear = Board64State::empty(layout);
    let step = PlacementStep::new(
        0,
        PieceDecision::new(PieceKind::O, 0, 1, None, None, HoldDecision::None),
        placement,
        before,
        BoardAfterStep::new(after_placement, after_clear),
        LineClearEvent::new(2),
    );
    SolutionTrace::new(vec![step])
}
