use clearra_core_ffi::{
    CBuildVariantView, CKickEvidenceView, CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING,
};
use clearra_replay::{
    BuildVariantOperation, BuildVariantReplayInput, KickEvidenceEvent, MovementEvidenceEvent,
    ReplayEngine, ReplayEngineError, ReplayTrace, RotationRequest as ReplayRotationRequest,
    TraceCompleteness as ReplayTraceCompleteness,
};

use crate::spin::build_variant_replay_evidence::BuildVariantReplayEvidence;

pub(crate) struct BuildVariantMapper;

impl BuildVariantMapper {
    #[cfg(test)]
    pub(crate) const REPLAY_BASIS: &'static str = "c-build-variant-operation-replay-basis";
}
impl BuildVariantMapper {
    #[cfg(test)]
    pub(crate) fn to_replay_trace(
        variant: &CBuildVariantView,
        replay_evidence: &BuildVariantReplayEvidence,
    ) -> Result<ReplayTrace, ReplayEngineError> {
        Self::to_replay_trace_with_marker(variant, replay_evidence, false, false)
    }
}
impl BuildVariantMapper {
    pub(crate) fn to_replay_trace_with_marker(
        variant: &CBuildVariantView,
        replay_evidence: &BuildVariantReplayEvidence,
        representative: bool,
        sample: bool,
    ) -> Result<ReplayTrace, ReplayEngineError> {
        let kick_evidence = replay_kick_evidence_for_variant(variant, replay_evidence);
        let input = BuildVariantReplayInput::new(
            format!(
                "bvk2:{:016x}:{:08x}:{:016x}",
                variant.candidate_id(),
                variant.coverage_pattern_id(),
                variant.build_variant_id()
            ),
            replay_evidence.layout(),
            replay_evidence.initial_board(),
            replay_evidence.operations().to_vec(),
        )
        .with_representative_order(replay_evidence.representative_order().to_vec())
        .with_hold_decisions(replay_evidence.hold_decisions().to_vec())
        .with_trace_marker(representative, sample)
        .with_kick_evidence(kick_evidence)
        .with_movement_evidence(replay_movement_evidence_for_variant(variant))
        .with_trace_completeness(replay_trace_completeness(
            variant.trace_completeness_flags(),
        ));

        ReplayEngine::build_variant_to_trace(&input)
    }
}

fn replay_movement_evidence_for_variant(variant: &CBuildVariantView) -> Vec<MovementEvidenceEvent> {
    variant
        .trace_steps()
        .iter()
        .enumerate()
        .map(|(step_index, step)| {
            let last_action_was_rotation = step.reachability.last_action_was_rotation != 0;
            MovementEvidenceEvent::new(
                step_index,
                step.reachability.exhaustive != 0,
                last_action_was_rotation,
                step.reachability.used_kick != 0,
                step.reachability.used_180 != 0,
                step.reachability.rotation_evidence_complete != 0,
            )
        })
        .collect()
}

fn replay_kick_evidence(
    evidence: &CKickEvidenceView,
    operations: &[BuildVariantOperation],
    representative_order: &[usize],
) -> Option<KickEvidenceEvent> {
    let step_index = step_index_for_kick_evidence(evidence, operations, representative_order)?;
    replay_kick_evidence_at_step(evidence, step_index)
}

fn replay_kick_evidence_at_step(
    evidence: &CKickEvidenceView,
    step_index: usize,
) -> Option<KickEvidenceEvent> {
    (evidence.has_kick_evidence != 0).then(|| {
        KickEvidenceEvent::new(
            step_index,
            evidence.from_rotation,
            evidence.to_rotation,
            replay_rotation_request(evidence.rotation_request),
            evidence.kick_index,
            evidence.kick_dx as i16,
            evidence.kick_dy as i16,
        )
        .with_profile_ids(evidence.kick_table_id, evidence.kick_profile_id)
        .with_anchors(
            (evidence.predecessor_x, evidence.predecessor_y),
            (evidence.result_x, evidence.result_y),
        )
        .with_first_success_confirmed(evidence.first_success_confirmed != 0)
    })
}

fn replay_kick_evidence_for_variant(
    variant: &CBuildVariantView,
    replay_evidence: &BuildVariantReplayEvidence,
) -> Vec<KickEvidenceEvent> {
    if !variant.trace_steps().is_empty() {
        return variant
            .trace_steps()
            .iter()
            .enumerate()
            .filter_map(|(step_index, step)| {
                replay_kick_evidence_at_step(
                    step.kick_evidence(variant.kick_evidence())?,
                    step_index,
                )
            })
            .collect();
    }

    variant
        .kick_evidence()
        .iter()
        .filter_map(|evidence| {
            replay_kick_evidence(
                evidence,
                replay_evidence.operations(),
                replay_evidence.representative_order(),
            )
        })
        .collect()
}

fn step_index_for_kick_evidence(
    evidence: &CKickEvidenceView,
    operations: &[BuildVariantOperation],
    representative_order: &[usize],
) -> Option<usize> {
    representative_order
        .iter()
        .copied()
        .enumerate()
        .find_map(|(step_index, operation_index)| {
            let operation = operations.get(operation_index)?;
            (i16::try_from(operation.x()).ok()? == evidence.result_x
                && i16::try_from(operation.y()).ok()? == evidence.result_y
                && operation.rotation().quarter_turns() as u8 == evidence.to_rotation)
                .then_some(step_index)
        })
}

fn replay_rotation_request(raw: u8) -> ReplayRotationRequest {
    match raw {
        1 => ReplayRotationRequest::Clockwise,
        2 => ReplayRotationRequest::CounterClockwise,
        3 => ReplayRotationRequest::HalfTurn,
        _ => ReplayRotationRequest::None,
    }
}

fn replay_trace_completeness(flags: u32) -> ReplayTraceCompleteness {
    if flags & CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING != 0 {
        ReplayTraceCompleteness::MissingKickEvidence
    } else {
        ReplayTraceCompleteness::Complete
    }
}
