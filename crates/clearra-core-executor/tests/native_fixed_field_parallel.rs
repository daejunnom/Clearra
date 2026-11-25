#![cfg(all(feature = "parallel", not(target_family = "wasm")))]

use std::sync::Mutex;

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{
    CoreExecutionResult, WasmBuildProbabilityBackend, WasmCpuSearchBackend,
};
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
    FinessePatternKnowledge, ProblemCompiler,
};

// Each request retains the same physical process resource authority. Test
// harness concurrency must not turn parity into unrelated request contention.
static REQUESTS: Mutex<()> = Mutex::new(());

fn policy(workers: usize) -> PcExecutionPolicy {
    PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_worker_hardware_limit(8)
        .with_use_all_logical_processors(true)
        .with_workers(workers)
}

fn source(height: u8, initial: u64, workers: usize) -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(u16::from(height), initial),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(4),
    )
    .with_exact_pieces(Some(4))
    .with_supply_window_size(SupplyWindowSize::new(4))
    .with_allow_hold(false)
    .with_count_policy(PcCountPolicy::CountAll)
    .with_objective(ObjectivePolicy::all())
    .with_execution_policy(policy(workers))
}

fn same_family(actual: &CoreExecutionResult, expected: &CoreExecutionResult) {
    assert_eq!(
        actual.normalized_solution_keys(),
        expected.normalized_solution_keys()
    );
    assert_eq!(
        actual.normalized_solution_identities(),
        expected.normalized_solution_identities()
    );
    assert_eq!(
        actual.normalized_solution_coverages(),
        expected.normalized_solution_coverages()
    );
    assert_eq!(
        actual.coverage_pattern_words(),
        expected.coverage_pattern_words()
    );
    for field in [
        "normalized_solution_set_hash",
        "coverage_probability",
        "covered_pattern_count",
        "coverage_pattern_count",
        "unique_solution_count",
        "count_complete",
        "probability_complete",
    ] {
        assert_eq!(actual.field(field), expected.field(field), "{field}");
    }
    assert_eq!(actual.bool_field("count_complete"), Some(true));
    assert_ne!(actual.bool_field("resource_truncated"), Some(true));
}

#[test]
fn compact_and_extended_build_keep_every_source_member_at_one_two_four_eight_workers() {
    let _request = REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // A fixed 8x2 target admits several piece multisets and physical tilings.
    // Height 8 exercises the same four-word Build producer used above 6L.
    let target = 0xff_u64 | (0xff_u64 << 10);
    for height in [4, 8] {
        for aggregation in [
            BuildProbabilityAggregation::Buildability,
            BuildProbabilityAggregation::spin_search(SpinProfileSelection::TSpins),
        ] {
            let mut baseline = None;
            for workers in [1, 2, 4, 8] {
                let problem =
                    ProblemCompiler::compile_scenario_pc(&source(height, 0, workers)).unwrap();
                let field = BuildProbabilityField::from_words_preserving_height(
                    height,
                    [0; 4],
                    [target, 0, 0, 0],
                )
                .unwrap_or_else(|error| panic!("height={height}, aggregation={aggregation:?}, workers={workers}: {error:?}"));
                let result = WasmBuildProbabilityBackend::execute_with_control(
                    &problem,
                    field,
                    aggregation,
                    BuildProbabilityFinesseRequest::Off,
                    &ExecutionControl::default().with_parallelism(workers),
                )
                .unwrap_or_else(|error| panic!("height={height}, aggregation={aggregation:?}, workers={workers}: {error:?}"));
                assert!(!result.normalized_solution_keys().is_empty());
                if let Some(expected) = &baseline {
                    same_family(&result, expected);
                    assert_eq!(
                        result.exact_scoring_execution_batches(),
                        expected.exact_scoring_execution_batches()
                    );
                    let actual = result.spin_coverage_execution_batches();
                    let expected = expected.spin_coverage_execution_batches();
                    assert_eq!(
                        actual.len(),
                        expected.len(),
                        "height={height}, aggregation={aggregation:?}, workers={workers}"
                    );
                    for (batch_index, (actual, expected)) in actual.iter().zip(expected).enumerate()
                    {
                        assert_eq!(actual.graphs().len(), expected.graphs().len());
                        for (graph_index, (actual, expected)) in
                            actual.graphs().iter().zip(expected.graphs()).enumerate()
                        {
                            assert!(actual == expected, "height={height}, aggregation={aggregation:?}, workers={workers}, batch={batch_index}, graph={graph_index}, actual_id={}, expected_id={}", actual.candidate_id(), expected.candidate_id());
                        }
                        assert!(actual == expected, "batch header mismatch: height={height}, aggregation={aggregation:?}, workers={workers}, batch={batch_index}");
                    }
                } else {
                    baseline = Some(result);
                }
            }
        }
    }
}

#[test]
fn compact_pc_rich_graphs_and_supply_proof_preserve_original_pattern_ordinals() {
    let _request = REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let initial = 0x300_u64 | (0x300_u64 << 10);
    let mut baseline = None;
    for workers in [1, 2, 4, 8] {
        let problem = ProblemCompiler::compile_scenario_pc(&source(2, initial, workers))
            .unwrap()
            .with_pc_path_v2_evidence();
        let result = WasmCpuSearchBackend::execute_with_control(
            &problem,
            &ExecutionControl::default().with_parallelism(workers),
        )
        .unwrap();
        let proof = result
            .pc_path_supply_evidence()
            .expect("executed compact source proof");
        assert!(proof.matches_source(&problem, result.exact_scoring_execution_batches()));
        let batch = &result.exact_scoring_execution_batches()[0];
        assert!(!batch.graphs().is_empty());
        let mut rejected = 0;
        for graph in batch.graphs() {
            let patterns = proof.patterns_for_identity(graph.identity()).unwrap();
            assert_eq!(patterns.pattern_count(), batch.patterns().len());
            let mut required = [0_u8; 7];
            let identity = graph.identity();
            for index in 0..identity.placement_count() {
                required[identity.placement(index).unwrap().piece() as usize] += 1;
            }
            for (index, queue) in batch.patterns().iter().enumerate() {
                let mut supplied = [0_u8; 7];
                for piece in queue {
                    supplied[*piece as usize] += 1;
                }
                let included = patterns.word_at(index / 64) & (1_u64 << (index % 64)) != 0;
                assert_eq!(included, supplied == required, "supply ordinal {index}");
            }
            rejected += batch.patterns().len() - patterns.count_ones() as usize;
        }
        assert!(
            rejected > 0,
            "necessary multiset membership removes impossible cells"
        );
        let replaced = result
            .clone()
            .with_exact_scoring_execution_batches(batch_reversed(&result));
        assert!(
            replaced.pc_path_supply_evidence().is_none(),
            "public graph mutation drops authority"
        );
        if let Some(expected) = &baseline {
            same_family(&result, expected);
            assert_eq!(
                result.exact_scoring_execution_batches(),
                expected.exact_scoring_execution_batches()
            );
        } else {
            baseline = Some(result);
        }
    }
}

fn batch_reversed(result: &CoreExecutionResult) -> Vec<clearra_replay::ExactScoringExecutionBatch> {
    let mut batches = result.exact_scoring_execution_batches().to_vec();
    batches.reverse();
    batches
}

#[test]
fn compact_and_full_height_finesse_reports_preserve_source_order_at_all_widths() {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_supply::queue::fixed_sequence::FixedSequence;
    let _request = REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for height in [4, 8] {
        let mut baseline = None;
        for workers in [1, 2, 4, 8] {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(height, 0),
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O, PieceKind::O])),
                PieceWindow::new(2),
            )
            .with_exact_pieces(Some(2))
            .with_execution_policy(policy(workers));
            let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
            let field = BuildProbabilityField::from_words_preserving_height(
                height as u8,
                [0; 4],
                [0xf | (0xf << 10), 0, 0, 0],
            )
            .unwrap();
            let result = WasmBuildProbabilityBackend::execute_with_control(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
                BuildProbabilityFinesseRequest::Search {
                    pattern_knowledge: FinessePatternKnowledge::Both,
                },
                &ExecutionControl::default().with_parallelism(workers),
            )
            .unwrap();
            let report = result.finesse_report().expect("computed finesse report");
            assert!(report.complete());
            if let Some(expected) = &baseline {
                assert_eq!(report, expected);
            } else {
                baseline = Some(report.clone());
            }
        }
    }
}

#[test]
fn compact_pc_keeps_finite_request_caps_and_visible_policy_at_all_widths() {
    let _request = REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for visible in [false, true] {
        let mut baseline = None;
        for workers in [1, 2, 4, 8] {
            let mut query = source(2, 0x300 | (0x300 << 10), workers)
                .with_count_policy(PcCountPolicy::CountUnique)
                .with_objective(ObjectivePolicy::unique())
                .with_execution_policy(
                    policy(workers)
                        .with_max_memory_mib(Some(1024))
                        .with_max_nodes(1_000_000)
                        .with_max_candidates(100_000)
                        .with_max_frontier_states(100_000),
                );
            if visible {
                query = query.with_queue_observation_policy(
                    clearra_supply::QueueObservationPolicy::VisibleSeven,
                );
            }
            let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
            let result = WasmCpuSearchBackend::execute_with_control(
                &problem,
                &ExecutionControl::default().with_parallelism(workers),
            )
            .unwrap_or_else(|error| panic!("visible={visible}, workers={workers}: {error:?}"));
            if workers > 1 {
                assert_eq!(result.usize_field("workers_used"), Some(workers));
                assert_eq!(result.bool_field("cpu_parallel_execution"), Some(true));
            }
            if let Some(expected) = &baseline {
                same_family(&result, expected);
            } else {
                baseline = Some(result);
            }
        }
    }
}

#[test]
fn compact_typed_tiling_keeps_the_same_family_under_its_native_parallel_authority() {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_core_executor::{
        WasmCpuSearchAdvance, WasmCpuSearchSession, WasmCpuTerminalResourceAuthority,
    };
    use clearra_pc_graph::request::WorkerPolicy;
    use clearra_supply::queue::fixed_sequence::FixedSequence;
    use std::sync::Arc;

    let _request = REQUESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let hardware = WorkerPolicy::hardware_worker_limit().min(2);
    for height in [2, 4] {
        // The 4L field has exactly nine inverse-row O tilings. The 2L field
        // has five pieces, keeping this an ordinary bounded product fixture.
        let (initial, queue) = if height == 2 {
            (
                0,
                vec![
                    PieceKind::I,
                    PieceKind::I,
                    PieceKind::O,
                    PieceKind::O,
                    PieceKind::O,
                ],
            )
        } else {
            (
                (0..4).fold(0_u64, |mask, row| mask | (0x3f0_u64 << (row * 10))),
                vec![PieceKind::O; 4],
            )
        };
        let mut baseline = None;
        for worker_policy in [
            WorkerPolicy::Fixed(1),
            WorkerPolicy::Fixed(hardware),
            WorkerPolicy::Auto,
        ] {
            let execution = policy(1)
                .with_worker_hardware_limit(hardware)
                .with_worker_policy(worker_policy);
            let workers = execution.workers();
            // This is the same pre-compilation parent/child authority that
            // App uses. Merely deleting the fixed-worker rejection would not
            // admit the child's N compute slots beneath its parent.
            let authority =
                WasmCpuTerminalResourceAuthority::try_acquire_full_capacity_with_compute_units(
                    workers,
                )
                .unwrap();
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(height, initial),
                PcQueueInput::fixed_sequence(FixedSequence::new(queue.clone())),
                PieceWindow::new(queue.len()),
            )
            .with_exact_pieces(Some(queue.len()))
            .with_min_remaining_queue(0)
            .with_allow_hold(false)
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_objective(ObjectivePolicy::tiling())
            .with_execution_policy(execution);
            let problem = Arc::new(ProblemCompiler::compile_scenario_pc_tiling(&query).unwrap());
            let result =
                WasmCpuSearchBackend::execute_shared_under_authority_with_control_and_terminal(
                    Arc::clone(&problem),
                    0,
                    &authority,
                    &ExecutionControl::default().with_parallelism(workers),
                    |result, terminal| {
                        let result = result.unwrap();
                        terminal
                            .unwrap()
                            .validate_public_result_memory(&result)
                            .unwrap();
                        result
                    },
                );
            assert!(result.pc_tiling_family_publication_contract_is_valid());
            assert_eq!(result.bool_field("count_complete"), Some(true));
            assert_eq!(result.bool_field("buildup_executed"), Some(false));
            assert_eq!(result.usize_field("workers_used"), Some(workers));
            let store = result.tiling_solution_page_store().unwrap();
            assert!(store.len() > 1);
            if height == 4 {
                assert_eq!(store.len(), 9);
            }
            let snapshot = (
                store.normalized_hash().to_owned(),
                store.page_keys(0, 100).unwrap(),
            );
            if let Some(expected) = &baseline {
                assert_eq!(
                    &snapshot, expected,
                    "height={height}, policy={worker_policy:?}"
                );
            } else {
                baseline = Some(snapshot.clone());
            }
            drop(result);

            let control = ExecutionControl::default().with_parallelism(workers);
            let mut session =
                WasmCpuSearchSession::new_shared_under_authority(problem, 0, &authority).unwrap();
            if workers > 1 {
                assert!(matches!(
                    session.advance(0, &control).unwrap(),
                    WasmCpuSearchAdvance::Pending
                ));
            }
            let result = (0..4096)
                .find_map(|_| match session.advance(256, &control).unwrap() {
                    WasmCpuSearchAdvance::Pending => None,
                    WasmCpuSearchAdvance::Completed(result) => Some(result),
                    WasmCpuSearchAdvance::Cancelled => panic!("Tiling cancelled"),
                })
                .expect("small cooperative Tiling finishes");
            session.validate_public_result_memory(&result).unwrap();
            assert!(result.pc_tiling_family_publication_contract_is_valid());
            assert_eq!(result.usize_field("workers_used"), Some(workers));
            let store = result.tiling_solution_page_store().unwrap();
            assert_eq!(
                (
                    store.normalized_hash().to_owned(),
                    store.page_keys(0, 100).unwrap()
                ),
                snapshot
            );
        }
    }
}
