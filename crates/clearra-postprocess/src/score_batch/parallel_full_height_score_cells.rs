//! Full-height cells use the common physical traversal and one admitted profile.
use super::*;
use clearra_core_domain::parallel_work::{try_for_each_chunk, ParallelWorkError};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};

// Physical path counts vary independently of pattern/candidate ordinals.
const CELLS_PER_CHUNK: usize = 1;

struct Workspace {
    path: Vec<ScoringExecutionEdge>,
    holds: Vec<HoldDecision>,
    selected: Vec<(ScoringExecutionEdge, HoldDecision)>,
    peak: u128,
    slack_peak: u128,
    charged_peak: u128,
}

pub(super) fn workspace_count(cells: usize, workers: usize) -> usize {
    #[cfg(target_family = "wasm")]
    let workers = {
        let _ = workers;
        1
    };
    workers.max(1).min(cells.div_ceil(CELLS_PER_CHUNK).max(1))
}

pub(super) fn checked_control_bytes(candidates: usize, workers: usize) -> Option<u128> {
    (candidates as u128)
        .checked_mul(core::mem::size_of::<(usize, usize, u64)>() as u128)?
        .checked_add((workers as u128).checked_mul(core::mem::size_of::<Workspace>() as u128)?)?
        .checked_add(
            (core::mem::size_of::<Mutex<u128>>()
                + core::mem::size_of::<Mutex<Vec<ScoreCell>>>()
                + core::mem::size_of::<AtomicBool>()
                + core::mem::size_of::<AtomicUsize>()) as u128,
        )
}

fn charge(
    ledger: &Mutex<u128>,
    additional: u128,
    maximum: u128,
) -> Result<(), FullHeightScoreCellError> {
    let mut peak = ledger
        .lock()
        .map_err(|_| FullHeightScoreCellError::ParallelWorkerFailed)?;
    let next = peak
        .checked_add(additional)
        .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
    ensure_limit(next, maximum)?;
    *peak = next;
    Ok(())
}

fn charge_workspace(
    ledger: &Mutex<u128>,
    charged: &mut u128,
    required: u128,
    maximum: u128,
) -> Result<(), FullHeightScoreCellError> {
    if required > *charged {
        charge(ledger, required - *charged, maximum)?;
        *charged = required;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn materialize(
    batch: &FullHeightExecutionBatch,
    policy: ScoreObjectivePolicy,
    profile: &clearra_scoring::profile::ScoreProfile,
    projection: FullHeightScoreCellMemoryProjection,
    source_start: usize,
    prepared_ranges: Option<&[(usize, usize, u64)]>,
    control: &ExecutionControl,
    already_retained: u128,
    maximum: u128,
) -> Result<FullHeightScoreCellMaterialization, FullHeightScoreCellError> {
    let peak = already_retained
        .checked_add(projection.required_peak_bytes)
        .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
    ensure_limit(peak, maximum)?;
    let ledger = Mutex::new(peak);
    let serial_scratch = checked_scratch_bytes(projection.max_path_len)
        .ok_or(FullHeightScoreCellError::ProjectionOverflow)?;
    let mut ranges = Vec::new();
    let graphs = batch.execution().graphs();
    if prepared_ranges.is_none() {
        ranges
            .try_reserve_exact(projection.candidate_count)
            .map_err(|_| FullHeightScoreCellError::AllocationFailed)?;
        charge(
            &ledger,
            (ranges.capacity().saturating_sub(projection.candidate_count) as u128)
                .checked_mul(core::mem::size_of::<(usize, usize, u64)>() as u128)
                .ok_or(FullHeightScoreCellError::ProjectionOverflow)?,
            maximum,
        )?;
        let mut first = 0;
        while first < graphs.len() {
            let end = first
                + graphs[first..]
                    .iter()
                    .take_while(|g| g.candidate_id() == graphs[first].candidate_id())
                    .count();
            let identity =
                ExtendedTilingSolutionKey::parse_canonical(graphs[first].candidate_key())
                    .map_err(|_| FullHeightScoreCellError::InvalidEvidence)?;
            ranges.push((first, end, (1_u64 << identity.placement_count()) - 1));
            first = end;
        }
    }
    let ranges = prepared_ranges.unwrap_or(&ranges);
    let mut cells = Vec::new();
    cells
        .try_reserve_exact(projection.cell_capacity)
        .map_err(|_| FullHeightScoreCellError::AllocationFailed)?;
    charge(
        &ledger,
        (cells.capacity().saturating_sub(projection.cell_capacity) as u128)
            .checked_mul(core::mem::size_of::<ScoreCell>() as u128)
            .ok_or(FullHeightScoreCellError::ProjectionOverflow)?,
        maximum,
    )?;
    let output = Mutex::new(cells);
    let complete = AtomicBool::new(batch.execution().complete());
    let completed = AtomicUsize::new(0);
    try_for_each_chunk(
        control,
        projection.cell_capacity,
        CELLS_PER_CHUNK,
        || {
            let mut workspace = Workspace {
                path: Vec::new(),
                holds: Vec::new(),
                selected: Vec::new(),
                peak: serial_scratch,
                charged_peak: serial_scratch,
                slack_peak: 0,
            };
            reserve_scratch(
                &mut workspace.path,
                projection.max_path_len,
                &mut workspace.peak,
                maximum,
            )?;
            reserve_scratch(
                &mut workspace.holds,
                projection.max_path_len,
                &mut workspace.peak,
                maximum,
            )?;
            reserve_scratch(
                &mut workspace.selected,
                projection.max_path_len,
                &mut workspace.peak,
                maximum,
            )?;
            charge_workspace(
                &ledger,
                &mut workspace.charged_peak,
                workspace.peak,
                maximum,
            )?;
            Ok(workspace)
        },
        |workspace, tasks| {
            let count = tasks.len();
            for local_ordinal in tasks {
                let ordinal = source_start + local_ordinal;
                let candidate = ordinal / projection.pattern_count;
                let pattern = ordinal % projection.pattern_count;
                let (first, end, required_operations) = ranges[candidate];
                let id = graphs[first].candidate_id();
                let physical = PhysicalProjection {
                    height: batch.height(),
                    required_operations,
                };
                let (best, cell_complete) = visit_cell(
                    batch,
                    policy,
                    profile,
                    first..end,
                    pattern,
                    &physical,
                    &mut workspace.path,
                    &mut workspace.holds,
                    &mut workspace.selected,
                    projection.max_path_len,
                    &mut workspace.peak,
                    &mut workspace.slack_peak,
                    maximum,
                    control,
                    &mut |required| {
                        charge_workspace(&ledger, &mut workspace.charged_peak, required, maximum)
                    },
                )?;
                if !cell_complete {
                    complete.store(false, Ordering::Relaxed);
                }
                if let Some(best) = best {
                    let trace = compact_score_cell_trace_identity(id, pattern)
                        .map_err(|_| FullHeightScoreCellError::AllocationFailed)?;
                    charge(
                        &ledger,
                        trace
                            .capacity()
                            .saturating_sub(COMPACT_SCORE_CELL_TRACE_ID_BYTES)
                            as u128,
                        maximum,
                    )?;
                    let cell = ScoreCell::new_with_static_accuracy(
                        id,
                        pattern,
                        trace,
                        best.score,
                        best.attack,
                        profile.accuracy_level().as_str(),
                    );
                    let mut cells = output
                        .lock()
                        .map_err(|_| FullHeightScoreCellError::ParallelWorkerFailed)?;
                    if cells.len() == cells.capacity() {
                        return Err(FullHeightScoreCellError::ProjectionOverflow);
                    }
                    cells.push(cell);
                }
            }
            let done = completed.fetch_add(count, Ordering::AcqRel) + count;
            control.report_progress(
                "score-cell-execution",
                done as u64,
                Some(projection.cell_capacity as u64),
            );
            Ok(())
        },
    )
    .map_err(|error| match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => FullHeightScoreCellError::Cancelled,
        ParallelWorkError::ThreadSpawnFailed => FullHeightScoreCellError::AllocationFailed,
        ParallelWorkError::WorkerPanicked => FullHeightScoreCellError::ParallelWorkerFailed,
    })?;
    let mut cells = output
        .into_inner()
        .map_err(|_| FullHeightScoreCellError::ParallelWorkerFailed)?;
    cells.sort_unstable_by_key(|cell| (cell.candidate_id(), cell.pattern_id()));
    if control.is_cancelled() {
        return Err(FullHeightScoreCellError::Cancelled);
    }
    Ok(FullHeightScoreCellMaterialization {
        cells,
        complete: complete.load(Ordering::Relaxed),
        admitted_peak_bytes: ledger
            .into_inner()
            .map_err(|_| FullHeightScoreCellError::ParallelWorkerFailed)?,
    })
}
