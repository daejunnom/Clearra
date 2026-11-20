use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{mpsc, Arc},
};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_coverage::row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind};
use clearra_problem::SearchProblem;

use crate::{
    cpu_worker_pool, performance::WorkerSearchProfile, solution_probability::SolutionCoverage,
    CorePathStep,
};

#[cfg(all(feature = "local-search-ab", target_os = "linux"))]
use super::parallel_memory_peak::{self, CoordinatorStorage};
#[cfg(any(test, feature = "local-search-ab"))]
use super::result::SolutionCoverageCapture;
use super::{
    buildup::{verify_candidate, BuildUpWorkspace, CandidateWitnessMode},
    catalog::GeometryCatalog,
    coverage_product::CoverageProductEvaluator,
    geometry::{GeometrySearch, ParallelGeometryPlan, ParallelGeometryPreparation, TargetGroup},
    parallel_coverage::SharedCoverage,
    parallel_worker::{
        run_branch_worker, BranchSearchOutcome, BranchWorkerInputs, ParallelBranchQueue,
        ParallelBranchTask, ParallelWorkerResult, RepresentativeCandidate, SharedWorkerRequest,
        WorkerAggregate, WorkerMemoryComponents,
    },
    reachability::{ReachabilityMetrics, SharedReachabilityTemplates},
    standard_bag_coverage::SharedStandardBagRequest,
    WasmExactSearchError,
};

// Worker completion is a one-shot ownership transfer and remains inline deliberately.
#[allow(clippy::large_enum_variant)]
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
    pub failed_queue_rows: Vec<CoverageRow>,
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
    pub candidate_digest_retained_bytes: usize,
    pub worker_memory_components: WorkerMemoryComponents,
    pub standard_bag_memo_storage: &'static str,
    pub piece_language_cache_hits: usize,
    pub piece_language_cache_misses: usize,
    pub standard_bag_cache_hits: usize,
    pub standard_bag_cache_misses: usize,
    pub reachability_metrics: ReachabilityMetrics,
    pub legal_board_verified_negative_prunes: usize,
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
    #[cfg(any(test, feature = "local-search-ab"))] solution_coverage_capture: Option<
        SolutionCoverageCapture,
    >,
) -> Result<ParallelSearchDecision, WasmExactSearchError> {
    let serial_reason = if requested_workers <= 1 {
        Some("single-worker-request")
    } else if has_explicit_resource_cap(&problem) {
        Some("explicit-resource-cap-requires-serial-accounting")
    } else {
        None
    };
    if let Some(reason) = serial_reason {
        return Ok(ParallelSearchDecision::Serial { geometry, reason });
    }

    let (plan, balanced_workers) = match geometry.prepare_parallel(
        &catalog,
        &control,
        requested_workers,
        parallel_worker_count_for_candidates,
    )? {
        ParallelGeometryPreparation::Ready {
            plan,
            balanced_workers,
        } => (plan, balanced_workers),
        ParallelGeometryPreparation::Serial { geometry, reason } => {
            return Ok(ParallelSearchDecision::Serial { geometry, reason });
        }
    };
    execute_plan(
        problem,
        catalog,
        plan,
        control,
        balanced_workers,
        cpu_warmup_requested,
        #[cfg(any(test, feature = "local-search-ab"))]
        solution_coverage_capture,
    )
    .map(ParallelSearchDecision::Completed)
}

fn parallel_worker_count_for_candidates(candidate_count: u128, requested_workers: usize) -> usize {
    let candidate_count = candidate_count.min(usize::MAX as u128) as usize;
    requested_workers.min(candidate_count)
}

#[cfg(test)]
mod tests {
    use super::parallel_worker_count_for_candidates;

    #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
    #[test]
    fn strict_parallel_prefix_p7_matches_exact_rows_and_does_not_escape_scope() {
        use clearra_core_domain::execution_cancellation::ExecutionControl;
        use clearra_pc_graph::request::{
            PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
            PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
        };
        use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
        use clearra_rules::profile::builtin_rules::srs_plus;

        use crate::{backend::with_strict_parallel_prefix_experiment, WasmCpuSearchBackend};

        let make_problem = |explicit_cap: bool, probability_policy| {
            let mut policy = PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_allow_backend_fallback(false)
                .with_workers(2)
                .with_exact_legal_board_enabled(false)
                .with_conditioned_reachability_enabled(false)
                .with_tablebase_requested(false);
            if explicit_cap {
                policy = policy.with_max_memory_mib(Some(256));
            }
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0x3c0f_03c0f),
                PcQueueInput::standard_7_bag(),
                PieceWindow::new(6),
            )
            .with_rule(srs_plus())
            .with_allow_hold(true)
            .with_exact_pieces(Some(6))
            .with_supply_window_size(SupplyWindowSize::new(7))
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_solution_probability_policy(probability_policy)
            .with_retained_trace_limit(0)
            .with_execution_policy(policy);
            ProblemCompiler::compile_scenario_pc(&query)
                .expect("exact native P7")
                .with_output_policy(SearchOutputPolicy::CoverageSummary)
        };

        let problem = make_problem(false, PcSolutionProbabilityPolicy::Include);
        let control = ExecutionControl::default();
        let run = || WasmCpuSearchBackend::execute_with_control(&problem, &control);

        let baseline = run().expect("native parallel baseline");
        assert_eq!(baseline.bool_field("cpu_parallel_execution"), Some(true));
        assert_eq!(baseline.usize_field("workers_used"), Some(2));
        assert_eq!(baseline.usize_field("packing_candidate_count"), Some(2260));
        assert_eq!(baseline.solution_coverages().len(), 246);
        assert!(baseline.usize_field("total_build_order_nodes").unwrap() > 0);

        let (strict, report) = with_strict_parallel_prefix_experiment(run);
        let strict = strict.expect("strict native parallel P7");
        assert!(report.sealed, "{report:?}");
        assert_eq!(report.workers, 2);
        assert_eq!(report.executing_workers, 2);
        assert_eq!(report.candidates, 2260);
        assert!(report.reverse_relations > 0);
        assert_eq!(strict.bool_field("cpu_parallel_execution"), Some(true));
        assert_eq!(strict.usize_field("workers_used"), Some(report.workers));
        assert_eq!(
            strict.usize_field("parallel_active_workers"),
            Some(report.executing_workers),
        );
        assert_eq!(
            strict.usize_field("packing_candidate_count"),
            Some(report.candidates),
        );
        assert_eq!(strict.usize_field("total_build_order_nodes"), Some(0));
        assert_eq!(strict.usize_field("total_reachability_states"), Some(0));
        assert_eq!(strict.solution_coverages(), baseline.solution_coverages());
        assert_eq!(
            strict.coverage_pattern_words(),
            baseline.coverage_pattern_words()
        );
        assert_eq!(strict.usize_field("coverage_pattern_count"), Some(5040));
        assert_eq!(strict.usize_field("covered_pattern_count"), Some(5040));

        let warm_reference = run().expect("concrete scope restored");
        assert_eq!(
            warm_reference.solution_coverages(),
            baseline.solution_coverages()
        );
        assert!(
            warm_reference
                .usize_field("total_build_order_nodes")
                .unwrap()
                > 0
        );

        let (refused, refused_report) = with_strict_parallel_prefix_experiment(|| {
            WasmCpuSearchBackend::execute_with_control(
                &make_problem(true, PcSolutionProbabilityPolicy::Include),
                &control,
            )
        });
        let error = refused.expect_err("solver cap cannot silently fall back to serial");
        assert_eq!(
            error.reason(),
            "symbolic_prefix_parallel_plan_did_not_admit"
        );
        assert!(!refused_report.sealed);
        assert_eq!(refused_report.workers, 0);

        use crate::backend::CapturedSolutionCoverage;
        use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
        use std::{
            collections::BTreeMap,
            sync::{Arc, Mutex},
        };

        let capture_problem = make_problem(false, PcSolutionProbabilityPolicy::Omit);
        let capture_arm = || {
            let rows = Arc::new(Mutex::new(BTreeMap::new()));
            let sink_rows = Arc::clone(&rows);
            let result = WasmCpuSearchBackend::execute_with_solution_coverage_capture(
                &capture_problem,
                &control,
                move |identity, fragment| {
                    let CapturedSolutionCoverage::Bitmap(bits) = fragment else {
                        return Err("p7_requires_an_explicit_bitmap");
                    };
                    sink_rows
                        .lock()
                        .map_err(|_| "p7_sink_poisoned")?
                        .entry(identity)
                        .or_insert_with(|| PatternBitSet::new(bits.pattern_count()))
                        .union_with(bits)
                        .map_err(|_| "p7_sink_universe_mismatch")
                },
            );
            (result, rows)
        };
        let (reference_capture, reference_rows) = capture_arm();
        let reference_capture = reference_capture.expect("ordinary no-replay capture");
        let ((strict_capture, strict_rows), capture_report) =
            with_strict_parallel_prefix_experiment(capture_arm);
        let strict_capture = strict_capture.expect("strict no-replay capture");
        assert!(capture_report.sealed, "{capture_report:?}");
        assert_eq!(capture_report.candidates, report.candidates);
        assert_eq!(capture_report.executing_workers, 2);
        assert_eq!(
            strict_capture.usize_field("total_build_order_nodes"),
            Some(0)
        );
        assert_eq!(
            strict_capture.usize_field("total_reachability_states"),
            Some(0)
        );
        let identities = baseline
            .solution_coverages()
            .iter()
            .map(|row| row.identity())
            .collect::<Vec<_>>();
        for terminal in [&reference_capture, &strict_capture] {
            assert_eq!(terminal.normalized_solution_identities(), identities);
            assert!(terminal.path_steps().is_empty());
            assert!(terminal.solution_coverages().is_empty());
            assert_eq!(
                terminal.coverage_pattern_words(),
                baseline.coverage_pattern_words()
            );
        }
        let reference_rows = reference_rows.lock().unwrap();
        let strict_rows = strict_rows.lock().unwrap();
        assert_eq!(*reference_rows, *strict_rows);
        assert_eq!(strict_rows.len(), identities.len());
        for row in baseline.solution_coverages() {
            assert_eq!(
                strict_rows.get(&row.identity()),
                Some(row.covered_patterns())
            );
        }
        let (failed_capture, failed_report) = with_strict_parallel_prefix_experiment(|| {
            WasmCpuSearchBackend::execute_with_solution_coverage_capture(
                &capture_problem,
                &control,
                |_, _| Err("p7_intentional_sink_failure"),
            )
        });
        assert!(failed_capture.is_err());
        assert!(!failed_report.sealed);
        eprintln!(
            "STRICT_PARALLEL_P7 workers=2 candidates=2260 identities=246 original_queues=5040"
        );
    }

    #[test]
    fn actual_candidate_work_clamps_only_workers_that_cannot_receive_a_branch() {
        assert_eq!(parallel_worker_count_for_candidates(40, 7), 7);
        assert_eq!(parallel_worker_count_for_candidates(16, 7), 7);
        assert_eq!(parallel_worker_count_for_candidates(2, 7), 2);
        assert_eq!(parallel_worker_count_for_candidates(1, 7), 1);
        assert_eq!(parallel_worker_count_for_candidates(u128::MAX, 7), 7);
    }

    #[cfg(feature = "local-search-ab")]
    #[test]
    fn operation_mask_scope_binds_every_pooled_worker_without_leaking_between_arms() {
        use clearra_core_domain::execution_cancellation::ExecutionControl;
        use clearra_pc_graph::request::{
            PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
            PcSolutionProbabilityPolicy, PieceWindow, SupplyWindowSize,
        };
        use clearra_problem::ProblemCompiler;

        use super::super::{
            buildup::with_operation_mask_experiment, result::WasmExactSearchSession,
        };

        let problem = ProblemCompiler::compile_scenario_pc(
            &PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0xfc3f_0fc3_f0),
                PcQueueInput::standard_7_bag(),
                PieceWindow::new(4),
            )
            .with_allow_hold(false)
            .with_exact_pieces(Some(4))
            .with_supply_window_size(SupplyWindowSize::new(4))
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
            .with_execution_policy(PcExecutionPolicy::default().with_workers(2)),
        )
        .expect("mixed 840-queue fixture");
        let execute = || {
            WasmExactSearchSession::new(&problem)
                .expect("exact session")
                .execute_parallel_if_worthwhile(2, &ExecutionControl::default())
                .expect("parallel search completes")
                .expect("the fixture has parallel work")
        };
        let (control, before) = with_operation_mask_experiment(false, execute);
        let (treatment, masks) = with_operation_mask_experiment(true, execute);
        let (repeated, after) = with_operation_mask_experiment(false, execute);
        for (summary, report) in [(&control, before), (&treatment, masks), (&repeated, after)] {
            assert_eq!(summary.bool_field("cpu_parallel_execution"), Some(true));
            assert_eq!(summary.bool_field("resource_truncated"), Some(false));
            assert_eq!(summary.usize_field("workers_used"), Some(2));
            assert_eq!(summary.usize_field("coverage_pattern_count"), Some(840));
            assert_eq!(summary.usize_field("covered_pattern_count"), Some(173));
            assert_eq!(summary.normalized_solution_identities().len(), 135);
            assert_eq!(
                summary.normalized_solution_identities(),
                control.normalized_solution_identities()
            );
            assert_eq!(summary.solution_coverages(), control.solution_coverages());
            assert_eq!(
                summary.coverage_pattern_words(),
                control.coverage_pattern_words()
            );
            assert_eq!(summary.path_steps(), control.path_steps());
            assert_eq!(
                report.workers, 2,
                "caller and pooled worker must each report once"
            );
            assert!(report.executing_workers > 0);
            assert_eq!(
                summary.usize_field("total_build_order_nodes"),
                Some(report.visited_subsets)
            );
            assert_eq!(
                summary.usize_field("worker_graph_projection_retained_bytes"),
                Some(report.graph_projection_bytes)
            );
            assert_eq!(report.visited_subsets, before.visited_subsets);
            assert_eq!(report.selected_edges, before.selected_edges);
        }
        for graph in [before, after] {
            assert_eq!(graph.requested_workers, 0);
            assert_eq!(graph.mask_passes, 0);
            assert_eq!(graph.mask_bytes, 0);
            assert!(graph.graph_node_bytes > 0);
            assert_eq!(graph.graph_passes, before.graph_passes);
        }
        assert_eq!(masks.requested_workers, 2);
        assert!(masks.mask_live_passes > 0);
        assert_eq!(masks.mask_passes, before.graph_passes);
        assert!(masks.mask_bytes > 0);
        assert_eq!(masks.graph_node_bytes, 0);
        assert_eq!(masks.graph_edge_bytes, 0);
        assert_eq!(masks.graph_piece_edge_bytes, 0);
        assert!(!treatment.path_steps().is_empty());
    }
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
    #[cfg(any(test, feature = "local-search-ab"))] solution_coverage_capture: Option<
        SolutionCoverageCapture,
    >,
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
    // Lazy immutable request tables are shared; language/decision IDs, mutable
    // memo/frontier and graph scratch remain private to each worker.
    let shared_standard_bag = (problem.objective().kind()
        != clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling)
        .then(|| Arc::new(SharedStandardBagRequest::default()));
    #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
    let strict_prefix = super::symbolic_prefix_parallel::claim_strict_parallel_prefix()?;
    let shared_request = SharedWorkerRequest {
        standard_bag: shared_standard_bag,
        reachability: Arc::new(SharedReachabilityTemplates::new(
            catalog.width(),
            catalog.height(),
            problem.kick_profile().profile_id(),
        )),
        #[cfg(any(test, feature = "local-search-ab"))]
        solution_coverage_capture,
        #[cfg(feature = "local-search-ab")]
        operation_masks: super::buildup::claim_operation_mask_experiment()?,
        #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
        strict_prefix,
    };
    #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
    if shared_request.strict_prefix.is_some()
        && (shared_request.operation_masks.is_some()
            || problem.output_policy() != clearra_problem::SearchOutputPolicy::CoverageSummary)
    {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_strict_parallel_prefix_requires_isolated_coverage_summary",
        ));
    }
    let branch_count = plan.searches.len();
    let inputs = Arc::new(BranchWorkerInputs {
        problem,
        catalog,
        targets: plan.targets,
        control,
        queue: ParallelBranchQueue::new(branch_tasks(plan.searches)),
        coverage: SharedCoverage::new(pattern_count),
        request: shared_request,
    });
    let (sender, receiver) = mpsc::channel();

    #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
    let memory_probe = parallel_memory_peak::current();
    #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
    if let Some(probe) = &memory_probe {
        probe.begin_verify(
            workers_used,
            inputs.catalog.retained_bytes(),
            CoordinatorStorage::Verifying {
                target_index_bytes: plan.group_pattern_index_bytes,
                solution_family_bytes: plan.shared_family_bytes,
                global_coverage_words_bytes: pattern_count.div_ceil(64)
                    * core::mem::size_of::<u64>(),
            },
        );
    }

    for _worker_index in 1..workers_used {
        let worker_inputs = Arc::clone(&inputs);
        let worker_sender = sender.clone();
        #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
        let worker_memory = memory_probe.clone();
        let profile = WorkerSearchProfile::capture();
        cpu_worker_pool::submit_cpu_job(move || {
            let result = catch_unwind(AssertUnwindSafe(|| {
                profile.measure(|| {
                    run_branch_worker(
                        &worker_inputs,
                        #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
                        worker_memory.as_deref().map(|probe| (probe, _worker_index)),
                    )
                })
            }))
            .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_parallel_worker_panicked"))
            .and_then(|result| result);
            if result.is_err() {
                worker_inputs.queue.abort();
            }
            let _ = worker_sender.send(result);
        })
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_cpu_worker_pool_unavailable"))?;
    }
    drop(sender);

    let caller_result = run_branch_worker(
        &inputs,
        #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
        memory_probe.as_deref().map(|probe| (probe, 0)),
    );
    if caller_result.is_err() {
        inputs.queue.abort();
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
    #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
    if let Some(probe) = &memory_probe {
        probe.coordinator(CoordinatorStorage::Merging);
    }
    let mut outcome = merge_results(
        &inputs.problem,
        &inputs.catalog,
        &inputs.targets,
        &inputs.control,
        &inputs.coverage,
        &inputs.request,
        worker_results,
        plan.group_pattern_index_bytes,
        plan.shared_family_bytes,
        workers_used,
        branch_count,
    )?;
    #[cfg(all(feature = "local-search-ab", not(target_family = "wasm")))]
    if let Some(request) = &inputs.request.strict_prefix {
        if inputs.control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        if !outcome.count_complete
            || outcome.truncated_reason.is_some()
            || !outcome.representative_path.is_empty()
        {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_strict_parallel_prefix_terminal_incomplete",
            ));
        }
        request.seal(outcome.workers_used, outcome.packing_candidate_count)?;
    }
    // The execution owner is authoritative for the single immutable payload;
    // no worker-count multiplier and no sum of overlapping exit snapshots.
    outcome
        .worker_memory_components
        .shared_standard_bag_request_bytes = inputs
        .request
        .standard_bag
        .as_ref()
        .map_or(0, |request| request.retained_bytes());
    outcome
        .worker_memory_components
        .shared_reachability_template_bytes = inputs.request.reachability.retained_bytes();
    #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
    if let Some(probe) = &memory_probe {
        probe.coordinator(CoordinatorStorage::Finalizing);
    }
    Ok(outcome)
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
    shared_request: &SharedWorkerRequest,
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

    let (packing_candidate_count, packing_candidate_digest) = candidate_identity_summary(
        &branch_outcomes,
        super::uses_order_independent_pc_candidate_digest(problem),
    )?;
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
        Some(representative) => reverify_representative(
            problem,
            catalog,
            targets,
            representative,
            control,
            shared_request,
        )?,
        None => (Vec::new(), None),
    };
    let failed_queue_rows =
        collect_failed_queue_rows(problem, &mut branch_outcomes, merged.coverage_row_count)?;
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
        failed_queue_rows,
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
        candidate_digest_retained_bytes: merged.candidate_digest_retained_bytes,
        worker_memory_components: merged.worker_memory_components,
        standard_bag_memo_storage: merged.standard_bag_memo_storage,
        piece_language_cache_hits: merged.piece_language_cache_hits,
        piece_language_cache_misses: merged.piece_language_cache_misses,
        standard_bag_cache_hits: merged.standard_bag_cache_hits,
        standard_bag_cache_misses: merged.standard_bag_cache_misses,
        reachability_metrics: merged.reachability_metrics,
        legal_board_verified_negative_prunes: merged.legal_board_verified_negative_prunes,
        truncated_reason,
        workers_used,
        active_workers,
        minimum_worker_candidates,
        maximum_worker_candidates,
    })
}

fn collect_failed_queue_rows(
    problem: &SearchProblem,
    branches: &mut [BranchSearchOutcome],
    expected_rows: usize,
) -> Result<Vec<CoverageRow>, WasmExactSearchError> {
    if problem
        .pc_chance_evidence_policy()
        .pc_failed_queue_example_limit()
        .is_none()
    {
        return Ok(Vec::new());
    }
    let source = problem.piece_source();
    let universe = source
        .materialized_universe()
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_piece_source_not_materialized",
        ))?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(expected_rows).map_err(|_| {
        WasmExactSearchError::InvalidProblem("wasm_parallel_failed_queue_row_storage_unavailable")
    })?;
    let mut prefix = 0usize;
    for branch in branches {
        for (ordinal, coverage) in std::mem::take(&mut branch.failed_queue_rows) {
            let id = (ordinal < branch.candidate_count)
                .then(|| prefix.checked_add(ordinal)?.checked_add(1))
                .flatten()
                .and_then(|id| u64::try_from(id).ok())
                .ok_or(WasmExactSearchError::InvalidProblem(
                    "wasm_parallel_failed_queue_candidate_ordinal_invalid",
                ))?;
            rows.push(CoverageRow::new_with_piece_source(
                id,
                CoverageRowKind::Build,
                source.id().get(),
                universe.pattern_universe_id(),
                universe.pattern_weight_model_id(),
                coverage,
            ));
        }
        prefix = prefix.checked_add(branch.candidate_count).ok_or(
            WasmExactSearchError::InvalidProblem(
                "wasm_parallel_failed_queue_candidate_count_overflow",
            ),
        )?;
    }
    if rows.len() != expected_rows {
        return Err(WasmExactSearchError::InvalidProblem(
            "wasm_parallel_failed_queue_coverage_cardinality_mismatch",
        ));
    }
    Ok(rows)
}

fn candidate_identity_summary(
    branches: &[BranchSearchOutcome],
    order_independent: bool,
) -> Result<(usize, u64), WasmExactSearchError> {
    let mut count = 0usize;
    let mut digest = 0u64;
    for branch in branches {
        count = count.saturating_add(branch.candidate_count);
        digest = branch
            .candidate_digest
            .fold_into(digest, order_independent)?;
    }
    Ok((count, digest))
}

fn reverify_representative(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    targets: &[TargetGroup],
    representative: RepresentativeCandidate,
    control: &ExecutionControl,
    shared_request: &SharedWorkerRequest,
) -> Result<(Vec<CorePathStep>, Option<u32>), WasmExactSearchError> {
    let candidate = representative.candidate;
    let target = targets.get(candidate.target_index as usize).ok_or(
        WasmExactSearchError::InvalidProblem("wasm_geometry_candidate_target_out_of_range"),
    )?;
    let mut workspace = BuildUpWorkspace::default();
    workspace.set_shared_reachability_templates(Arc::clone(&shared_request.reachability))?;
    if let Some(supply) = &shared_request.standard_bag {
        workspace.set_shared_standard_bag_request(Arc::clone(supply))?;
    }
    let mut evaluator = CoverageProductEvaluator::default();
    let result = verify_candidate(
        problem,
        catalog,
        &candidate,
        target,
        &mut workspace,
        &mut evaluator,
        CandidateWitnessMode::Disabled,
        true,
        0,
        control,
    )?;
    Ok((result.representative_path, result.witness_pattern_id))
}
