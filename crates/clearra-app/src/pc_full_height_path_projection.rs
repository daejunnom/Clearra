//! Selected four-word witness projection for the same public PC-path contract.
//! Counts and membership are supplied by the producer-bound language, not here.
use super::*;
use clearra_postprocess::FullHeightCandidateExecution;
use clearra_replay::FullHeightReplayProjector;

pub(crate) fn checked_full_height_execution_projection_peak_bytes(
    execution: &FullHeightCandidateExecution,
) -> Option<u128> {
    (core::mem::size_of::<PcPathWitnessV2>() as u128)
        .checked_add((execution.trace_identity().len() as u128).checked_mul(2)?)?
        .checked_add(
            (execution.replay_trace().steps().len() as u128).checked_mul(
                (core::mem::size_of::<PcPathStepV2>() + PC_PATH_ROW_IDENTITY_MAX_BYTES) as u128,
            )?,
        )
}

pub(crate) fn project_full_height_execution(
    context: PcPathProjectionContext,
    producer_candidate_id: u64,
    execution: &FullHeightCandidateExecution,
    pattern_count: usize,
    candidate_id: u64,
) -> Result<PcPathWitnessV2, &'static str> {
    let PcPathBoardMask::FullHeight { height, occupied } = context.initial_board else {
        return Err("pc path full-height projection cannot consume a compact board");
    };
    let trace = execution.replay_trace();
    if execution.pattern_id() >= pattern_count
        || trace.height() != height
        || trace.initial() != occupied
        || trace.steps().is_empty()
        || !trace.canonical_key_matches(execution.trace_identity())
    {
        return Err("pc path full-height trace identity is invalid");
    }
    let mut board = occupied;
    let mut cursor = context.initial_cursor;
    let mut hold = context.initial_hold;
    let mut used_operations = 0_u64;
    let mut steps = Vec::new();
    steps
        .try_reserve_exact(trace.steps().len())
        .map_err(|_| "complete_replay_allocation_failed")?;
    let mask = |occupied| PcPathBoardMask::FullHeight { height, occupied };
    for (index, step) in trace.steps().iter().copied().enumerate() {
        let edge = step.edge();
        let decision = step.decision();
        let transition = FullHeightReplayProjector::project_scoring_step(height, board, edge)
            .map_err(|_| "pc path full-height physical chain is invalid")?;
        if edge.operation_index() >= 60
            || used_operations & (1_u64 << edge.operation_index()) != 0
            || step.before() != board
            || transition != step.transition()
            || decision.active_piece() != edge.piece()
            || decision.input_cursor() != cursor
            || decision.input_hold_piece() != hold
        {
            return Err("pc path full-height supply/physical chain is invalid");
        }
        used_operations |= 1_u64 << edge.operation_index();
        let cleared_row_mask = u64::from(transition.cleared_row_mask());
        let cleared_lines = transition.cleared_lines();
        let mut line_clear_identity = String::with_capacity(PC_PATH_ROW_IDENTITY_MAX_BYTES);
        use core::fmt::Write;
        write!(
            &mut line_clear_identity,
            "rows:{cleared_row_mask:016x}:count:{cleared_lines}"
        )
        .map_err(|_| "pc path full-height row identity formatting failed")?;
        steps.push(PcPathStepV2 {
            step_index: index,
            operation_id: u16::from(edge.operation_index()),
            active_piece: decision.active_piece(),
            input_cursor: decision.input_cursor(),
            output_cursor: decision.output_cursor(),
            input_hold_piece: decision.input_hold_piece(),
            output_hold_piece: decision.output_hold_piece(),
            hold_decision: decision.hold_decision().as_str(),
            rotation: edge.rotation().quarter_turns(),
            x: u16::try_from(edge.x()).map_err(|_| "pc path full-height x origin invalid")?,
            y: u16::try_from(edge.y()).map_err(|_| "pc path full-height y origin invalid")?,
            placement_mask: mask(transition.placement()),
            board_before_mask: mask(board),
            board_after_placement_mask: mask(transition.after_placement()),
            board_after_line_clear_mask: mask(transition.after_line_clear()),
            cleared_row_mask,
            cleared_lines,
            line_clear_identity,
        });
        board = transition.after_line_clear();
        cursor = decision.output_cursor();
        hold = decision.output_hold_piece();
    }
    if !board.is_empty() {
        return Err("pc path full-height replay does not clear to empty");
    }
    Ok(PcPathWitnessV2 {
        candidate_id,
        producer_candidate_id,
        pattern_id: execution.pattern_id(),
        trace_identity: execution.trace_identity().to_owned(),
        normalized_trace_key: execution.trace_identity().to_owned(),
        consumed_piece_count: cursor,
        terminal_hold_piece: hold,
        steps,
    })
}
