use clearra_replay::{
    board::board64_state::Board64State, trace::PlacementStep, KickEvidenceEvent,
    MovementEvidenceEvent, ReplayEvent, ReplayTrace, RotationRequest as ReplayRotationRequest,
    ScoringExecutionEdge, TraceCompleteness as ReplayCompleteness,
};

use crate::spin::t_spin_corner_rule::{exact_t_spin_mini, fifth_kick_test_regular_override};
use crate::{
    event::spin_event::SpinEvent,
    profile::{NonTSpinRecognition, ScoreProfile, SpinProfile, SpinRuleId, TSpinRecognition},
    spin::{
        BoardAnchor, KickEvidence, MovementInfo, RotationRequest, SpinClassificationInput,
        SpinClassifier, TSpinCornerRule, TraceCompleteness,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpinDetector;

impl SpinDetector {
    /// spin_detector_postprocess_only: score-event spin detection consumes
    /// accepted replay evidence. Search pruning must use validated search-domain
    /// proofs, and unknown_spin_not_false_for_pc_pruning remains the search rule.
    pub fn detect(
        step: PlacementStep,
        spin_rule: SpinRuleId,
        cleared_lines: u8,
    ) -> Option<SpinEvent> {
        Self::detect_with_profile(step, SpinProfile::builtin(spin_rule), cleared_lines)
    }

    pub fn detect_with_profile(
        step: PlacementStep,
        profile: SpinProfile,
        cleared_lines: u8,
    ) -> Option<SpinEvent> {
        match profile.t_spin_recognition() {
            TSpinRecognition::Disabled => None,
            TSpinRecognition::Simple => simple_t_spin(step, cleared_lines),
            TSpinRecognition::ThreeCorner | TSpinRecognition::ThreeCornerOrImmobileMini => {
                corner_based_t_spin(step, cleared_lines)
            }
        }
    }
}
impl SpinDetector {
    pub fn is_exact_t_spin_single_edge(edge: ScoringExecutionEdge) -> bool {
        if edge.cleared_lines() != 1 || edge.piece().as_ascii() != 'T' {
            return false;
        }
        let evidence = edge.lock_evidence();
        let final_kick_override = fifth_kick_test_regular_override(
            evidence.first_success_confirmed(),
            evidence.kick_index(),
            matches!(
                evidence.rotation_request(),
                ReplayRotationRequest::Clockwise | ReplayRotationRequest::CounterClockwise
            ),
        );
        exact_t_spin_mini(
            'T',
            evidence.last_action_was_rotation(),
            edge.blocked_t_corners(),
            edge.blocked_t_front_corners(),
            final_kick_override,
        ) == Some(false)
    }

    pub fn detect_scoring_edge(
        edge: ScoringExecutionEdge,
        spin_rule: SpinRuleId,
    ) -> Option<SpinEvent> {
        Self::detect_scoring_edge_with_profile(edge, SpinProfile::builtin(spin_rule))
    }

    pub fn detect_scoring_edge_with_profile(
        edge: ScoringExecutionEdge,
        profile: SpinProfile,
    ) -> Option<SpinEvent> {
        if edge.piece().as_ascii() == 'T' {
            return match profile.t_spin_recognition() {
                TSpinRecognition::Disabled => None,
                TSpinRecognition::Simple => Some(SpinEvent::new('T', false, edge.cleared_lines())),
                TSpinRecognition::ThreeCorner => exact_t_spin_event_from_edge(edge),
                TSpinRecognition::ThreeCornerOrImmobileMini => exact_t_spin_event_from_edge(edge)
                    .or_else(|| {
                        immobile_profile_fallback(
                            profile,
                            edge.piece().as_ascii(),
                            edge.lock_evidence().last_action_was_rotation(),
                            edge.lock_evidence().immobile_before_clear(),
                            edge.cleared_lines(),
                        )
                    }),
            };
        }
        let evidence = edge.lock_evidence();
        immobile_profile_fallback(
            profile,
            edge.piece().as_ascii(),
            evidence.last_action_was_rotation(),
            evidence.immobile_before_clear(),
            edge.cleared_lines(),
        )
    }
}
impl SpinDetector {
    pub fn detect_replay_step(
        trace: &ReplayTrace,
        step: PlacementStep,
        spin_rule: SpinRuleId,
        cleared_lines: u8,
    ) -> Option<SpinEvent> {
        Self::detect_replay_step_with_profile(
            trace,
            step,
            SpinProfile::builtin(spin_rule),
            cleared_lines,
        )
    }

    pub fn detect_replay_step_with_profile(
        trace: &ReplayTrace,
        step: PlacementStep,
        profile: SpinProfile,
        cleared_lines: u8,
    ) -> Option<SpinEvent> {
        match profile.t_spin_recognition() {
            TSpinRecognition::Disabled => None,
            TSpinRecognition::Simple => simple_t_spin(step, cleared_lines),
            TSpinRecognition::ThreeCorner | TSpinRecognition::ThreeCornerOrImmobileMini => {
                let input = spin_classification_input_from_replay(trace, step, cleared_lines)?;
                let t_spin = TSpinCornerRule
                    .classify(
                        input.clone(),
                        &ScoreProfile::new("t-spin-corner", "T-spin corner rule"),
                    )
                    .result();
                if t_spin.is_spin() {
                    return Some(SpinEvent::new(
                        t_spin.piece(),
                        t_spin.is_mini(),
                        t_spin.cleared_lines(),
                    ));
                }
                immobile_profile_fallback(
                    profile,
                    input.piece,
                    input.movement_info.rotation_used,
                    input.movement_info.immobile,
                    input.cleared_lines,
                )
            }
        }
    }
}
impl SpinDetector {
    pub fn detect_with_classifier(
        step: PlacementStep,
        classifier: &dyn SpinClassifier,
        profile: &ScoreProfile,
        cleared_lines: u8,
    ) -> Option<SpinEvent> {
        let input = spin_classification_input_from_step(step, cleared_lines)?;
        let result = classifier.classify(input, profile).result();
        result.is_spin().then_some(SpinEvent::new(
            result.piece(),
            result.is_mini(),
            result.cleared_lines(),
        ))
    }
}

fn simple_t_spin(step: PlacementStep, cleared_lines: u8) -> Option<SpinEvent> {
    let piece = step.piece_decision().active_piece();
    (piece.as_ascii() == 'T').then_some(SpinEvent::new('T', false, cleared_lines))
}

fn corner_based_t_spin(step: PlacementStep, cleared_lines: u8) -> Option<SpinEvent> {
    SpinDetector::detect_with_classifier(
        step,
        &TSpinCornerRule,
        &ScoreProfile::new("t-spin-corner", "T-spin corner rule"),
        cleared_lines,
    )
}

fn exact_t_spin_event_from_edge(edge: ScoringExecutionEdge) -> Option<SpinEvent> {
    if edge.piece().as_ascii() != 'T' {
        return None;
    }
    let evidence = edge.lock_evidence();
    let final_kick_override = fifth_kick_test_regular_override(
        evidence.first_success_confirmed(),
        evidence.kick_index(),
        matches!(
            evidence.rotation_request(),
            ReplayRotationRequest::Clockwise | ReplayRotationRequest::CounterClockwise
        ),
    );
    exact_t_spin_mini(
        'T',
        evidence.last_action_was_rotation(),
        edge.blocked_t_corners(),
        edge.blocked_t_front_corners(),
        final_kick_override,
    )
    .map(|mini| SpinEvent::new('T', mini, edge.cleared_lines()))
}

fn immobile_profile_fallback(
    spin_profile: SpinProfile,
    piece: char,
    last_action_was_rotation: bool,
    immobile: bool,
    cleared_lines: u8,
) -> Option<SpinEvent> {
    if !last_action_was_rotation || !immobile {
        return None;
    }
    let piece = piece.to_ascii_uppercase();
    let mini = if piece == 'T' {
        if !spin_profile.allows_immobile_t_fallback() {
            return None;
        }
        true
    } else {
        match spin_profile.non_t_spin_recognition() {
            NonTSpinRecognition::Disabled => return None,
            NonTSpinRecognition::ImmobileRegular => false,
            NonTSpinRecognition::ImmobileMini => true,
        }
    };
    Some(SpinEvent::new(piece, mini, cleared_lines))
}

fn spin_classification_input_from_step(
    step: PlacementStep,
    cleared_lines: u8,
) -> Option<SpinClassificationInput> {
    let placement = step.placement();
    Some(SpinClassificationInput {
        piece: step.piece_decision().active_piece().as_ascii(),
        rotation: placement.rotation().quarter_turns(),
        x: placement.x() as i16,
        y: placement.y() as i16,
        board_before: step.board_before().occupied(),
        board_after_placement: step.board_after().after_placement().occupied(),
        board_after_clear: step.board_after().after_line_clear().occupied(),
        cleared_lines,
        blocked_corners: blocked_t_corners_for_step(step)?,
        front_corners: blocked_t_front_corners_for_step(step)?,
        kick_evidence: None,
        movement_info: MovementInfo {
            immobile: placement_is_immobile(step),
            rotation_used: false,
            evidence_complete: false,
        },
        trace_completeness: crate::spin::TraceCompleteness::Full,
    })
}

fn spin_classification_input_from_replay(
    trace: &ReplayTrace,
    step: PlacementStep,
    cleared_lines: u8,
) -> Option<SpinClassificationInput> {
    let movement = movement_evidence_for_step(trace, step.step_index());
    let kick = kick_evidence_for_step(trace, step.step_index());
    let trace_completeness = replay_trace_completeness(trace);
    let blocked_corners = blocked_t_corners_for_step(step)?;
    let front_corners = blocked_t_front_corners_for_step(step)?;
    Some(SpinClassificationInput {
        piece: step.piece_decision().active_piece().as_ascii(),
        rotation: step.placement().rotation().quarter_turns(),
        x: step.placement().x() as i16,
        y: step.placement().y() as i16,
        board_before: step.board_before().occupied(),
        board_after_placement: step.board_after().after_placement().occupied(),
        board_after_clear: step.board_after().after_line_clear().occupied(),
        cleared_lines,
        blocked_corners,
        front_corners,
        kick_evidence: kick.map(scoring_kick_evidence),
        movement_info: MovementInfo {
            immobile: placement_is_immobile(step),
            rotation_used: movement.is_some_and(|evidence| evidence.last_action_was_rotation()),
            evidence_complete: movement.is_some_and(|evidence| {
                evidence.path_complete() && evidence.rotation_evidence_complete()
            }),
        },
        trace_completeness,
    })
}

fn placement_is_immobile(step: PlacementStep) -> bool {
    let layout = step.board_before().layout();
    let width = usize::from(layout.width());
    let placement = step.placement();
    let mask = placement.mask();
    let occupied = step.board_before().occupied();
    let mut left_wall = 0_u64;
    let mut right_wall = 0_u64;
    for row in 0..usize::from(layout.height()) {
        left_wall |= 1_u64 << (row * width);
        right_wall |= 1_u64 << (row * width + width - 1);
    }
    let bottom_mask = (1_u64 << width) - 1;
    let can_move_down = mask & bottom_mask == 0 && occupied & (mask >> width) == 0;
    let can_move_left = mask & left_wall == 0 && occupied & (mask >> 1) == 0;
    let can_move_right = mask & right_wall == 0
        && (mask << 1) & !layout.all_cells_mask() == 0
        && occupied & (mask << 1) == 0;
    let can_move_up = occupied & (mask << width) == 0;
    !(can_move_down || can_move_left || can_move_right || can_move_up)
}

fn movement_evidence_for_step(
    trace: &ReplayTrace,
    step_index: usize,
) -> Option<MovementEvidenceEvent> {
    trace.events().iter().find_map(|event| match event {
        ReplayEvent::MovementEvidence(evidence) if evidence.step_index() == step_index => {
            Some(*evidence)
        }
        _ => None,
    })
}

fn kick_evidence_for_step(trace: &ReplayTrace, step_index: usize) -> Option<&KickEvidenceEvent> {
    trace.events().iter().find_map(|event| match event {
        ReplayEvent::KickEvidence(evidence) if evidence.step_index() == step_index => {
            Some(evidence)
        }
        _ => None,
    })
}

fn scoring_kick_evidence(event: &KickEvidenceEvent) -> KickEvidence {
    let predecessor = event.predecessor();
    let result = event.result();
    KickEvidence {
        from_rotation: event.from_rotation(),
        to_rotation: event.to_rotation(),
        rotation_request: match event.rotation_request() {
            ReplayRotationRequest::None => RotationRequest::None,
            ReplayRotationRequest::Clockwise => RotationRequest::Clockwise,
            ReplayRotationRequest::CounterClockwise => RotationRequest::CounterClockwise,
            ReplayRotationRequest::HalfTurn => RotationRequest::HalfTurn,
        },
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

fn replay_trace_completeness(trace: &ReplayTrace) -> TraceCompleteness {
    trace
        .events()
        .iter()
        .find_map(|event| match event {
            ReplayEvent::TraceCompleteness(completeness) => {
                Some(match completeness.completeness() {
                    ReplayCompleteness::Complete => TraceCompleteness::Full,
                    ReplayCompleteness::MissingKickEvidence => {
                        TraceCompleteness::MissingKickEvidence
                    }
                    ReplayCompleteness::SampleOnly => TraceCompleteness::RetainedSample,
                    ReplayCompleteness::Incomplete => TraceCompleteness::Incomplete,
                })
            }
            _ => None,
        })
        .unwrap_or(TraceCompleteness::Full)
}

fn blocked_t_corners_for_step(step: PlacementStep) -> Option<u8> {
    if step.piece_decision().active_piece().as_ascii() != 'T' {
        return Some(0);
    }

    let (center_x, center_y) = t_center_for_step(step)?;
    Some(
        t_corner_offsets()
            .into_iter()
            .filter(|(dx, dy)| {
                corner_is_blocked(
                    step.board_before(),
                    step.board_after().after_placement(),
                    center_x + dx,
                    center_y + dy,
                )
            })
            .count() as u8,
    )
}

fn blocked_t_front_corners_for_step(step: PlacementStep) -> Option<u8> {
    if step.piece_decision().active_piece().as_ascii() != 'T' {
        return Some(0);
    }
    let (center_x, center_y) = t_center_for_step(step)?;
    let offsets = match step.placement().rotation().quarter_turns() {
        0 => [(-1, 1), (1, 1)],
        1 => [(1, -1), (1, 1)],
        2 => [(-1, -1), (1, -1)],
        3 => [(-1, -1), (-1, 1)],
        _ => return None,
    };
    Some(
        offsets
            .into_iter()
            .filter(|(dx, dy)| {
                corner_is_blocked(
                    step.board_before(),
                    step.board_after().after_placement(),
                    center_x + dx,
                    center_y + dy,
                )
            })
            .count() as u8,
    )
}

fn t_center_for_step(step: PlacementStep) -> Option<(i32, i32)> {
    let placement = step.placement();
    let x = i32::from(placement.x());
    let y = i32::from(placement.y());
    match placement.rotation().quarter_turns() {
        0 => Some((x + 1, y)),
        1 => Some((x, y + 1)),
        2 | 3 => Some((x + 1, y + 1)),
        _ => None,
    }
}

fn t_corner_offsets() -> [(i32, i32); 4] {
    [(-1, -1), (1, -1), (-1, 1), (1, 1)]
}

fn corner_is_blocked(before: Board64State, after_placement: Board64State, x: i32, y: i32) -> bool {
    let layout = before.layout();
    if x < 0 || y < 0 || x >= i32::from(layout.width()) {
        return true;
    }
    if y >= i32::from(layout.height()) {
        return false;
    }

    let index = y as u64 * u64::from(layout.width()) + x as u64;
    let bit = 1_u64 << index;
    (before.occupied() | after_placement.occupied()) & bit != 0
}

#[cfg(test)]
#[path = "spin_detector_tests.rs"]
mod tests;
