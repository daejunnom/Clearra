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
    ProblemCompiler,
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
