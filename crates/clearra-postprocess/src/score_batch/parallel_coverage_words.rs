//! Exact graph/supply products over bounded pattern-word windows. The source
//! graph and original pattern universe are borrowed; workers never copy the
//! complete pattern catalog or allocate a full-universe transition cache.
use super::*;
use clearra_core_domain::parallel_work::{try_for_each_chunk, ParallelWorkError};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};

const PATTERNS_PER_WINDOW: usize = 1024;

struct GraphWords {
    key: String,
    id: u64,
    words: Vec<u64>,
}

pub(super) fn applies(batch: SpinBatchRef<'_>, parallelism: usize) -> bool {
    batch.patterns().len() > PATTERNS_PER_WINDOW
        || (parallelism > 1 && batch.graph_count().saturating_mul(batch.patterns().len()) > 1)
}

pub(super) fn projection(
    batch: SpinBatchRef<'_>,
    parallelism: usize,
) -> Option<TSpinCoverageMemoryProjection> {
    let mut whole = batch.checked_memory_projection()?;
    if !applies(batch, parallelism) {
        return Some(whole);
    }
    let window = batch
        .with_pattern_range(0, batch.patterns().len().min(PATTERNS_PER_WINDOW))
        .checked_memory_projection()?;
    let tasks = batch
        .graph_count()
        .checked_mul(batch.patterns().len().div_ceil(PATTERNS_PER_WINDOW))?;
    #[cfg(target_family = "wasm")]
    let parallelism = {
        let _ = parallelism;
        1
    };
    let workers = parallelism.max(1).min(tasks.max(1)) as u128;
    let row_words = (whole.pattern_count.div_ceil(64) as u128)
        .checked_mul(8)?
        .checked_mul(whole.graph_count as u128)?;
    let row_headers =
        (whole.graph_count as u128).checked_mul(core::mem::size_of::<GraphWords>() as u128)?;
    whole.word_storage_bytes = whole
        .word_storage_bytes
        .checked_add(window.word_storage_bytes.checked_mul(workers)?)?
        .checked_add(row_words)?
        .checked_add(row_headers)?;
    whole.transition_cache_bytes = window.transition_cache_bytes.checked_mul(workers)?;
    whole.branch_workspace_bytes = window.branch_workspace_bytes.checked_mul(workers)?;
    whole.graph_workspace_bytes = window.graph_workspace_bytes.checked_mul(workers)?;
    whole.required_peak_bytes = whole
        .word_storage_bytes
        .checked_add(whole.candidate_storage_bytes)?
        .checked_add(whole.transition_cache_bytes)?
        .checked_add(whole.branch_workspace_bytes)?
        .checked_add(whole.graph_workspace_bytes)?;
    Some(whole)
}

pub(super) fn materialize(
    batch: SpinBatchRef<'_>,
    target: Option<SpinCoverageTarget>,
    pattern_range: Range<usize>,
    control: &ExecutionControl,
) -> Result<TSpinCoverageOnlyMaterialization, TSpinCoverageMaterializationError> {
    let pattern_count = batch.patterns().len();
    let words = pattern_count.div_ceil(64);
    let start = pattern_range.start.min(pattern_count);
    let end = pattern_range.end.min(pattern_count).max(start);
    let graph_count = batch.graph_count();
    let task_count = graph_count
        .checked_mul(pattern_count.div_ceil(PATTERNS_PER_WINDOW))
        .ok_or(TSpinCoverageMaterializationError::ProjectionOverflow)?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(graph_count)
        .map_err(|_| TSpinCoverageMaterializationError::AllocationFailed)?;
    for index in 0..graph_count {
        let graph = batch
            .graph(index)
            .ok_or(TSpinCoverageMaterializationError::ParallelWorkerFailed)?;
        let mut graph_words = Vec::new();
        graph_words
            .try_reserve_exact(words)
            .map_err(|_| TSpinCoverageMaterializationError::AllocationFailed)?;
        graph_words.resize(words, 0);
        rows.push(GraphWords {
            key: graph.candidate_key(),
            id: graph.candidate_id(),
            words: graph_words,
        });
    }
    let mut covered = Vec::new();
    covered
        .try_reserve_exact(words)
        .map_err(|_| TSpinCoverageMaterializationError::AllocationFailed)?;
    covered.resize(words, 0);
    let output = Mutex::new((rows, covered));
    let complete = AtomicBool::new(batch.complete());
    let completed = AtomicUsize::new(0);
    let window_projection = batch
        .with_pattern_range(0, pattern_count.min(PATTERNS_PER_WINDOW))
        .checked_memory_projection()
        .ok_or(TSpinCoverageMaterializationError::ProjectionOverflow)?;
    try_for_each_chunk(
        control,
        task_count,
        1,
        || {
            Ok::<Option<(usize, SupplyTransitionCache<'_>)>, TSpinCoverageMaterializationError>(
                None,
            )
        },
        |cached, tasks| {
            for task in tasks {
                let window_index = task / graph_count;
                let graph_index = task % graph_count;
                let first = window_index * PATTERNS_PER_WINDOW;
                let last = first.saturating_add(PATTERNS_PER_WINDOW).min(pattern_count);
                let view = batch.with_pattern_range(first, last);
                if cached
                    .as_ref()
                    .is_none_or(|(index, _)| *index != window_index)
                {
                    *cached = Some((
                        window_index,
                        SupplyTransitionCache::new(view, &window_projection),
                    ));
                }
                let cache = &mut cached.as_mut().expect("window cache initialized").1;
                let mut initial = zeroed_words((last - first).div_ceil(64));
                for index in start.max(first)..end.min(last) {
                    set_pattern(&mut initial, index - first);
                }
                let mut accepted = zeroed_words(initial.len());
                let graph = batch
                    .graph(graph_index)
                    .ok_or(TSpinCoverageMaterializationError::ParallelWorkerFailed)?;
                if !evaluate_graph_product(
                    view,
                    graph,
                    &initial,
                    cache,
                    &mut accepted,
                    target,
                    control,
                    window_projection.max_product_states_per_node,
                )
                .map_err(|_| TSpinCoverageMaterializationError::Cancelled)?
                {
                    complete.store(false, Ordering::Relaxed);
                }
                // Only sixteen words are merged per receipt. Original global
                // bit positions never change, including a partial final word.
                {
                    let mut output = output
                        .lock()
                        .map_err(|_| TSpinCoverageMaterializationError::ParallelWorkerFailed)?;
                    let (rows, covered) = &mut *output;
                    let offset = first / 64;
                    for (index, value) in accepted.into_iter().enumerate() {
                        rows[graph_index].words[offset + index] |= value;
                        covered[offset + index] |= value;
                    }
                }
                let done = completed.fetch_add(1, Ordering::AcqRel) + 1;
                control.report_progress("execution-coverage", done as u64, Some(task_count as u64));
            }
            Ok(())
        },
    )
    .map_err(|error| match error {
        ParallelWorkError::Worker(error) => error,
        ParallelWorkError::Cancelled => TSpinCoverageMaterializationError::Cancelled,
        ParallelWorkError::ThreadSpawnFailed => TSpinCoverageMaterializationError::AllocationFailed,
        ParallelWorkError::WorkerPanicked => {
            TSpinCoverageMaterializationError::ParallelWorkerFailed
        }
    })?;
    let (mut rows, covered_words) = output
        .into_inner()
        .map_err(|_| TSpinCoverageMaterializationError::ParallelWorkerFailed)?;
    rows.sort_unstable_by(|left, right| left.key.cmp(&right.key));
    let mut candidate_coverages: Vec<CandidatePatternCoverage> = Vec::new();
    candidate_coverages
        .try_reserve_exact(rows.len())
        .map_err(|_| TSpinCoverageMaterializationError::AllocationFailed)?;
    for row in rows {
        if control.is_cancelled() {
            return Err(TSpinCoverageMaterializationError::Cancelled);
        }
        if !words_are_nonempty(&row.words) {
            continue;
        }
        let coverage = PatternBitSet::from_words(pattern_count, row.words)
            .map_err(|_| TSpinCoverageMaterializationError::ParallelWorkerFailed)?;
        if let Some(previous) = candidate_coverages
            .last_mut()
            .filter(|previous| previous.candidate_key == row.key)
        {
            previous.candidate_id = previous.candidate_id.min(row.id);
            previous
                .covered_patterns
                .union_with(&coverage)
                .map_err(|_| TSpinCoverageMaterializationError::ParallelWorkerFailed)?;
        } else {
            candidate_coverages.push(CandidatePatternCoverage {
                candidate_id: row.id,
                candidate_key: row.key,
                covered_patterns: coverage,
            });
        }
    }
    let mut candidate_ids: Vec<_> = candidate_coverages
        .iter()
        .map(|row| row.candidate_id)
        .collect();
    candidate_ids.sort_unstable();
    candidate_ids.dedup();
    let candidate_keys = candidate_coverages
        .iter()
        .map(|row| row.candidate_key.clone())
        .collect();
    let witnessed_pattern_count = candidate_coverages
        .iter()
        .map(|row| u128::from(row.covered_patterns.count_ones()))
        .sum();
    Ok(TSpinCoverageOnlyMaterialization {
        covered_patterns: PatternBitSet::from_words(pattern_count, covered_words)
            .map_err(|_| TSpinCoverageMaterializationError::ParallelWorkerFailed)?,
        candidate_ids,
        candidate_keys,
        candidate_coverages,
        witnessed_pattern_count,
        complete: complete.load(Ordering::Relaxed),
    })
}
