#![cfg(feature = "parallel")]

use std::sync::{Mutex, MutexGuard};
#[cfg(feature = "local-search-ab")]
use std::{path::Path, sync::Arc, time::Instant};

#[cfg(feature = "local-search-ab")]
use clearra_core_executor::{built_in_legal_board_binding, LegalBoardExpectation};

use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    pc::pc_target::PcTarget,
};
use clearra_core_executor::WasmCpuSearchBackend;
#[cfg(feature = "local-search-ab")]
use clearra_core_executor::{
    install_local_pc4_legal_board_index, set_local_search_prune_policy, LocalPc4LegalBoardIndex,
    LocalSearchPrunePolicy,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
    PcScenarioBoard, PcScenarioQuery, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
    WorkerPolicy,
};
use clearra_problem::ProblemCompiler;
#[cfg(feature = "local-search-ab")]
use clearra_rules::kicks::KickTableProfileId;

const P7P4_UNIQUE_TILING_COUNT: usize = 456_923;
const P7P4_NORMALIZED_SET_HASH: &str = "cts1:98ebe8726537b29f";

// These parity cases share the process-global execution resource authority.
// Serialize only this integration-test family so test-harness concurrency
// cannot masquerade as product-level resource contention.
static PARALLEL_EXACT_TEST_LOCK: Mutex<()> = Mutex::new(());

fn parallel_exact_test_guard() -> MutexGuard<'static, ()> {
    PARALLEL_EXACT_TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn five_piece_unique_uses_work_based_parallelism_with_canonical_parity() {
    let _resource_guard = parallel_exact_test_guard();
    let base = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false);
    let serial = execute_five_piece_unique(base.clone().with_workers(1));
    let fixed = execute_five_piece_unique(
        base.clone()
            .with_workers(2)
            .with_worker_hardware_limit(2)
            .with_use_all_logical_processors(true),
    );
    let auto = execute_five_piece_unique(
        base.with_worker_policy(WorkerPolicy::Auto)
            .with_automatic_worker_limit(2)
            .with_worker_hardware_limit(2)
            .with_use_all_logical_processors(true),
    );

    for candidate in [&fixed, &auto] {
        assert_eq!(
            candidate.normalized_solution_identities(),
            serial.normalized_solution_identities()
        );
        assert_eq!(
            candidate.normalized_solution_keys(),
            serial.normalized_solution_keys()
        );
        assert_eq!(
            candidate.normalized_solution_coverages(),
            serial.normalized_solution_coverages()
        );
        assert_eq!(
            candidate.field("normalized_solution_set_hash"),
            serial.field("normalized_solution_set_hash")
        );
        assert_eq!(candidate.usize_field("workers_used"), Some(2));
        assert_eq!(candidate.bool_field("cpu_parallel_execution"), Some(true));
        assert_eq!(
            candidate.field("cpu_parallel_decision_reason"),
            Some("parallel-immutable-family-queue")
        );
        assert_eq!(candidate.bool_field("count_complete"), Some(true));
        assert_eq!(candidate.bool_field("probability_complete"), Some(true));
    }
}

fn execute_five_piece_unique(
    policy: PcExecutionPolicy,
) -> clearra_core_executor::CoreExecutionResult {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_hold_policy(PcHoldPolicy::Disabled)
        .with_objective(ObjectivePolicy::unique())
        .with_execution_policy(policy);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("five-piece exact problem");
    WasmCpuSearchBackend::execute_with_control(
        &problem,
        &ExecutionControl::new(ExecutionCancellationToken::new()),
    )
    .expect("five-piece exact CPU search")
}

#[test]
fn one_piece_request_reaches_actual_family_gate_and_remains_serial_when_unsplittable() {
    let _resource_guard = parallel_exact_test_guard();
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(2)
        .with_worker_hardware_limit(2)
        .with_use_all_logical_processors(true);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_execution_policy(policy);
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("one-piece exact problem");
    assert!(WasmCpuSearchBackend::distributed_execution_is_worthwhile(
        &problem
    ));
    let result = WasmCpuSearchBackend::execute_with_control(
        &problem,
        &ExecutionControl::new(ExecutionCancellationToken::new()),
    )
    .expect("one-piece exact CPU search");
    assert_eq!(result.usize_field("workers_used"), Some(1));
    assert_eq!(
        result.field("cpu_parallel_decision_reason"),
        Some("small-compiled-candidate-family")
    );
}

#[test]
fn four_piece_all_uses_work_based_parallelism_with_canonical_parity() {
    let _resource_guard = parallel_exact_test_guard();
    let base = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false);
    let serial = execute_four_piece_all(base.clone().with_workers(1));
    let fixed = execute_four_piece_all(
        base.clone()
            .with_workers(2)
            .with_worker_hardware_limit(2)
            .with_use_all_logical_processors(true),
    );
    let auto = execute_four_piece_all(
        base.with_worker_policy(WorkerPolicy::Auto)
            .with_automatic_worker_limit(2)
            .with_worker_hardware_limit(2)
            .with_use_all_logical_processors(true),
    );

    for candidate in [&fixed, &auto] {
        assert_eq!(
            candidate.normalized_solution_identities(),
            serial.normalized_solution_identities()
        );
        assert_eq!(
            candidate.normalized_solution_keys(),
            serial.normalized_solution_keys()
        );
        assert_eq!(
            candidate.normalized_solution_coverages(),
            serial.normalized_solution_coverages()
        );
        assert_eq!(
            candidate.field("normalized_solution_set_hash"),
            serial.field("normalized_solution_set_hash")
        );
        assert_eq!(candidate.usize_field("workers_used"), Some(2));
        assert_eq!(candidate.bool_field("cpu_parallel_execution"), Some(true));
        assert_eq!(
            candidate.field("cpu_parallel_decision_reason"),
            Some("parallel-immutable-family-queue")
        );
    }
}

fn execute_four_piece_all(policy: PcExecutionPolicy) -> clearra_core_executor::CoreExecutionResult {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0xfc3f_0fc3_f0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(4),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(4))
    .with_supply_window_size(SupplyWindowSize::new(4))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_execution_policy(policy);
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("four-piece exact problem");
    assert!(WasmCpuSearchBackend::distributed_execution_is_worthwhile(
        &problem
    ));
    WasmCpuSearchBackend::execute_with_control(
        &problem,
        &ExecutionControl::new(ExecutionCancellationToken::new()),
    )
    .expect("four-piece exact CPU search")
}

#[test]
fn six_piece_score_minimum_source_uses_work_based_parallelism_with_canonical_parity() {
    let _resource_guard = parallel_exact_test_guard();
    let base = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false);
    let serial = execute_six_piece_score_minimum_source(base.clone().with_workers(1));
    let fixed = execute_six_piece_score_minimum_source(
        base.clone()
            .with_workers(2)
            .with_worker_hardware_limit(2)
            .with_use_all_logical_processors(true),
    );
    let auto = execute_six_piece_score_minimum_source(
        base.with_worker_policy(WorkerPolicy::Auto)
            .with_automatic_worker_limit(2)
            .with_worker_hardware_limit(2)
            .with_use_all_logical_processors(true),
    );

    for candidate in [&fixed, &auto] {
        assert_eq!(
            candidate.normalized_solution_identities(),
            serial.normalized_solution_identities()
        );
        assert_eq!(
            candidate.normalized_solution_keys(),
            serial.normalized_solution_keys()
        );
        assert_eq!(
            candidate.normalized_solution_coverages(),
            serial.normalized_solution_coverages()
        );
        assert_eq!(
            candidate.field("normalized_solution_set_hash"),
            serial.field("normalized_solution_set_hash")
        );
        assert_eq!(candidate.usize_field("workers_used"), Some(2));
        assert_eq!(candidate.bool_field("cpu_parallel_execution"), Some(true));
        assert_eq!(
            candidate.field("cpu_parallel_decision_reason"),
            Some("parallel-immutable-family-queue")
        );
        assert_eq!(candidate.bool_field("count_complete"), Some(true));
        assert_eq!(candidate.bool_field("probability_complete"), Some(true));
    }
}

fn execute_six_piece_score_minimum_source(
    policy: PcExecutionPolicy,
) -> clearra_core_executor::CoreExecutionResult {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0xf03c_0f03_c0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::S,
            PieceKind::J,
            PieceKind::L,
            PieceKind::Z,
        ])),
        PieceWindow::new(6),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(6))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_retained_trace_limit(1)
    .with_objective(ObjectivePolicy::minimum_cover().with_score_summary())
    .with_execution_policy(policy);
    let problem = ProblemCompiler::compile_scenario_pc(&query)
        .expect("six-piece score-minimum problem")
        .with_pc_score_portfolio_v2_evidence();
    assert!(WasmCpuSearchBackend::distributed_execution_is_worthwhile(
        &problem
    ));
    WasmCpuSearchBackend::execute_with_control(
        &problem,
        &ExecutionControl::new(ExecutionCancellationToken::new()),
    )
    .expect("six-piece score-minimum CPU search")
}

#[test]
#[ignore = "full P7P4 parallel exact-set regression"]
fn p7p4_parallel_matches_exact_solution_oracle() {
    let _resource_guard = parallel_exact_test_guard();
    let parallel_workers = WorkerPolicy::default_worker_limit();
    if parallel_workers <= 1 {
        return;
    }
    let parallel = execute_p7p4(parallel_workers, true);

    eprintln!(
        "P7P4 normalized set: count={:?}, hash={:?}, active={:?}, min={:?}, max={:?}",
        parallel.usize_field("normalized_unique_solution_count"),
        parallel.field("normalized_solution_set_hash"),
        parallel.usize_field("parallel_active_workers"),
        parallel.usize_field("parallel_minimum_worker_candidates"),
        parallel.usize_field("parallel_maximum_worker_candidates")
    );

    assert_eq!(
        parallel.usize_field("normalized_unique_solution_count"),
        Some(P7P4_UNIQUE_TILING_COUNT)
    );
    assert_eq!(
        parallel.field("normalized_solution_set_hash"),
        Some(P7P4_NORMALIZED_SET_HASH)
    );
    assert_eq!(parallel.bool_field("count_complete"), Some(true));
    assert_eq!(parallel.bool_field("probability_complete"), Some(true));
    assert_eq!(parallel.bool_field("cpu_parallel_execution"), Some(true));
    assert_eq!(
        parallel.field("cpu_parallel_decision_reason"),
        Some("parallel-immutable-family-queue")
    );
    assert_eq!(parallel.bool_field("cpu_warmup_performed"), Some(true));
    assert_eq!(parallel.usize_field("workers_used"), Some(parallel_workers));
    assert_eq!(
        parallel.usize_field("parallel_active_workers"),
        Some(parallel_workers)
    );
    assert!(parallel
        .usize_field("parallel_minimum_worker_candidates")
        .is_some_and(|candidates| candidates > 0));
}

fn execute_p7p4(workers: usize, warmup: bool) -> clearra_core_executor::CoreExecutionResult {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_workers(workers)
        .with_cpu_warmup(warmup);
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_queue(PcQueueInput::standard_7_bag())
        .with_hold_policy(PcHoldPolicy::EnabledEmpty)
        .with_objective(ObjectivePolicy::unique())
        .with_execution_policy(policy);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("P7P4 problem");
    let cancellation = ExecutionCancellationToken::new();
    WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::new(cancellation))
        .expect("exact CPU search")
}

#[cfg(feature = "local-search-ab")]
#[test]
fn local_prune_ab_modes_preserve_small_exact_set() {
    let _resource_guard = parallel_exact_test_guard();
    let previous = set_local_search_prune_policy(LocalSearchPrunePolicy::product_default());
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(2)
        .with_worker_hardware_limit(2)
        .with_use_all_logical_processors(true);
    let baseline = execute_four_piece_all(policy.clone());
    for additive_parity in [false, true] {
        for apdp in [false, true] {
            for dependency_relaxation in [false, true] {
                set_local_search_prune_policy(LocalSearchPrunePolicy::new(
                    additive_parity,
                    apdp,
                    dependency_relaxation,
                ));
                let candidate = execute_four_piece_all(policy.clone());
                assert_eq!(
                    candidate.normalized_solution_identities(),
                    baseline.normalized_solution_identities(),
                    "identity mismatch for additive={additive_parity} apdp={apdp} dependency={dependency_relaxation}"
                );
                assert_eq!(
                    candidate.field("normalized_solution_set_hash"),
                    baseline.field("normalized_solution_set_hash")
                );
            }
        }
    }
    set_local_search_prune_policy(previous);
}

#[cfg(feature = "local-search-ab")]
#[test]
#[ignore = "finite local Full 4L P7P4 prune matrix benchmark"]
fn p7p4_prune_matrix_two_runs_per_pair() {
    let _resource_guard = parallel_exact_test_guard();
    let previous = set_local_search_prune_policy(LocalSearchPrunePolicy::product_default());
    let workers = WorkerPolicy::default_worker_limit();
    assert!(workers > 1, "P7P4 A/B requires a parallel worker topology");
    let forward = [(false, false), (false, true), (true, false), (true, true)];
    let mut parity_records = Vec::new();
    for repetition in 0..2 {
        let order: Box<dyn Iterator<Item = (bool, bool)>> = if repetition == 0 {
            Box::new(forward.into_iter())
        } else {
            Box::new(forward.into_iter().rev())
        };
        for (additive_parity, apdp) in order {
            set_local_search_prune_policy(LocalSearchPrunePolicy::new(
                additive_parity,
                apdp,
                false,
            ));
            let started = Instant::now();
            let result = execute_p7p4(workers, true);
            let elapsed = started.elapsed();
            assert_p7p4_exact(&result);
            parity_records.push((additive_parity, apdp, elapsed));
            eprintln!(
                "P7P4_PRUNE_AB repetition={} additive_parity={} apdp={} elapsed_ms={} geometry_nodes={:?} domain_pruned={:?} column_pruned={:?} build_nodes={:?}",
                repetition + 1,
                additive_parity,
                apdp,
                elapsed.as_millis(),
                result.usize_field("searched_nodes"),
                result.usize_field("geometry_domain_pruned_states"),
                result.usize_field("geometry_column_pruned_states"),
                result.usize_field("total_build_order_nodes")
            );
        }
    }

    let mut pair_best = forward.map(|pair| (pair, std::time::Duration::MAX));
    for (additive, apdp, elapsed) in parity_records {
        let slot = pair_best
            .iter_mut()
            .find(|(pair, _)| *pair == (additive, apdp))
            .unwrap();
        slot.1 = slot.1.min(elapsed);
    }
    let ((best_additive, best_apdp), _) = *pair_best
        .iter()
        .min_by_key(|(_, elapsed)| *elapsed)
        .unwrap();

    // Load the pre-generated legal-board index only after the parity matrix.
    // This keeps its retained memory and cache footprint out of all eight
    // parity samples while still excluding generation time from the ABBA run.
    let legal_bundle = std::env::var_os("CLEARRA_LOCAL_PC4_LEGAL_BOARD_BUNDLE").expect(
        "set CLEARRA_LOCAL_PC4_LEGAL_BOARD_BUNDLE to the exact SRS+ F-intersection-R bundle",
    );
    install_local_pc4_legal_board_index(load_legal_board_index(Path::new(&legal_bundle))).unwrap();

    let mut legal_records = Vec::new();
    for legal_board in [false, true, true, false] {
        set_local_search_prune_policy(
            LocalSearchPrunePolicy::new(best_additive, best_apdp, false)
                .with_legal_board(legal_board),
        );
        let started = Instant::now();
        let result = execute_p7p4(workers, true);
        let elapsed = started.elapsed();
        assert_p7p4_exact(&result);
        legal_records.push((legal_board, elapsed));
        eprintln!(
            "P7P4_LEGAL_BOARD_ABBA legal_board={} elapsed_ms={}",
            legal_board,
            elapsed.as_millis(),
        );
    }
    let best_legal = [false, true]
        .into_iter()
        .min_by_key(|enabled| {
            legal_records
                .iter()
                .filter(|(candidate, _)| candidate == enabled)
                .map(|(_, elapsed)| *elapsed)
                .min()
                .unwrap()
        })
        .unwrap();

    let mut dependency_records = Vec::new();
    for dependency in [false, true] {
        set_local_search_prune_policy(
            LocalSearchPrunePolicy::new(best_additive, best_apdp, dependency)
                .with_legal_board(best_legal),
        );
        let started = Instant::now();
        let result = execute_p7p4(workers, true);
        let elapsed = started.elapsed();
        assert_p7p4_exact(&result);
        dependency_records.push((dependency, elapsed));
        eprintln!(
            "P7P4_ILC_CYCLE_AB dependency_relaxation={} elapsed_ms={}",
            dependency,
            elapsed.as_millis(),
        );
    }
    let first_a = dependency_records[0].1.as_secs_f64();
    let first_b = dependency_records[1].1.as_secs_f64();
    let relative_difference = (first_a - first_b).abs() / first_a.min(first_b).max(0.001);
    if relative_difference <= 0.05 {
        for dependency in [true, false] {
            set_local_search_prune_policy(
                LocalSearchPrunePolicy::new(best_additive, best_apdp, dependency)
                    .with_legal_board(best_legal),
            );
            let started = Instant::now();
            let result = execute_p7p4(workers, true);
            let elapsed = started.elapsed();
            assert_p7p4_exact(&result);
            eprintln!(
                "P7P4_ILC_CYCLE_ABBA dependency_relaxation={} elapsed_ms={}",
                dependency,
                elapsed.as_millis(),
            );
        }
    } else {
        eprintln!(
            "P7P4_ILC_CYCLE_ABBA skipped=true relative_difference={relative_difference:.6} threshold=0.05"
        );
    }
    set_local_search_prune_policy(previous);
}

#[cfg(feature = "local-search-ab")]
fn assert_p7p4_exact(result: &clearra_core_executor::CoreExecutionResult) {
    assert_eq!(
        result.usize_field("normalized_unique_solution_count"),
        Some(P7P4_UNIQUE_TILING_COUNT)
    );
    assert_eq!(
        result.field("normalized_solution_set_hash"),
        Some(P7P4_NORMALIZED_SET_HASH)
    );
}

#[cfg(feature = "local-search-ab")]
fn load_legal_board_index(path: &Path) -> LocalPc4LegalBoardIndex {
    let bytes = std::fs::read(path).expect("read exact legal-board bundle");
    let binding = built_in_legal_board_binding(KickTableProfileId::SrsPlus).unwrap();
    LocalPc4LegalBoardIndex::load_bundle(
        Arc::from(bytes),
        LegalBoardExpectation {
            binding,
            generation_identity: None,
        },
    )
    .expect("complete SRS+ exact legal-board bundle")
}
