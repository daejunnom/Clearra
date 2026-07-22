use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_geometry::{
    layout::board64_layout::Board64Layout, placement::placement_mask::PlacementMask,
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_replay::{
    board::board64_state::Board64State,
    trace::{BoardAfterStep, HoldDecision, LineClearEvent, PieceDecision, PlacementStep},
};

use super::*;

#[test]
fn solution_trace_events_extract_line_clear_and_perfect_clear_sequence() {
    let trace = sample_perfect_clear_trace();

    let events = SolutionTraceEvents::from_trace(&trace, SpinRuleId::Disabled);

    assert_eq!(events.len(), trace.len());
    assert!(events
        .events()
        .iter()
        .any(|event| event.clear().lines() > 0));
    assert!(events
        .events()
        .last()
        .is_some_and(|event| event.clear().is_perfect_clear()));
}

#[test]
fn solution_trace_events_can_be_extracted_from_replay_trace() {
    let trace = sample_perfect_clear_trace();
    let replay = clearra_replay::ReplayTrace::new(
        "variant",
        trace.clone(),
        Vec::new(),
        clearra_replay::ColoredCellOwnership::from_trace(&trace).expect("ownership"),
        true,
        true,
    );

    let events = SolutionTraceEvents::from_replay_trace(&replay, SpinRuleId::Disabled);

    assert_eq!(events.len(), trace.len());
    assert!(events.events()[0].clear().is_perfect_clear());
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
