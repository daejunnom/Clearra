use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{mpsc, Arc},
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_problem::SearchProblem;

use crate::{cpu_worker_pool, solution_probability::SolutionCoverage, CorePathStep};

use super::{
    buildup::{verify_candidate, BuildUpWorkspace},
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    geometry::{GeometrySearch, ParallelGeometryPlan, TargetGroup},
    mix_digest,
    parallel_coverage::SharedCoverage,
    parallel_worker::{
        run_branch_worker, BranchSearchOutcome, ParallelBranchQueue, ParallelBranchTask,
        ParallelWorkerResult, RepresentativeCandidate, WorkerAggregate,
    },
    reachability::ReachabilityMetrics,
    WasmExactSearchError,
};

const MIN_PARALLEL_PIECES: usize = 7;
const MIN_ESTIMATED_PARALLEL_STATES: usize = 4_096;

pub(super) enum ParallelSearchDecision {
    Serial {
        geometry: GeometrySearch,
        reason: &'static str,
    },
    Completed(ParallelSearchOutcome),
}

pub(super) struct ParallelSearchOutcome {
    pub geometry: GeometrySearch,
    pub covered_patterns: PatternBitSet,
    pub buildable_identities: Vec<StandardBoard64TilingIdentity>,
    pub solution_coverage: Vec<SolutionCoverage>,
    pub packing_candidate_count: usize,
    pub packing_candidate_digest: u64,
    pub coverage_row_count: usize,
    pub pattern_verified_execution_count: usize,
    pub build_variant_count: u128,
    pub count_complete: bool,
    pub representative_path: Vec<CorePathStep>,
    pub representative_candidate_id: Option<u64>,
    pub representative_pattern_id: Option<u32>,
    pub peak_build_nodes: usize,
    pub total_build_nodes: usize,
    pub coverage_product_words: usize,
    pub coverage_product_states: usize,
    pub coverage_product_edge_checks: usize,
    pub feasibility_states: usize,
    pub feasibility_rejected_candidates: usize,
    pub peak_reachability_states: usize,
    pub total_reachability_states: usize,
    pub worker_retained_bytes: usize,
    pub piece_language_cache_hits: usize,
    pub piece_language_cache_misses: usize,
    pub standard_bag_cache_hits: usize,
    pub standard_bag_cache_misses: usize,
    pub reachability_metrics: ReachabilityMetrics,
    pub truncated_reason: Option<&'static str>,
    pub workers_used: usize,
    pub active_workers: usize,
    pub minimum_worker_candidates: usize,
    pub maximum_worker_candidates: usize,
}

pub(super) fn execute_if_worthwhile(
    problem: Arc<SearchProblem>,
    catalog: Arc<GeometryCatalog>,
    geometry: GeometrySearch,
    control: ExecutionControl,
    requested_workers: usize,
    cpu_warmup_requested: bool,
) -> Result<ParallelSearchDecision, WasmExactSearchError> {
    let target_piece_count = catalog.required_cells().count_ones() as usize / 4;
    let supply_target_count = geometry.parallel_target_count();
    let estimated_states = supply_target_count.saturating_mul(
        catalog.skeleton_count().saturating_mul(
            1usize
                .checked_shl(target_piece_count.min(usize::BITS as usize - 1) as u32)
                .unwrap_or(usize::MAX),
        ),
    );
    let serial_reason = if requested_workers <= 1 {
        Some("single-worker-request")
    } else if target_piece_count < MIN_PARALLEL_PIECES {
        Some("small-piece-count")
    } else if estimated_states < MIN_ESTIMATED_PARALLEL_STATES {
        Some("small-estimated-state-space")
    } else if has_explicit_resource_cap(&problem) {
        Some("explicit-resource-cap-requires-serial-accounting")
    } else {
        None
    };
    if let Some(reason) = serial_reason {
        return Ok(ParallelSearchDecision::Serial { geometry, reason });
    }

    let mut geometry = geometry;
    geometry.compile_for_parallel(&catalog, &control)?;
    let plan = match geometry.into_parallel_plan(requested_workers) {
        Ok(plan) => plan,
        Err(geometry) => {
            return Ok(ParallelSearchDecision::Serial {
                geometry,
                reason: "exact-family-not-splittable",
            });
        }
    };
    execute_plan(
        problem,
        catalog,
        plan,
        control,
        requested_workers,
        cpu_warmup_requested,
    )
    .map(ParallelSearchDecision::Completed)
}

fn has_explicit_resource_cap(problem: &SearchProblem) -> bool {
    problem.budget().max_nodes() != 0
        || problem.backend_request().max_candidates() != 0
        || problem.backend_request().max_frontier_states() != 0
        || problem.backend_request().max_memory_mib().is_some()
}

fn execute_plan(
    problem: Arc<SearchProblem>,
    catalog: Arc<GeometryCatalog>,
    plan: ParallelGeometryPlan,
    control: ExecutionControl,
    requested_workers: usize,
    cpu_warmup_requested: bool,
) -> Result<ParallelSearchOutcome, WasmExactSearchError> {
    let workers_used = requested_workers.min(plan.searches.len()).max(1);
    let pool = if cpu_warmup_requested {
        cpu_worker_pool::prewarm_cpu_workers(workers_used)
    } else {
        cpu_worker_pool::ensure_cpu_workers(workers_used)
    }
    .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_cpu_worker_pool_unavailable"))?;
    let workers_used = pool.total_workers();
    let pattern_count = problem
        .piece_source()
        .materialized_universe()
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_piece_source_not_materialized",
        ))?
        .pattern_count();
    let shared_coverage = Arc::new(SharedCoverage::new(pattern_count));
    let branch_count = plan.searches.len();
    let queue = Arc::new(ParallelBranchQueue::new(branch_tasks(plan.searches)));
    let (sender, receiver) = mpsc::channel();

    for _ in 0..workers_used.saturating_sub(1) {
        let worker_problem = Arc::clone(&problem);
        let worker_catalog = Arc::clone(&catalog);
        let worker_targets = Arc::clone(&plan.targets);
        let worker_control = control.clone();
        let worker_queue = Arc::clone(&queue);
        let worker_coverage = Arc::clone(&shared_coverage);
        let worker_sender = sender.clone();
        cpu_worker_pool::submit_cpu_job(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                run_branch_worker(
                    &worker_problem,
                    &worker_catalog,
                    &worker_targets,
                    &worker_control,
                    &worker_queue,
                    &worker_coverage,
                )
            }))
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_parallel_worker_panicked"))
            .and_then(|result| result);
            if result.is_err() {
                worker_queue.abort();
            }
            let _ = worker_sender.send(result);
        })
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_cpu_worker_pool_unavailable"))?;
    }
    drop(sender);

    let caller_result = run_branch_worker(
        &problem,
        &catalog,
        &plan.targets,
        &control,
        &queue,
        &shared_coverage,
    );
    if caller_result.is_err() {
        queue.abort();
    }
    let mut results = Vec::with_capacity(workers_used);
    results.push(caller_result);
    for _ in 0..workers_used.saturating_sub(1) {
        results.push(
            receiver
                .recv()
                .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_parallel_worker_lost"))?,
        );
    }
    let worker_results = results
        .into_iter()
        .collect::<Result<Vec<_>, WasmExactSearchError>>()?;
    merge_results(
        &problem,
        &catalog,
        &plan.targets,
        &control,
        &shared_coverage,
        worker_results,
        plan.group_pattern_index_bytes,
        plan.shared_family_bytes,
        workers_used,
        branch_count,
    )
}

fn branch_tasks(searches: Vec<GeometrySearch>) -> Vec<ParallelBranchTask> {
    searches
        .into_iter()
        .enumerate()
        .map(|(canonical_index, search)| ParallelBranchTask {
            canonical_index,
            priority: search.parallel_priority(),
            search,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn merge_results(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    targets: &[TargetGroup],
    control: &ExecutionControl,
    shared_coverage: &SharedCoverage,
    worker_results: Vec<ParallelWorkerResult>,
    group_pattern_index_bytes: usize,
    shared_family_bytes: usize,
    workers_used: usize,
    expected_branch_count: usize,
) -> Result<ParallelSearchOutcome, WasmExactSearchError> {
    let active_workers = worker_results
        .iter()
        .filter(|worker| worker.candidate_count != 0)
        .count();
    let minimum_worker_candidates = worker_results
        .iter()
        .filter(|worker| worker.candidate_count != 0)
        .map(|worker| worker.candidate_count)
        .min()
        .unwrap_or(0);
    let maximum_worker_candidates = worker_results
        .iter()
        .map(|worker| worker.candidate_count)
        .max()
        .unwrap_or(0);
    let mut branch_outcomes = Vec::new();
    let mut merged = WorkerAggregate {
        count_complete: true,
        ..WorkerAggregate::default()
    };
    for worker in worker_results {
        branch_outcomes.extend(worker.branches);
        merged.merge(worker.aggregate)?;
    }
    branch_outcomes.sort_unstable_by_key(|branch| branch.canonical_index);
    if branch_outcomes.len() != expected_branch_count
        || branch_outcomes
            .iter()
            .enumerate()
            .any(|(index, branch)| index != branch.canonical_index)
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_parallel_branch_result_incomplete",
        ));
    }

    let (packing_candidate_count, packing_candidate_digest) =
        candidate_identity_summary(&branch_outcomes);
    let truncated_reason = branch_outcomes
        .iter()
        .find_map(|branch| branch.truncated_reason);
    if problem.objective().kind()
        == clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling
    {
        merged.buildable_identities.sort_unstable();
        merged.buildable_identities.dedup();
    }
    let representative = merged.representative;
    let (representative_path, representative_pattern_id) = match representative {
        Some(_)
            if problem.objective().kind()
                == clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling =>
        {
            (Vec::new(), None)
        }
        Some(representative) => {
            reverify_representative(problem, catalog, targets, representative, control)?
        }
        None => (Vec::new(), None),
    };
    let searches = branch_outcomes
        .into_iter()
        .map(|branch| branch.geometry)
        .collect::<Vec<_>>();
    let geometry = GeometrySearch::from_parallel_searches(
        &searches,
        group_pattern_index_bytes,
        shared_family_bytes,
    );

    Ok(ParallelSearchOutcome {
        geometry,
        covered_patterns: shared_coverage.to_bitset()?,
        buildable_identities: merged.buildable_identities,
        solution_coverage: merged
            .solution_coverage
            .into_iter()
            .map(|(identity, coverage)| SolutionCoverage::new(identity, coverage))
            .collect(),
        packing_candidate_count,
        packing_candidate_digest,
        coverage_row_count: merged.coverage_row_count,
        pattern_verified_execution_count: merged.pattern_verified_execution_count,
        build_variant_count: merged.build_variant_count,
        count_complete: merged.count_complete && truncated_reason.is_none(),
        representative_path,
        representative_candidate_id: representative
            .map(|entry| entry.candidate.identity.bucket_hash()),
        representative_pattern_id,
        peak_build_nodes: merged.peak_build_nodes,
        total_build_nodes: merged.total_build_nodes,
        coverage_product_words: merged.coverage_product_words,
        coverage_product_states: merged.coverage_product_states,
        coverage_product_edge_checks: merged.coverage_product_edge_checks,
        feasibility_states: merged.feasibility_states,
        feasibility_rejected_candidates: merged.feasibility_rejected_candidates,
        peak_reachability_states: merged.peak_reachability_states,
        total_reachability_states: merged.total_reachability_states,
        worker_retained_bytes: merged.worker_retained_bytes,
        piece_language_cache_hits: merged.piece_language_cache_hits,
        piece_language_cache_misses: merged.piece_language_cache_misses,
        standard_bag_cache_hits: merged.standard_bag_cache_hits,
        standard_bag_cache_misses: merged.standard_bag_cache_misses,
        reachability_metrics: merged.reachability_metrics,
        truncated_reason,
        workers_used,
        active_workers,
        minimum_worker_candidates,
        maximum_worker_candidates,
    })
}

fn candidate_identity_summary(branches: &[BranchSearchOutcome]) -> (usize, u64) {
    let mut count = 0usize;
    let mut digest = 0u64;
    for branch in branches {
        for candidate_hash in &branch.candidate_hashes {
            count = count.saturating_add(1);
            digest = mix_digest(digest, *candidate_hash);
        }
    }
    (count, digest)
}

fn reverify_representative(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    targets: &[TargetGroup],
    representative: RepresentativeCandidate,
    control: &ExecutionControl,
) -> Result<(Vec<CorePathStep>, Option<u32>), WasmExactSearchError> {
    let candidate = representative.candidate;
    let target = targets.get(candidate.target_index as usize).ok_or(
        WasmExactSearchError::InvalidProblem("wasm_geometry_candidate_target_out_of_range"),
    )?;
    let mut workspace = BuildUpWorkspace::default();
    let mut evaluator = CoverageProductEvaluator::default();
    let result = verify_candidate(
        problem,
        catalog,
        &candidate,
        target,
        &mut workspace,
        &mut evaluator,
        false,
        true,
        0,
        control,
    )?;
    Ok((result.representative_path, result.witness_pattern_id))
}
