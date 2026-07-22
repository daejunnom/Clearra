use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_replay::{
    KickEvidenceEvent, ReplayEvent, ReplayTrace, RotationRequest as ReplayRotationRequest,
    TraceCompleteness as ReplayTraceCompleteness,
};
use clearra_scoring::spin::{
    BoardAnchor, KickEvidence, MovementInfo, RotationRequest as ScoringRotationRequest,
    SpinClassificationInput, TraceCompleteness as ScoringTraceCompleteness,
};

pub(crate) fn spin_input_from_replay(trace: &ReplayTrace) -> Option<SpinClassificationInput> {
    let spin = trace.events().iter().rev().find_map(|event| {
        let ReplayEvent::SpinBasis(spin) = event else {
            return None;
        };
        Some(*spin)
    })?;
    let kick_evidence = trace.events().iter().rev().find_map(|event| match event {
        ReplayEvent::KickEvidence(evidence) if evidence.step_index() == spin.step_index() => {
            Some(scoring_kick_evidence(evidence))
        }
        _ => None,
    });
    let movement_evidence = trace.events().iter().rev().find_map(|event| match event {
        ReplayEvent::MovementEvidence(evidence) if evidence.step_index() == spin.step_index() => {
            Some(*evidence)
        }
        _ => None,
    });
    let trace_completeness = trace
        .events()
        .iter()
        .rev()
        .find_map(|event| match event {
            ReplayEvent::TraceCompleteness(completeness) => {
                Some(scoring_trace_completeness(completeness.completeness()))
            }
            _ => None,
        })
        .unwrap_or(ScoringTraceCompleteness::Full);

    Some(SpinClassificationInput {
        piece: spin.piece().as_ascii(),
        rotation: spin.rotation().quarter_turns(),
        x: spin.x() as i16,
        y: spin.y() as i16,
        board_before: spin.board_before(),
        board_after_placement: spin.board_after_placement(),
        board_after_clear: spin.board_after_placement(),
        cleared_lines: spin.cleared_lines(),
        blocked_corners: blocked_t_corners_from_spin_basis(spin),
        front_corners: blocked_t_front_corners_from_spin_basis(spin),
        kick_evidence,
        movement_info: MovementInfo {
            immobile: false,
            rotation_used: movement_evidence
                .is_some_and(|evidence| evidence.last_action_was_rotation()),
            evidence_complete: movement_evidence.is_some_and(|evidence| {
                evidence.path_complete() && evidence.rotation_evidence_complete()
            }),
        },
        trace_completeness,
    })
}

fn scoring_rotation_request(raw: ReplayRotationRequest) -> ScoringRotationRequest {
    match raw {
        ReplayRotationRequest::Clockwise => ScoringRotationRequest::Clockwise,
        ReplayRotationRequest::CounterClockwise => ScoringRotationRequest::CounterClockwise,
        ReplayRotationRequest::HalfTurn => ScoringRotationRequest::HalfTurn,
        ReplayRotationRequest::None => ScoringRotationRequest::None,
    }
}

fn scoring_kick_evidence(event: &KickEvidenceEvent) -> KickEvidence {
    let predecessor = event.predecessor();
    let result = event.result();
    KickEvidence {
        from_rotation: event.from_rotation(),
        to_rotation: event.to_rotation(),
        rotation_request: scoring_rotation_request(event.rotation_request()),
        kick_index: event.kick_index(),
        kick_dx: event.kick_dx(),
        kick_dy: event.kick_dy(),
        kick_table_id: event.kick_table_id().to_string(),
        kick_profile_id: (event.kick_profile_id() != 0)
            .then(|| event.kick_profile_id().to_string()),
        first_success_confirmed: event.first_success_confirmed(),
        predecessor_anchor: BoardAnchor::new(predecessor.0, predecessor.1),
        result_anchor: BoardAnchor::new(result.0, result.1),
    }
}

fn scoring_trace_completeness(completeness: ReplayTraceCompleteness) -> ScoringTraceCompleteness {
    match completeness {
        ReplayTraceCompleteness::Complete => ScoringTraceCompleteness::Full,
        ReplayTraceCompleteness::MissingKickEvidence => {
            ScoringTraceCompleteness::MissingKickEvidence
        }
        ReplayTraceCompleteness::SampleOnly => ScoringTraceCompleteness::RetainedSample,
        ReplayTraceCompleteness::Incomplete => ScoringTraceCompleteness::Incomplete,
    }
}

fn blocked_t_corners_from_spin_basis(spin: clearra_replay::replay::ReplaySpinBasisEvent) -> u8 {
    if spin.piece() != PieceKind::T {
        return 0;
    }
    let Some((center_x, center_y)) = t_center_for_spin_basis(spin) else {
        return 0;
    };
    [(-1, -1), (1, -1), (-1, 1), (1, 1)]
        .into_iter()
        .filter(|(dx, dy)| {
            corner_is_blocked(
                spin.board_before() | spin.board_after_placement(),
                center_x + dx,
                center_y + dy,
            )
        })
        .count() as u8
}

fn blocked_t_front_corners_from_spin_basis(
    spin: clearra_replay::replay::ReplaySpinBasisEvent,
) -> u8 {
    if spin.piece() != PieceKind::T {
        return 0;
    }
    let Some((center_x, center_y)) = t_center_for_spin_basis(spin) else {
        return 0;
    };
    let offsets = match spin.rotation().quarter_turns() {
        0 => [(-1, 1), (1, 1)],
        1 => [(1, -1), (1, 1)],
        2 => [(-1, -1), (1, -1)],
        3 => [(-1, -1), (-1, 1)],
        _ => return 0,
    };
    offsets
        .into_iter()
        .filter(|(dx, dy)| {
            corner_is_blocked(
                spin.board_before() | spin.board_after_placement(),
                center_x + dx,
                center_y + dy,
            )
        })
        .count() as u8
}

fn t_center_for_spin_basis(
    spin: clearra_replay::replay::ReplaySpinBasisEvent,
) -> Option<(i32, i32)> {
    let x = i32::from(spin.x());
    let y = i32::from(spin.y());
    match spin.rotation().quarter_turns() {
        0 => Some((x + 1, y)),
        1 => Some((x, y + 1)),
        2 | 3 => Some((x + 1, y + 1)),
        _ => None,
    }
}

fn corner_is_blocked(occupied: u64, x: i32, y: i32) -> bool {
    const WIDTH: i32 = 10;
    if !(0..WIDTH).contains(&x) || y < 0 {
        return true;
    }
    let index = y as u64 * WIDTH as u64 + x as u64;
    if index >= 64 {
        return false;
    }
    let bit = 1_u64 << index;
    occupied & bit != 0
}
