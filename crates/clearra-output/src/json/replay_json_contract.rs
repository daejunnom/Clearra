use clearra_replay::{
    CellOwner, ReplayBoardSnapshotPhase, ReplayEvent, ReplayTrace, RotationRequest,
    TraceCompleteness,
};

use crate::json::json_value::JsonValue;

mod board_snapshot_phase_name {
    use super::*;

    pub(super) fn board_snapshot_phase_name(phase: ReplayBoardSnapshotPhase) -> &'static str {
        match phase {
            ReplayBoardSnapshotPhase::Initial => "initial",
            ReplayBoardSnapshotPhase::BeforePlacement => "before-placement",
            ReplayBoardSnapshotPhase::AfterPlacement => "after-placement",
            ReplayBoardSnapshotPhase::AfterLineClear => "after-line-clear",
        }
    }
}
mod cell_owner_value {
    use super::*;

    pub(super) fn cell_owner_value(owner: CellOwner) -> JsonValue {
        match owner {
            CellOwner::InitialGray => {
                JsonValue::object([("kind", JsonValue::string("initial-gray"))])
            }
            CellOwner::Piece(operation_id) => JsonValue::object([
                ("kind", JsonValue::string("piece")),
                (
                    "operation_id",
                    JsonValue::number(operation_id.0.to_string()),
                ),
            ]),
        }
    }
}
mod colored_cell_ownership {
    use super::*;

    pub(super) fn colored_cell_ownership(trace: &ReplayTrace) -> JsonValue {
        let ownership = trace.colored_cell_ownership();
        let width = usize::from(ownership.layout().width());
        JsonValue::object([
            (
                "owned_cell_count",
                JsonValue::number(ownership.owned_cell_count().to_string()),
            ),
            (
                "cells",
                JsonValue::array(
                    ownership
                        .owners()
                        .iter()
                        .enumerate()
                        .filter_map(|(index, owner)| owner.map(|owner| (index, owner)))
                        .map(|(index, owner)| {
                            JsonValue::object([
                                ("index", JsonValue::number(index.to_string())),
                                ("x", JsonValue::number((index % width).to_string())),
                                ("y", JsonValue::number((index / width).to_string())),
                                (
                                    "step_index",
                                    JsonValue::number(owner.step_index().to_string()),
                                ),
                                (
                                    "piece",
                                    JsonValue::string(owner.piece().as_ascii().to_string()),
                                ),
                            ])
                        }),
                ),
            ),
        ])
    }
}
mod mask_value {
    use super::*;

    pub(super) fn mask_value(mask: u64) -> JsonValue {
        JsonValue::string(format!("0x{mask:016x}"))
    }
}
mod replay_events {
    use super::*;

    pub(super) fn replay_events(trace: &ReplayTrace) -> JsonValue {
        JsonValue::array(trace.events().iter().map(|event| {
            match event {
                ReplayEvent::TraceMarker(marker) => JsonValue::object([
                    ("type", JsonValue::string("trace-marker")),
                    ("representative", JsonValue::Bool(marker.representative())),
                    ("sample", JsonValue::Bool(marker.sample())),
                ]),
                ReplayEvent::Placement(event) => JsonValue::object([
                    ("type", JsonValue::string("placement")),
                    (
                        "step_index",
                        JsonValue::number(event.step_index().to_string()),
                    ),
                    (
                        "piece",
                        JsonValue::string(event.piece().as_ascii().to_string()),
                    ),
                    (
                        "rotation",
                        JsonValue::number(event.rotation().quarter_turns().to_string()),
                    ),
                    ("x", JsonValue::number(event.x().to_string())),
                    ("y", JsonValue::number(event.y().to_string())),
                    ("placed_mask", mask_value(event.placed_mask())),
                ]),
                ReplayEvent::Lock(event) => JsonValue::object([
                    ("type", JsonValue::string("lock")),
                    (
                        "event_id",
                        JsonValue::number(event.event_id().0.to_string()),
                    ),
                    (
                        "operation_id",
                        JsonValue::number(event.operation_id().0.to_string()),
                    ),
                    (
                        "piece",
                        JsonValue::string(event.piece().as_ascii().to_string()),
                    ),
                    (
                        "rotation",
                        JsonValue::number(event.rotation().quarter_turns().to_string()),
                    ),
                    ("lock_x", JsonValue::number(event.lock_x().to_string())),
                    ("lock_y", JsonValue::number(event.lock_y().to_string())),
                    ("board_before", mask_value(event.board_before().mask)),
                    (
                        "board_after_place",
                        mask_value(event.board_after_place().mask),
                    ),
                    ("cleared_lines", mask_value(event.cleared_lines().0)),
                    (
                        "cleared_cell_owners",
                        JsonValue::array(
                            event
                                .cleared_cell_owners()
                                .iter()
                                .map(|owner| cell_owner_value(*owner)),
                        ),
                    ),
                    (
                        "board_after_clear",
                        mask_value(event.board_after_clear().mask),
                    ),
                ]),
                ReplayEvent::HoldStore(event) => JsonValue::object([
                    ("type", JsonValue::string("hold-store")),
                    (
                        "step_index",
                        JsonValue::number(event.step_index().to_string()),
                    ),
                    (
                        "stored_piece",
                        JsonValue::string(event.stored_piece().as_ascii().to_string()),
                    ),
                ]),
                ReplayEvent::HoldSwap(event) => JsonValue::object([
                    ("type", JsonValue::string("hold-swap")),
                    (
                        "step_index",
                        JsonValue::number(event.step_index().to_string()),
                    ),
                    (
                        "held_piece",
                        JsonValue::string(event.held_piece().as_ascii().to_string()),
                    ),
                    (
                        "active_piece",
                        JsonValue::string(event.active_piece().as_ascii().to_string()),
                    ),
                ]),
                ReplayEvent::HoldRelease(event) => JsonValue::object([
                    ("type", JsonValue::string("terminal-hold-release")),
                    (
                        "step_index",
                        JsonValue::number(event.step_index().to_string()),
                    ),
                    (
                        "active_piece",
                        JsonValue::string(event.active_piece().as_ascii().to_string()),
                    ),
                ]),
                ReplayEvent::Drop(event) => JsonValue::object([
                    ("type", JsonValue::string("drop")),
                    (
                        "step_index",
                        JsonValue::number(event.step_index().to_string()),
                    ),
                    ("from_y", JsonValue::number(event.from_y().to_string())),
                    ("to_y", JsonValue::number(event.to_y().to_string())),
                    ("distance", JsonValue::number(event.distance().to_string())),
                ]),
                ReplayEvent::SpinBasis(event) => JsonValue::object([
                    ("type", JsonValue::string("spin-basis")),
                    (
                        "step_index",
                        JsonValue::number(event.step_index().to_string()),
                    ),
                    (
                        "piece",
                        JsonValue::string(event.piece().as_ascii().to_string()),
                    ),
                    (
                        "rotation",
                        JsonValue::number(event.rotation().quarter_turns().to_string()),
                    ),
                    ("x", JsonValue::number(event.x().to_string())),
                    ("y", JsonValue::number(event.y().to_string())),
                    ("board_before", mask_value(event.board_before())),
                    (
                        "board_after_placement",
                        mask_value(event.board_after_placement()),
                    ),
                    (
                        "cleared_lines",
                        JsonValue::number(event.cleared_lines().to_string()),
                    ),
                ]),
                ReplayEvent::ScoreBasis(event) => JsonValue::object([
                    ("type", JsonValue::string("score-basis")),
                    (
                        "step_index",
                        JsonValue::number(event.step_index().to_string()),
                    ),
                    (
                        "piece",
                        JsonValue::string(event.piece().as_ascii().to_string()),
                    ),
                    (
                        "cleared_lines",
                        JsonValue::number(event.cleared_lines().to_string()),
                    ),
                    ("board_before", mask_value(event.board_before())),
                    (
                        "board_after_line_clear",
                        mask_value(event.board_after_line_clear()),
                    ),
                ]),
                ReplayEvent::BoardSnapshot(event) => JsonValue::object([
                    ("type", JsonValue::string("board-snapshot")),
                    (
                        "step_index",
                        JsonValue::number(event.step_index().to_string()),
                    ),
                    (
                        "phase",
                        JsonValue::string(board_snapshot_phase_name(event.phase())),
                    ),
                    ("occupied", mask_value(event.occupied())),
                ]),
                ReplayEvent::LineClear(event) => JsonValue::object([
                    ("type", JsonValue::string("line-clear")),
                    (
                        "step_index",
                        JsonValue::number(event.step_index().to_string()),
                    ),
                    (
                        "cleared_lines",
                        JsonValue::number(event.cleared_lines().to_string()),
                    ),
                ]),
                ReplayEvent::KickEvidence(event) => {
                    let predecessor = event.predecessor();
                    let result = event.result();
                    JsonValue::object([
                        ("type", JsonValue::string("kick-evidence")),
                        (
                            "step_index",
                            JsonValue::number(event.step_index().to_string()),
                        ),
                        (
                            "from_rotation",
                            JsonValue::number(event.from_rotation().to_string()),
                        ),
                        (
                            "to_rotation",
                            JsonValue::number(event.to_rotation().to_string()),
                        ),
                        (
                            "rotation_request",
                            JsonValue::string(rotation_request_name(event.rotation_request())),
                        ),
                        (
                            "kick_index",
                            JsonValue::number(event.kick_index().to_string()),
                        ),
                        ("kick_dx", JsonValue::number(event.kick_dx().to_string())),
                        ("kick_dy", JsonValue::number(event.kick_dy().to_string())),
                        (
                            "kick_table_id",
                            JsonValue::number(event.kick_table_id().to_string()),
                        ),
                        (
                            "kick_profile_id",
                            JsonValue::number(event.kick_profile_id().to_string()),
                        ),
                        (
                            "first_success_confirmed",
                            JsonValue::Bool(event.first_success_confirmed()),
                        ),
                        (
                            "predecessor",
                            JsonValue::object([
                                ("x", JsonValue::number(predecessor.0.to_string())),
                                ("y", JsonValue::number(predecessor.1.to_string())),
                            ]),
                        ),
                        (
                            "result",
                            JsonValue::object([
                                ("x", JsonValue::number(result.0.to_string())),
                                ("y", JsonValue::number(result.1.to_string())),
                            ]),
                        ),
                    ])
                }
                ReplayEvent::MovementEvidence(event) => JsonValue::object([
                    ("type", JsonValue::string("movement-evidence")),
                    (
                        "step_index",
                        JsonValue::number(event.step_index().to_string()),
                    ),
                    ("path_complete", JsonValue::Bool(event.path_complete())),
                    (
                        "last_action_was_rotation",
                        JsonValue::Bool(event.last_action_was_rotation()),
                    ),
                    ("used_kick", JsonValue::Bool(event.used_kick())),
                    ("used_180", JsonValue::Bool(event.used_180())),
                    (
                        "rotation_evidence_complete",
                        JsonValue::Bool(event.rotation_evidence_complete()),
                    ),
                ]),
                ReplayEvent::TraceCompleteness(event) => JsonValue::object([
                    ("type", JsonValue::string("trace-completeness")),
                    (
                        "completeness",
                        JsonValue::string(trace_completeness_name(event.completeness())),
                    ),
                ]),
            }
        }))
    }
}
mod replay_steps {
    use super::*;

    pub(super) fn replay_steps(trace: &ReplayTrace) -> JsonValue {
        JsonValue::array(trace.solution_trace().steps().iter().map(|step| {
            let placement = step.placement();
            let board_after = step.board_after();
            JsonValue::object([
                (
                    "step_index",
                    JsonValue::number(step.step_index().to_string()),
                ),
                (
                    "piece",
                    JsonValue::string(placement.piece_kind().as_ascii().to_string()),
                ),
                (
                    "rotation",
                    JsonValue::number(placement.rotation().quarter_turns().to_string()),
                ),
                ("x", JsonValue::number(placement.x().to_string())),
                ("y", JsonValue::number(placement.y().to_string())),
                ("placed_mask", mask_value(placement.mask())),
                ("board_before", mask_value(step.board_before().occupied())),
                (
                    "board_after_placement",
                    mask_value(board_after.after_placement().occupied()),
                ),
                (
                    "board_after_line_clear",
                    mask_value(board_after.after_line_clear().occupied()),
                ),
                (
                    "cleared_lines",
                    JsonValue::number(step.line_clear().cleared_lines().to_string()),
                ),
            ])
        }))
    }
}
mod replay_trace_object {
    use super::*;

    pub(crate) fn replay_trace_object(trace: &ReplayTrace) -> JsonValue {
        JsonValue::object([
            ("variant_id", JsonValue::string(trace.variant_id())),
            ("representative", JsonValue::Bool(trace.representative())),
            ("sample", JsonValue::Bool(trace.sample())),
            (
                "trace_steps",
                JsonValue::number(trace.trace_steps().to_string()),
            ),
            ("canonical_key", JsonValue::string(trace.canonical_key())),
            ("steps", replay_steps(trace)),
            ("events", replay_events(trace)),
            ("colored_cell_ownership", colored_cell_ownership(trace)),
        ])
    }
}
mod rotation_request_name {
    use super::*;

    pub(super) fn rotation_request_name(request: RotationRequest) -> &'static str {
        match request {
            RotationRequest::None => "none",
            RotationRequest::Clockwise => "clockwise",
            RotationRequest::CounterClockwise => "counter-clockwise",
            RotationRequest::HalfTurn => "half-turn",
        }
    }
}
mod trace_completeness_name {
    use super::*;

    pub(super) fn trace_completeness_name(completeness: TraceCompleteness) -> &'static str {
        match completeness {
            TraceCompleteness::Complete => "complete",
            TraceCompleteness::MissingKickEvidence => "missing-kick-evidence",
            TraceCompleteness::SampleOnly => "sample-only",
            TraceCompleteness::Incomplete => "incomplete",
        }
    }
}

use board_snapshot_phase_name::board_snapshot_phase_name;
use cell_owner_value::cell_owner_value;
use colored_cell_ownership::colored_cell_ownership;
use mask_value::mask_value;
use replay_events::replay_events;
use replay_steps::replay_steps;
pub(crate) use replay_trace_object::replay_trace_object;
use rotation_request_name::rotation_request_name;
use trace_completeness_name::trace_completeness_name;
