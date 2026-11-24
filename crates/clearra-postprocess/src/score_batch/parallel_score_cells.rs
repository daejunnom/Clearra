//! Independent graph/pattern cells share immutable source data and one score
//! profile. Results return to their original ordinal before public reduction.
use super::*;
use clearra_core_domain::parallel_work::{try_for_each_chunk, ParallelWorkError};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};

const CELLS_PER_CHUNK: usize = 16;

pub(super) fn workspace_count(cells: usize, parallelism: usize) -> usize {
    #[cfg(target_family = "wasm")]
    let parallelism = {
        let _ = parallelism;
        1
    };
    parallelism
        .max(1)
        .min(cells.div_ceil(CELLS_PER_CHUNK).max(1))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn materialize(
    batch: &ExactScoringExecutionBatch,
    policy: ScoreObjectivePolicy,
    profile: &ScoreProfile,
    projection: ExactScoreCellMemoryProjection,
    control: &ExecutionControl,
    already_retained_bytes: u128,
    max_memory_bytes: u128,
) -> Result<
    (ExactScoreCellMaterialization, ExactScoreCellMemoryReport),
    ExactScoreCellMaterializationError,
> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(projection.cell_capacity)
        .map_err(|_| ExactScoreCellMaterializationError::AllocationFailed)?;
    let output = Mutex::new(output);
    let complete = AtomicBool::new(batch.complete());
    let done = AtomicUsize::new(0);
    let evaluation_policy = ScoreEvaluationPolicy::tetrio_pc(policy.initial_b2b());
    try_for_each_chunk(
        control,
        projection.cell_capacity,
        CELLS_PER_CHUNK,
        || {
            let mut path = Vec::new();
            let mut holds = Vec::new();
            path.try_reserve_exact(projection.max_path_len)
                .map_err(|_| ExactScoreCellMaterializationError::AllocationFailed)?;
            holds
                .try_reserve_exact(projection.max_path_len)
                .map_err(|_| ExactScoreCellMaterializationError::AllocationFailed)?;
            Ok((path, holds))
        },
        |(path, holds), range| {
            let count = range.len();
            for ordinal in range {
                let graph = &batch.graphs()[ordinal / projection.pattern_count];
                let pattern_id = ordinal % projection.pattern_count;
                let sequence = &batch.patterns()[pattern_id];
                path.clear();
                holds.clear();
                let mut best = None;
                if !visit_score_cell_paths(
                    batch,
                    graph,
                    sequence,
                    SupplyState {
                        node: graph.root(),
                        cursor: batch.initial_cursor(),
                        hold: batch.initial_hold(),
                    },
                    path,
                    holds,
                    projection.max_path_len,
                    profile,
                    ScoreModelEvaluator::initial_state(evaluation_policy),
                    &mut best,
                    control,
                )? {
                    complete.store(false, Ordering::Relaxed);
                }
                if let Some(best) = best {
                    let execution = ExactScoredExecution {
                        source_ordinal: ordinal,
                        candidate_identity: graph.identity(),
                        pattern_id,
                        trace_identity: compact_score_cell_trace_identity(
                            graph.candidate_id(),
                            pattern_id,
                        )?,
                        score: best.score,
                        attack: best.attack,
                    };
                    let mut output = output
                        .lock()
                        .map_err(|_| ExactScoreCellMaterializationError::ParallelWorkerFailed)?;
                    if output.len() == output.capacity() {
                        return Err(ExactScoreCellMaterializationError::ProjectionOverflow);
                    }
                    output.push(execution);
                }
            }
            let completed = done.fetch_add(count, Ordering::AcqRel) + count;
            control.report_progress(
                "score-cell-execution",
                completed as u64,
                Some(projection.cell_capacity as u64),
            );
            Ok(())
        },
    )
    .map_err(|error| match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => ExactScoreCellMaterializationError::Cancelled,
        ParallelWorkError::ThreadSpawnFailed => {
            ExactScoreCellMaterializationError::AllocationFailed
        }
        ParallelWorkError::WorkerPanicked => {
            ExactScoreCellMaterializationError::ParallelWorkerFailed
        }
    })?;
    let mut scored_executions = output
        .into_inner()
        .map_err(|_| ExactScoreCellMaterializationError::ParallelWorkerFailed)?;
    scored_executions.sort_unstable_by_key(|execution| execution.source_ordinal);
    if control.is_cancelled() {
        return Err(ExactScoreCellMaterializationError::Cancelled);
    }
    let materialized = ExactScoreCellMaterialization {
        scored_executions,
        complete: complete.load(Ordering::Relaxed),
    };
    let retained_bytes = materialized
        .checked_retained_bytes()
        .ok_or(ExactScoreCellMaterializationError::ProjectionOverflow)?;
    let required_memory_bytes = already_retained_bytes
        .checked_add(retained_bytes)
        .ok_or(ExactScoreCellMaterializationError::ProjectionOverflow)?;
    if required_memory_bytes > max_memory_bytes {
        return Err(ExactScoreCellMaterializationError::LimitExceeded {
            required_memory_bytes,
            max_memory_bytes,
        });
    }
    Ok((
        materialized,
        ExactScoreCellMemoryReport {
            projection,
            retained_bytes,
        },
    ))
}
