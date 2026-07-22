use clearra_core_domain::{
    field::occupancy_field::OccupancyField, operation::operation::OperationId,
};
use clearra_geometry::layout::board64_layout::Board64Layout;

use crate::{
    event::{KickEvidenceEvent, MovementEvidenceEvent, TraceCompleteness, TraceCompletenessEvent},
    replay::replay_event::{
        CellOwner, ReplayBoardSnapshotEvent, ReplayBoardSnapshotPhase, ReplayDropEvent,
        ReplayEvent, ReplayEventId, ReplayHoldReleaseEvent, ReplayHoldStoreEvent,
        ReplayHoldSwapEvent, ReplayLineClearEvent, ReplayLockEvent, ReplayPlacementEvent,
        ReplayScoreBasisEvent, ReplaySpinBasisEvent, ReplayTraceMarker, RowMask,
    },
    trace::{HoldDecision, SolutionTrace},
};

pub(crate) fn replay_events_from_trace(
    trace: &SolutionTrace,
    representative: bool,
    sample: bool,
    kick_evidence: &[KickEvidenceEvent],
    movement_evidence: &[MovementEvidenceEvent],
    trace_completeness: TraceCompleteness,
) -> Vec<ReplayEvent> {
    let mut events = vec![ReplayEvent::TraceMarker(ReplayTraceMarker::new(
        representative,
        sample,
    ))];
    let Some(first_step) = trace.steps().first() else {
        return events;
    };
    let layout = first_step.board_before().layout();
    let initial_occupied = first_step.board_before().occupied();
    let mut owners = initial_cell_owners(layout, initial_occupied);
    events.push(ReplayEvent::BoardSnapshot(ReplayBoardSnapshotEvent::new(
        0,
        ReplayBoardSnapshotPhase::Initial,
        initial_occupied,
    )));
    for step in trace.steps() {
        let placement = step.placement();
        let operation_id = step.operation_id();
        if let Some(hold_event) =
            hold_event_from_decision(step.step_index(), step.piece_decision().hold_decision())
        {
            events.push(hold_event);
        }
        events.push(ReplayEvent::BoardSnapshot(ReplayBoardSnapshotEvent::new(
            step.step_index(),
            ReplayBoardSnapshotPhase::BeforePlacement,
            step.board_before().occupied(),
        )));
        events.push(ReplayEvent::Drop(ReplayDropEvent::new(
            step.step_index(),
            trace_drop_from_y(step.board_before().layout().height()),
            placement.y(),
        )));
        events.push(ReplayEvent::Placement(ReplayPlacementEvent::new(
            step.step_index(),
            placement.piece_kind(),
            placement.rotation(),
            placement.x(),
            placement.y(),
            placement.mask(),
        )));
        events.push(ReplayEvent::SpinBasis(ReplaySpinBasisEvent::new(
            step.step_index(),
            placement.piece_kind(),
            placement.rotation(),
            placement.x(),
            placement.y(),
            step.board_before().occupied(),
            step.board_after().after_placement().occupied(),
            step.line_clear().cleared_lines(),
        )));
        events.push(ReplayEvent::ScoreBasis(ReplayScoreBasisEvent::new(
            step.step_index(),
            placement.piece_kind(),
            step.line_clear().cleared_lines(),
            step.board_before().occupied(),
            step.board_after().after_line_clear().occupied(),
        )));
        let owners_after_placement =
            owners_after_placement(layout, &owners, placement.mask(), operation_id);
        events.push(ReplayEvent::Lock(ReplayLockEvent::new(
            ReplayEventId(u32::try_from(step.step_index()).unwrap_or(u32::MAX)),
            operation_id,
            placement.piece_kind(),
            placement.rotation(),
            placement.x() as i16,
            placement.y() as i16,
            occupancy_field(layout, step.board_before().occupied()),
            occupancy_field(layout, step.board_after().after_placement().occupied()),
            full_cleared_row_mask(layout, step.board_after().after_placement().occupied()),
            cleared_cell_owners(
                layout,
                &owners_after_placement,
                step.board_after().after_placement().occupied(),
            ),
            occupancy_field(layout, step.board_after().after_line_clear().occupied()),
        )));
        owners = compact_cell_owners_after_line_clear(
            layout,
            owners_after_placement,
            step.board_after().after_placement().occupied(),
        );
        events.push(ReplayEvent::BoardSnapshot(ReplayBoardSnapshotEvent::new(
            step.step_index(),
            ReplayBoardSnapshotPhase::AfterPlacement,
            step.board_after().after_placement().occupied(),
        )));
        if step.line_clear().cleared_lines() > 0 {
            events.push(ReplayEvent::LineClear(ReplayLineClearEvent::new(
                step.step_index(),
                step.line_clear().cleared_lines(),
            )));
        }
        events.push(ReplayEvent::BoardSnapshot(ReplayBoardSnapshotEvent::new(
            step.step_index(),
            ReplayBoardSnapshotPhase::AfterLineClear,
            step.board_after().after_line_clear().occupied(),
        )));
    }
    if trace_completeness != TraceCompleteness::Complete {
        events.push(ReplayEvent::TraceCompleteness(TraceCompletenessEvent::new(
            trace_completeness,
        )));
    }
    events.extend(
        movement_evidence
            .iter()
            .copied()
            .map(ReplayEvent::MovementEvidence),
    );
    events.extend(kick_evidence.iter().cloned().map(ReplayEvent::KickEvidence));
    events
}

fn trace_drop_from_y(layout_height: u16) -> u16 {
    layout_height
}

fn occupancy_field(layout: Board64Layout, mask: u64) -> OccupancyField {
    OccupancyField::new(layout.width() as u8, layout.height() as u8, mask)
        .expect("replay trace board masks stay inside layout")
}

fn initial_cell_owners(layout: Board64Layout, occupied: u64) -> Vec<Option<CellOwner>> {
    let mut owners = vec![None; usize::from(layout.cell_count())];
    for (index, owner) in owners.iter_mut().enumerate() {
        if (occupied & (1_u64 << index)) != 0 {
            *owner = Some(CellOwner::InitialGray);
        }
    }
    owners
}

fn owners_after_placement(
    layout: Board64Layout,
    owners: &[Option<CellOwner>],
    placement_mask: u64,
    operation_id: OperationId,
) -> Vec<Option<CellOwner>> {
    let mut next = owners.to_vec();
    for (index, owner) in next
        .iter_mut()
        .enumerate()
        .take(usize::from(layout.cell_count()))
    {
        if (placement_mask & (1_u64 << index)) != 0 {
            *owner = Some(CellOwner::Piece(operation_id));
        }
    }
    next
}

fn full_cleared_row_mask(layout: Board64Layout, occupied_after_placement: u64) -> RowMask {
    let mut cleared = 0_u64;
    for y in 0..usize::from(layout.height()) {
        let row = row_mask(usize::from(layout.width()), y);
        if occupied_after_placement & row == row {
            cleared |= row;
        }
    }
    RowMask(cleared)
}

fn cleared_cell_owners(
    layout: Board64Layout,
    owners_after_placement: &[Option<CellOwner>],
    occupied_after_placement: u64,
) -> Vec<CellOwner> {
    let cleared = full_cleared_row_mask(layout, occupied_after_placement).0;
    let mut owners = Vec::new();
    for (index, owner) in owners_after_placement
        .iter()
        .enumerate()
        .take(usize::from(layout.cell_count()))
    {
        if (cleared & (1_u64 << index)) != 0 {
            owners.push(owner.unwrap_or(CellOwner::InitialGray));
        }
    }
    owners
}

fn compact_cell_owners_after_line_clear(
    layout: Board64Layout,
    owners_after_placement: Vec<Option<CellOwner>>,
    occupied_after_placement: u64,
) -> Vec<Option<CellOwner>> {
    let width = usize::from(layout.width());
    let height = usize::from(layout.height());
    let mut compacted = vec![None; owners_after_placement.len()];
    let mut dest_y = 0_usize;

    for source_y in 0..height {
        let row = row_mask(width, source_y);
        if occupied_after_placement & row == row {
            continue;
        }

        for x in 0..width {
            let source = source_y * width + x;
            let dest = dest_y * width + x;
            compacted[dest] = owners_after_placement[source];
        }
        dest_y += 1;
    }

    compacted
}

fn row_mask(width: usize, y: usize) -> u64 {
    let start = y * width;
    ((1_u64 << width) - 1) << start
}

fn hold_event_from_decision(step_index: usize, decision: HoldDecision) -> Option<ReplayEvent> {
    match decision {
        HoldDecision::None => None,
        HoldDecision::SwapWithHold {
            incoming_piece,
            held_piece,
        } => Some(ReplayEvent::HoldSwap(ReplayHoldSwapEvent::new(
            step_index,
            incoming_piece,
            held_piece,
        ))),
        HoldDecision::StoreIncoming { stored_piece, .. } => Some(ReplayEvent::HoldStore(
            ReplayHoldStoreEvent::new(step_index, stored_piece),
        )),
        HoldDecision::ReleaseHeldAtTerminal { held_piece } => Some(ReplayEvent::HoldRelease(
            ReplayHoldReleaseEvent::new(step_index, held_piece),
        )),
    }
}
