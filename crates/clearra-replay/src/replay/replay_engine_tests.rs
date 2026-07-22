use crate::{
    event::{KickEvidenceEvent, RotationRequest, TraceCompleteness},
    replay::{
        BuildVariantOperation, BuildVariantReplayInput, CellOwner, ReplayBoardSnapshotPhase,
        ReplayEngine, ReplayEngineError, ReplayEvent, ReplayLineClearEvent,
        ReplayTraceBufferBudget, ReplayTraceMarker, RowMask,
    },
};

use clearra_core_domain::{
    operation::operation::OperationId,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_geometry::layout::board64_layout::Board64Layout;

#[test]
fn build_variant_becomes_representative_sample_replay_trace() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let input = BuildVariantReplayInput::new(
        "variant-1",
        layout,
        0x003f,
        vec![BuildVariantOperation::new(PieceKind::I, RotationState::Zero, 6, 0).with_mask(0x03c0)],
    )
    .with_trace_marker(true, true);

    let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");

    assert_eq!(trace.variant_id(), "variant-1");
    assert_eq!(trace.trace_steps(), 1);
    assert!(trace.representative());
    assert!(trace.sample());
    assert!(trace
        .events()
        .contains(&ReplayEvent::TraceMarker(ReplayTraceMarker::new(
            true, true
        ))));
    assert!(trace
        .events()
        .contains(&ReplayEvent::LineClear(ReplayLineClearEvent::new(0, 1))));
    assert!(trace
        .events()
        .iter()
        .any(|event| matches!(event, ReplayEvent::Drop(drop) if drop.distance() > 0)));
    assert!(trace
        .events()
        .iter()
        .any(|event| matches!(event, ReplayEvent::SpinBasis(spin) if spin.step_index() == 0)));
    assert!(trace.events().iter().any(|event| {
        matches!(event, ReplayEvent::ScoreBasis(score)
            if score.step_index() == 0 && score.cleared_lines() == 1)
    }));
    assert!(trace.events().iter().any(|event| {
        matches!(event, ReplayEvent::BoardSnapshot(snapshot)
            if snapshot.phase() == ReplayBoardSnapshotPhase::BeforePlacement
                && snapshot.occupied() == 0x003f)
    }));
    assert!(trace.events().iter().any(|event| {
        matches!(event, ReplayEvent::BoardSnapshot(snapshot)
            if snapshot.phase() == ReplayBoardSnapshotPhase::AfterLineClear)
    }));
    assert!(trace.canonical_key().starts_with("trk1:"));
}

#[test]
fn replay_trace_preserves_line_clear_events() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let input = BuildVariantReplayInput::new(
        "variant-line-clear",
        layout,
        0x003f,
        vec![BuildVariantOperation::new(PieceKind::I, RotationState::Zero, 6, 0).with_mask(0x03c0)],
    );

    let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");

    assert!(trace
        .events()
        .iter()
        .any(|event| matches!(event, ReplayEvent::LineClear(line) if line.cleared_lines() == 1)));
    assert!(trace.events().iter().any(|event| {
        matches!(event, ReplayEvent::BoardSnapshot(snapshot)
            if snapshot.phase() == ReplayBoardSnapshotPhase::AfterLineClear
                && snapshot.occupied() == 0)
    }));
}

#[test]
fn replay_preserves_cleared_cell_owners() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let input = BuildVariantReplayInput::new(
        "variant-owner-clear",
        layout,
        0x003f,
        vec![BuildVariantOperation::new(PieceKind::I, RotationState::Zero, 6, 0).with_mask(0x03c0)],
    );

    let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");
    let lock = trace
        .events()
        .iter()
        .find_map(|event| match event {
            ReplayEvent::Lock(lock) => Some(lock),
            _ => None,
        })
        .expect("lock event");

    assert_eq!(lock.cleared_lines(), RowMask(0x03ff));
    assert_eq!(lock.board_before().mask, 0x003f);
    assert_eq!(lock.board_after_place().mask, 0x03ff);
    assert_eq!(lock.board_after_clear().mask, 0);
    assert_eq!(lock.cleared_cell_owners().len(), 10);
    assert!(lock
        .cleared_cell_owners()
        .iter()
        .take(6)
        .all(|owner| *owner == CellOwner::InitialGray));
    assert!(lock
        .cleared_cell_owners()
        .iter()
        .skip(6)
        .all(|owner| *owner == CellOwner::Piece(OperationId(0))));
}

#[test]
fn replay_records_actual_lock_coordinate() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let input = BuildVariantReplayInput::new(
        "variant-lock-coordinate",
        layout,
        0x0000,
        vec![BuildVariantOperation::new(PieceKind::I, RotationState::Zero, 6, 0).with_mask(0x03c0)],
    );

    let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");
    let lock = trace
        .events()
        .iter()
        .find_map(|event| match event {
            ReplayEvent::Lock(lock) => Some(lock),
            _ => None,
        })
        .expect("lock event");

    assert_eq!(lock.lock_x(), 6);
    assert_eq!(lock.lock_y(), 0);
}

#[test]
fn replay_ownership_timeline_preserves_cleared_cell_owner() {
    replay_preserves_cleared_cell_owners();
}

#[test]
fn rust_replay_event_preserves_kick_evidence() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let kick = KickEvidenceEvent::new(0, 0, 1, RotationRequest::Clockwise, 2, 1, -1)
        .with_profile_ids(11, 22)
        .with_anchors((3, 4), (5, 6));
    let input = BuildVariantReplayInput::new(
        "variant-kick",
        layout,
        0x003f,
        vec![BuildVariantOperation::new(PieceKind::I, RotationState::Zero, 6, 0).with_mask(0x03c0)],
    )
    .with_kick_evidence(vec![kick.clone()])
    .with_trace_completeness(TraceCompleteness::MissingKickEvidence);

    let trace = ReplayEngine::build_variant_to_trace(&input).expect("trace");

    assert!(trace.events().iter().any(|event| {
        matches!(event, ReplayEvent::KickEvidence(evidence)
            if evidence.kick_index() == 2
                && evidence.kick_dx() == 1
                && evidence.kick_dy() == -1
                && evidence.kick_table_id() == 11)
    }));
    assert!(trace.events().iter().any(|event| {
        matches!(event, ReplayEvent::TraceCompleteness(completeness)
            if completeness.completeness() == TraceCompleteness::MissingKickEvidence)
    }));
}

#[test]
fn replay_trace_buffer_respects_memory_budget() {
    let layout = Board64Layout::standard_10_by_lines(2).expect("layout");
    let input = BuildVariantReplayInput::new(
        "variant-budget",
        layout,
        0x003f,
        vec![BuildVariantOperation::new(PieceKind::I, RotationState::Zero, 6, 0).with_mask(0x03c0)],
    );

    let error =
        ReplayEngine::build_variant_to_trace_with_budget(&input, ReplayTraceBufferBudget::new(1))
            .expect_err("budget error");

    assert_eq!(
        error,
        ReplayEngineError::ReplayTraceBufferBudgetExceeded {
            event_count: 11,
            max_events: 1
        }
    );
}
