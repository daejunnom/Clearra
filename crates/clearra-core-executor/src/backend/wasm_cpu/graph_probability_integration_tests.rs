//! Graph-language fusion must preserve each solution's source queues, not just
//! an aggregate success count. The explicit fixed-queue runs below do not use
//! the symbolic standard-bag product and run with the prototype disabled.

use std::sync::{Mutex, MutexGuard};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_coverage::pattern::pattern_id::PatternId;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::{
    buildup::with_operation_mask_comparison,
    placement_dependency::{with_mode, PlacementDependencyMetrics},
};
use crate::{CoreExecutionResult, WasmCpuSearchBackend};

static GRAPH_PRODUCT_TEST_LOCK: Mutex<()> = Mutex::new(());

fn product_guard() -> MutexGuard<'static, ()> {
    GRAPH_PRODUCT_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn execution_policy() -> PcExecutionPolicy {
    // The test override belongs to this thread. Product worker selection is
    // unchanged; the large benchmark uses process-wide mode configuration.
    PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(1)
        .with_max_memory_mib(Some(256))
}

fn run(
    problem: &SearchProblem,
    enabled: bool,
) -> (CoreExecutionResult, PlacementDependencyMetrics) {
    with_mode(enabled, || {
        WasmCpuSearchBackend::execute_with_control(problem, &ExecutionControl::default())
            .expect("bounded exact graph/supply fixture")
    })
}

fn assert_complete(result: &CoreExecutionResult) {
    for field in [
        "count_complete",
        "probability_complete",
        "solution_probability_complete",
    ] {
        assert_eq!(result.bool_field(field), Some(true), "{field}");
    }
    assert_eq!(result.bool_field("resource_truncated"), Some(false));
}

fn assert_same_solution_queues(a: &CoreExecutionResult, b: &CoreExecutionResult) {
    assert_complete(a);
    assert_complete(b);
    assert_eq!(
        a.normalized_solution_identities(),
        b.normalized_solution_identities()
    );
    assert_eq!(a.normalized_solution_keys(), b.normalized_solution_keys());
    assert_eq!(a.coverage_pattern_words(), b.coverage_pattern_words());
    assert_eq!(a.solution_coverages(), b.solution_coverages());
    assert_eq!(
        a.normalized_solution_coverages(),
        b.normalized_solution_coverages()
    );
    assert_eq!(a.solution_probabilities(), b.solution_probabilities());
    for field in [
        "normalized_unique_solution_count",
        "normalized_solution_set_hash",
        "coverage_pattern_count",
        "covered_pattern_count",
        "coverage_probability",
        "materialized_probability_mass",
        "build_variant_count",
        "build_variant_count_exact",
    ] {
        assert_eq!(a.field(field), b.field(field), "{field}");
    }
}

fn two_piece_query(input: PcQueueInput, hold: bool, held: Option<PieceKind>) -> PcScenarioQuery {
    // Required cells: bottom x=0..5 and upper x=0..1. O then I is a
    // concrete solution with an intermediate clear; alternative tilings and
    // ordering are left to the exact producer rather than assumed by the test.
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0xff3c0),
        input,
        PieceWindow::new(2),
    )
    .with_allow_hold(hold)
    .with_hold_piece(held)
    .with_exact_pieces(Some(2))
    .with_supply_window_size(SupplyWindowSize::new(2))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_retained_trace_limit(1)
    .with_execution_policy(execution_policy())
}

#[test]
fn fused_graph_probability_matches_every_explicit_two_piece_queue_with_hold() {
    let _guard = product_guard();
    let mut compared_queues = 0;
    for (hold, held) in [(false, None), (true, None), (true, Some(PieceKind::O))] {
        let query = two_piece_query(PcQueueInput::standard_7_bag(), hold, held);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("symbolic bag fixture");
        assert_eq!(problem.count_policy(), PcCountPolicy::CountUnique);
        let (baseline, off) = run(&problem, false);
        let (fused, on) = run(&problem, true);
        assert_same_solution_queues(&baseline, &fused);
        assert_eq!(off.fused_language_graphs, 0);
        assert!(
            on.fused_language_graphs > 0,
            "must exercise the fused producer"
        );
        assert!(!fused.solution_coverages().is_empty());
        assert_eq!(
            fused.solution_coverages().len(),
            fused.normalized_solution_identities().len()
        );
        assert_eq!(
            fused.solution_probabilities().len(),
            fused.solution_coverages().len()
        );

        let universe = problem.piece_source().materialized_universe().unwrap();
        assert_eq!(universe.pattern_count(), 42);
        let mut accepted = 0;
        for index in 0..universe.pattern_count() {
            let queue = universe.sequence_at(index).into_owned();
            let explicit_query = two_piece_query(
                PcQueueInput::fixed_sequence(FixedSequence::new(queue.clone())),
                hold,
                held,
            );
            let explicit_problem = ProblemCompiler::compile_scenario_pc(&explicit_query)
                .expect("explicit queue fixture");
            let (explicit, _) = run(&explicit_problem, false);
            assert_complete(&explicit);
            let pattern = PatternId::new(index);
            let actual: Vec<_> = fused
                .solution_coverages()
                .iter()
                .filter(|entry| entry.covered_patterns().contains(pattern))
                .map(|entry| entry.identity())
                .collect();
            assert_eq!(
                actual,
                explicit.normalized_solution_identities(),
                "queue={queue:?}, hold={hold}, held={held:?}",
            );
            let globally_covered =
                fused.coverage_pattern_words()[index / 64] & (1_u64 << (index % 64)) != 0;
            assert_eq!(globally_covered, !actual.is_empty());
            accepted += usize::from(globally_covered);
            compared_queues += 1;
        }
        assert!(
            accepted > 0 && accepted < universe.pattern_count(),
            "nonvacuous queue language"
        );
        assert_eq!(fused.usize_field("covered_pattern_count"), Some(accepted));
        let probability: f64 = fused
            .field("coverage_probability")
            .unwrap()
            .parse()
            .unwrap();
        let expected = accepted as f64 / universe.pattern_count() as f64;
        assert!((probability - expected).abs() < 1e-14);
        eprintln!(
            "fused queue mapping: hold={hold}, held={held:?}, solutions={}, accepted={accepted}/42, fused_graphs={}",
            fused.solution_coverages().len(), on.fused_language_graphs,
        );
    }
    assert_eq!(compared_queues, 126);
}

#[test]
fn fused_graph_skips_dead_languages_without_changing_live_solution_queues() {
    let _guard = product_guard();
    // This catalog contains both physically dead and successful candidates.
    // One workspace is reused across both kinds of candidate.
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0xfc3f_0fc3_f0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(4),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(4))
    .with_supply_window_size(SupplyWindowSize::new(4))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_execution_policy(execution_policy());
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("mixed candidate fixture");
    let (baseline, _) = run(&problem, false);
    let (treatment, metrics) = run(&problem, true);
    assert_same_solution_queues(&baseline, &treatment);
    assert_eq!(
        treatment.usize_field("normalized_unique_solution_count"),
        Some(135)
    );
    assert_eq!(treatment.usize_field("covered_pattern_count"), Some(173));
    assert_eq!(treatment.usize_field("coverage_pattern_count"), Some(840));
    assert!(metrics.fused_language_graphs > 0);
    assert!(metrics.skipped_dead_language_graphs > 0);
    assert!(metrics.skipped_dead_language_nodes > 0);
    eprintln!(
        "mixed graph language: fused={}, skipped_dead_graphs={}, skipped_dead_nodes={}",
        metrics.fused_language_graphs,
        metrics.skipped_dead_language_graphs,
        metrics.skipped_dead_language_nodes,
    );
}

#[test]
fn operation_mask_physical_traversal_preserves_840_queue_coverage_and_graph_replay() {
    let _guard = product_guard();
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0xfc3f_0fc3_f0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(4),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(4))
    .with_supply_window_size(SupplyWindowSize::new(4))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_retained_trace_limit(1)
    .with_execution_policy(execution_policy());
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("mixed candidate fixture");
    let run_arm = |enabled| {
        with_operation_mask_comparison(enabled, || {
            super::structural_order::with_mode(false, || {
                super::structural_order::with_conditional_order_mode(false, || run(&problem, false))
            })
            .0
        })
    };
    let ((baseline, _), graph) = run_arm(false);
    let ((treatment, _), masks) = run_arm(true);

    assert_same_solution_queues(&baseline, &treatment);
    assert_eq!(treatment.usize_field("coverage_pattern_count"), Some(840));
    assert_eq!(treatment.usize_field("covered_pattern_count"), Some(173));
    assert_eq!(treatment.solution_coverages().len(), 135);
    assert!(treatment
        .solution_coverages()
        .iter()
        .all(|row| { row.covered_patterns().pattern_count() == 840 }));
    assert_eq!(graph.mask_passes, 0);
    assert_eq!(graph.max_mask_capacity, 0);
    assert!(graph.max_node_capacity > 0 && graph.max_edge_capacity > 0);
    assert!(masks.mask_live_passes > 0 && masks.mask_dead_passes > 0);
    assert_eq!(
        masks.mask_passes,
        masks.mask_live_passes + masks.mask_dead_passes
    );
    assert_eq!(masks.graph_passes + masks.mask_passes, graph.graph_passes);
    assert_eq!(masks.visited_subsets, graph.visited_subsets);
    assert_eq!(masks.selected_edges, graph.selected_edges);
    assert!(masks.max_mask_capacity >= 1 << 4);
    assert_eq!(masks.max_mask_owner_node_capacity, 0);
    assert_eq!(masks.max_mask_owner_edge_capacity, 0);

    // A representative still enters verify_candidate with retain_trace and
    // replays a concrete graph path; no mask-generated pose escapes storage.
    assert!(masks.representative_graph_paths > 0);
    assert_eq!(
        masks.representative_graph_paths,
        graph.representative_graph_paths
    );
    assert_eq!(treatment.path_steps(), baseline.path_steps());
    assert_eq!(treatment.path_steps().len(), 4);
    assert_eq!(
        treatment
            .path_steps()
            .iter()
            .map(|step| usize::from(step.cleared_lines()))
            .sum::<usize>(),
        4
    );
    let pattern_id = treatment
        .usize_field("representative_pattern_id")
        .expect("representative has an explicit source queue");
    assert_eq!(
        Some(pattern_id),
        baseline.usize_field("representative_pattern_id")
    );
    let queue = problem
        .piece_source()
        .materialized_universe()
        .expect("fixture source universe")
        .sequence_at(pattern_id);
    assert!(treatment
        .path_steps()
        .iter()
        .map(|step| step.piece())
        .eq(queue.iter().copied()));
    eprintln!("operation-mask raw slots: graph={graph:?}, treatment={masks:?}");
}

#[test]
fn fused_graph_keeps_same_kind_supply_actions_distinct_for_count_all() {
    let _guard = product_guard();
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_execution_policy(execution_policy());
    let mut counts = [0_u128; 2];
    for (index, held) in [None, Some(PieceKind::I)].into_iter().enumerate() {
        let problem = ProblemCompiler::compile_scenario_pc(
            &query
                .clone()
                .with_allow_hold(held.is_some())
                .with_hold_piece(held)
                .with_count_policy(PcCountPolicy::CountAll),
        )
        .expect("same-current-and-held fixture");
        let (baseline, _) = run(&problem, false);
        let (treatment, metrics) = run(&problem, true);
        assert_same_solution_queues(&baseline, &treatment);
        assert_eq!(
            metrics.fused_language_graphs, 0,
            "Boolean language must not count variants"
        );
        assert_eq!(treatment.solution_coverages().len(), 1);
        assert_eq!(treatment.coverage_pattern_words(), &[1]);
        counts[index] = treatment
            .field("build_variant_count")
            .unwrap()
            .parse()
            .unwrap();
    }
    assert!(counts[0] > 0);
    assert_eq!(counts[1], 2 * counts[0]);
}
