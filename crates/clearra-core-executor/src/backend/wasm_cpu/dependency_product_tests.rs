//! Small differential boundaries for the Boolean dependency consumers. The
//! endpoint checks retain the concrete BuildUp verifier as an independent
//! movement reference and check that CountAll and explicit memory admission
//! retain their existing concrete producer contracts.

use std::sync::{Mutex, MutexGuard};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblem};
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::queue::{
    fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression,
};

use super::{buildup::with_product_verifier_counts, with_pc_placement_dependency_reference};
use crate::{CoreExecutionResult, WasmCpuSearchBackend};

const O_CAVITY: u64 = 0xf3fcf;

// These endpoint checks acquire the process-wide terminal resource lease.
// Keep their unbounded requests from contending with one another while the
// reference/product TLS and the real product admission rules stay intact.
static PRODUCT_TEST_LOCK: Mutex<()> = Mutex::new(());

fn product_test_guard() -> MutexGuard<'static, ()> {
    PRODUCT_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn one_lock_problem(
    board: PcScenarioBoard,
    source: PcQueueInput,
    draws: usize,
    hold_enabled: bool,
    held: Option<PieceKind>,
    count: PcCountPolicy,
    max_memory_mib: Option<u64>,
) -> SearchProblem {
    ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(board, source, PieceWindow::new(1))
            .with_rule(srs_plus())
            .with_allow_hold(hold_enabled)
            .with_hold_piece(held)
            .with_exact_pieces(Some(1))
            .with_supply_window_size(SupplyWindowSize::new(draws))
            .with_count_policy(count)
            .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
            .with_retained_trace_limit(1)
            .with_execution_policy(
                PcExecutionPolicy::mvp_default()
                    .with_requested_backend(RequestedSearchBackend::Cpu)
                    .with_allow_backend_fallback(false)
                    .with_workers(1)
                    .with_max_memory_mib(max_memory_mib)
                    .with_tablebase_requested(false)
                    .with_precompute_build_dependencies(false),
            ),
    )
    .expect("bounded one-lock source")
    .with_output_policy(SearchOutputPolicy::Summary)
}

fn assert_consumer_parity(reference: &CoreExecutionResult, dependency: &CoreExecutionResult) {
    assert!(reference.solution_found());
    assert!(!reference.solution_coverages().is_empty());
    assert_eq!(
        dependency.normalized_solution_identities(),
        reference.normalized_solution_identities()
    );
    assert_eq!(
        dependency.coverage_pattern_words(),
        reference.coverage_pattern_words()
    );
    assert_eq!(
        dependency.solution_coverages(),
        reference.solution_coverages()
    );
    assert_eq!(
        dependency.normalized_solution_coverages(),
        reference.normalized_solution_coverages()
    );
    assert_eq!(
        dependency.solution_probabilities(),
        reference.solution_probabilities()
    );
    assert_eq!(dependency.path_steps(), reference.path_steps());
    for field in [
        "unique_solution_count",
        "normalized_solution_set_hash",
        "coverage_pattern_count",
        "covered_pattern_count",
        "coverage_probability",
        "count_complete",
        "probability_complete",
        "resource_truncated",
    ] {
        assert_eq!(dependency.field(field), reference.field(field), "{field}");
    }
    for result in [reference, dependency] {
        assert_eq!(result.bool_field("count_complete"), Some(true));
        assert_eq!(result.bool_field("probability_complete"), Some(true));
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
        assert_eq!(result.path_steps().len(), 1);
        assert_eq!(result.path_steps()[0].piece(), PieceKind::O);
    }
    assert!(reference.usize_field("total_build_order_nodes").unwrap() > 0);
    assert_eq!(dependency.usize_field("total_build_order_nodes"), Some(0));
    assert_eq!(dependency.usize_field("total_reachability_states"), Some(0));
}

#[test]
fn dependency_fixed_and_materialized_pc_preserve_hold_coverage_and_replay() {
    let _guard = product_test_guard();
    use PieceKind::{I, O};

    let fixed = |pieces| PcQueueInput::fixed_sequence(FixedSequence::new(pieces));
    for (source, draws, hold_enabled, held) in [
        (fixed(vec![O]), 1, false, None),
        (fixed(vec![I, O]), 2, true, None),
        (fixed(vec![I]), 1, true, Some(O)),
        (
            PcQueueInput::pattern_expression(QueuePatternExpression::parse("[IOT]", 7).unwrap()),
            1,
            false,
            None,
        ),
    ] {
        let problem = one_lock_problem(
            PcScenarioBoard::standard_10(2, O_CAVITY),
            source,
            draws,
            hold_enabled,
            held,
            PcCountPolicy::CountUnique,
            None,
        );
        let execute = || {
            with_product_verifier_counts(|| {
                WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                    .expect("complete one-O PC")
            })
        };
        let (reference, (reference_graph, reference_dependency)) =
            with_pc_placement_dependency_reference(execute);
        let (dependency, (dependency_graph, dependency_proof)) = execute();
        assert!(reference_graph > 0);
        assert_eq!(reference_dependency, 0);
        assert_eq!(dependency_graph, 0);
        assert!(dependency_proof > 0);
        assert_consumer_parity(&reference, &dependency);
        assert_eq!(dependency.path_steps()[0].cleared_lines(), 2);

        // Compute the expected original IDs from the request's immutable
        // universe, independently of either candidate's local pattern index.
        let universe = problem.piece_source().materialized_universe().unwrap();
        let expected = PatternBitSet::from_pattern_indices(
            universe.pattern_count(),
            (0..universe.pattern_count())
                .filter(|&ordinal| {
                    let queue = universe.sequence_at(ordinal);
                    queue.first() == Some(&O)
                        || (hold_enabled
                            && (held == Some(O) || (held.is_none() && queue.get(1) == Some(&O))))
                })
                .map(|ordinal| ordinal as u32)
                .collect(),
        )
        .unwrap();
        assert_eq!(
            dependency.coverage_pattern_words(),
            expected.to_owned_words()
        );
        let ordinal = dependency.usize_field("representative_pattern_id").unwrap();
        assert_ne!(
            dependency.coverage_pattern_words()[ordinal / 64] & (1 << (ordinal % 64)),
            0
        );
    }
}

#[test]
fn dependency_boolean_scope_keeps_count_all_and_explicit_budget_on_concrete_producer() {
    let _guard = product_test_guard();
    for (count, max_memory_mib) in [
        (PcCountPolicy::CountAll, None),
        (PcCountPolicy::CountUnique, Some(256)),
    ] {
        let problem = one_lock_problem(
            PcScenarioBoard::standard_10(2, O_CAVITY),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            1,
            false,
            None,
            count,
            max_memory_mib,
        );
        let (result, (graph_calls, dependency_calls)) = with_product_verifier_counts(|| {
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .unwrap()
        });
        assert!(graph_calls > 0);
        assert_eq!(dependency_calls, 0);
        assert!(result.usize_field("total_build_order_nodes").unwrap() > 0);
        assert!(result.solution_found());
        assert_eq!(result.coverage_pattern_words(), [1]);
        assert_eq!(result.bool_field("count_complete"), Some(true));
        if count == PcCountPolicy::CountAll {
            assert!(result.usize_field("build_variant_count").unwrap() > 0);
        }
        assert_eq!(result.path_steps().len(), 1);
        assert_eq!(result.path_steps()[0].piece(), PieceKind::O);
    }
}

#[test]
fn dependency_product_selects_trace_and_summary_for_every_builtin_without_legacy_flag() {
    let _guard = product_test_guard();
    use clearra_rules::profile::builtin_rules;

    for rule in [
        builtin_rules::srs(),
        builtin_rules::srs_plus(),
        builtin_rules::srs_x(),
        builtin_rules::jstris_180(),
        builtin_rules::no_kick(),
    ] {
        for output in [SearchOutputPolicy::Trace, SearchOutputPolicy::Summary] {
            let problem = ProblemCompiler::compile_scenario_pc(
                &PcScenarioQuery::new(
                    PcScenarioBoard::standard_10(2, O_CAVITY),
                    PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
                    PieceWindow::new(1),
                )
                .with_rule(rule.clone())
                .with_allow_hold(false)
                .with_exact_pieces(Some(1))
                .with_supply_window_size(SupplyWindowSize::new(1))
                .with_count_policy(PcCountPolicy::CountUnique)
                .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
                .with_retained_trace_limit(1)
                .with_execution_policy(
                    PcExecutionPolicy::mvp_default()
                        .with_requested_backend(RequestedSearchBackend::Cpu)
                        .with_allow_backend_fallback(false)
                        .with_workers(1)
                        .with_tablebase_requested(false)
                        .with_precompute_build_dependencies(false),
                ),
            )
            .unwrap()
            .with_output_policy(output);
            assert!(problem.pc_placement_dependency_eligible());
            assert!(!problem.pc_placement_dependency_requested());
            let execute = || {
                with_product_verifier_counts(|| {
                    WasmCpuSearchBackend::execute_with_control(
                        &problem,
                        &ExecutionControl::default(),
                    )
                    .unwrap()
                })
            };
            let (reference, (reference_graph, reference_dependency)) =
                with_pc_placement_dependency_reference(execute);
            let (product, (product_graph, product_dependency)) = execute();
            assert!(reference_graph > 0);
            assert_eq!(reference_dependency, 0);
            assert_eq!(product_graph, 0);
            assert!(product_dependency > 0);
            assert_consumer_parity(&reference, &product);
            assert_eq!(
                product.bool_field("pc_placement_dependency_selected"),
                Some(true),
            );
            assert_eq!(
                reference.bool_field("pc_placement_dependency_selected"),
                Some(false),
            );
        }
    }
}
