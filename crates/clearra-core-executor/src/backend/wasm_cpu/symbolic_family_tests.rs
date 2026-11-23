//! Finite correctness comparisons, not timing or small-input benchmarks.

use std::collections::BTreeSet;

use clearra_core_domain::execution_cancellation::CancellationToken;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x};
use clearra_supply::{
    queue::{fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression},
    QueueObservationPolicy,
};

use super::*;
use crate::WasmCpuSearchBackend;

fn limits() -> SymbolicFamilyLimits {
    // Explicit correctness-fixture bounds: refusal must fail the test rather
    // than silently replacing the algorithm or reporting an empty solution.
    SymbolicFamilyLimits {
        max_geometry_nodes: 200_000,
        max_set_nodes: 2_000_000,
        max_predicate_nodes: 2_000_000,
        max_work_per_arena: 200_000_000,
        max_memo_entries: 2_000_000,
    }
}

fn query(height: u16, initial: u64, pieces: usize, source: PcQueueInput) -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(height, initial),
        source,
        PieceWindow::new(pieces),
    )
    .with_exact_pieces(Some(pieces))
    .with_supply_window_size(SupplyWindowSize::new(pieces))
    .with_allow_hold(false)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_objective(ObjectivePolicy::unique())
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_retained_trace_limit(1)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_allow_backend_fallback(false)
            .with_workers(1)
            .with_max_memory_mib(Some(512))
            .with_cpu_warmup(false)
            .with_tablebase_requested(false)
            .with_exact_legal_board_enabled(false)
            .with_conditioned_reachability_enabled(false)
            .with_max_patterns(0),
    )
}

fn fixed(pieces: Vec<PieceKind>) -> PcQueueInput {
    PcQueueInput::fixed_sequence(FixedSequence::new(pieces))
}

fn pattern(expression: &str) -> PcQueueInput {
    PcQueueInput::pattern_expression(QueuePatternExpression::parse(expression, 0).unwrap())
}

fn compare(query: &PcScenarioQuery) -> SymbolicFamilyResult {
    let problem = ProblemCompiler::compile_scenario_pc(query).unwrap();
    let mut symbolic = solve_symbolic_family(&problem, limits(), &ExecutionControl::default())
        .unwrap_or_else(|error| panic!("symbolic execution: {error:?}"));
    let reference =
        WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
            .expect("finite reference execution");
    assert_eq!(reference.bool_field("count_complete"), Some(true));
    assert_eq!(reference.bool_field("resource_truncated"), Some(false));
    let actual: BTreeSet<_> = symbolic
        .identity_page(0, 10_000)
        .unwrap()
        .into_iter()
        .collect();
    let expected: BTreeSet<_> = reference
        .normalized_solution_identities()
        .iter()
        .copied()
        .collect();
    assert_eq!(actual, expected, "exact placement identities");
    assert_eq!(
        symbolic.report.unique_solution_count,
        expected.len() as u128
    );
    assert_eq!(
        symbolic.report.covered_source_count,
        reference
            .coverage_pattern_words()
            .iter()
            .map(|word| u128::from(word.count_ones()))
            .sum::<u128>(),
        "original source coverage",
    );
    for coverage in reference.solution_coverages() {
        assert_eq!(
            symbolic
                .source_count_for_identity(&coverage.identity())
                .unwrap(),
            u128::from(coverage.covered_patterns().count_ones()),
            "per-identity source relation",
        );
    }
    let before = symbolic.identity_page(0, 3).unwrap();
    symbolic.compact().unwrap();
    assert_eq!(symbolic.identity_page(0, 3).unwrap(), before);
    if symbolic.report.unique_solution_count > 1 {
        let tail = symbolic
            .identity_page(symbolic.report.unique_solution_count - 1, 1)
            .unwrap();
        assert_eq!(tail.len(), 1);
        assert!(expected.contains(&tail[0]));
    }
    assert!(symbolic
        .identity_page(symbolic.report.unique_solution_count, 1)
        .unwrap()
        .is_empty());
    symbolic
}

#[test]
fn symbolic_family_matches_existing_unique_solver_without_build_graphs() {
    use PieceKind::{I, O};
    let result = compare(&query(2, 0, 5, fixed(vec![I, I, O, O, O])).with_rule(srs_plus()));
    assert!(result.report.unique_solution_count > 1);
    assert!(
        result.report.catalog_rows > 64,
        "global variables exceed an operation bitmask"
    );
    assert_eq!(result.report.ranks.len(), 5);
    assert!(result
        .report
        .ranks
        .iter()
        .any(|rank| rank.joint_members > 1));
    assert!(result
        .report
        .ranks
        .iter()
        .all(|rank| rank.movement_propagations > 0));
}

#[test]
fn symbolic_family_preserves_line_clear_frames_for_every_builtin_profile() {
    for rule in [srs(), srs_plus(), srs_x(), jstris_180(), no_kick()] {
        let result = compare(&query(2, 0xfc3f0, 2, fixed(vec![PieceKind::I; 2])).with_rule(rule));
        assert!(result.report.unique_solution_count > 0);
        assert_eq!(result.report.covered_source_count, 1);
        assert_eq!(result.report.ranks.len(), 2);
    }
}

#[test]
fn symbolic_family_retains_queue_correlations_through_hold_and_terminal_projection() {
    for hold in [false, true] {
        // Hidden P7 completions retain their original source identities. The
        // separated holes can consume their two visible draws only as II.
        let result = compare(&query(2, 0xfff0, 2, pattern("[IO]1[IO]1P7")).with_allow_hold(hold));
        assert_eq!(result.report.source_count, 20_160);
        assert_eq!(result.report.covered_source_count, 5_040);
    }
    let held = compare(
        &query(2, 0xfc3f0, 2, fixed(vec![PieceKind::O, PieceKind::I]))
            .with_allow_hold(true)
            .with_hold_piece(Some(PieceKind::I)),
    );
    assert_eq!(held.report.covered_source_count, 1);
}

#[test]
fn symbolic_family_empty_refusal_and_cancellation_are_different_outcomes() {
    let impossible = query(2, 0xfc3f0, 2, fixed(vec![PieceKind::I, PieceKind::O]));
    let result = compare(&impossible);
    assert_eq!(result.report.unique_solution_count, 0);
    let problem =
        ProblemCompiler::compile_scenario_pc(&query(2, 0xfc3f0, 2, fixed(vec![PieceKind::I; 2])))
            .unwrap();
    let refusal = SymbolicFamilyLimits {
        max_set_nodes: 1,
        ..limits()
    };
    assert!(matches!(
        solve_symbolic_family(&problem, refusal, &ExecutionControl::default()),
        Err(SymbolicFamilyError::Resource(_))
    ));
    let cancellation = CancellationToken::new();
    cancellation.handle().cancel();
    assert!(matches!(
        solve_symbolic_family(&problem, limits(), &ExecutionControl::new(cancellation)),
        Err(SymbolicFamilyError::Cancelled)
    ));
    let unsupported = ProblemCompiler::compile_scenario_pc(
        &query(2, 0xfc3f0, 2, fixed(vec![PieceKind::I; 2]))
            .with_count_policy(PcCountPolicy::CountAll),
    )
    .unwrap();
    assert!(matches!(
        solve_symbolic_family(&unsupported, limits(), &ExecutionControl::default()),
        Err(SymbolicFamilyError::Unsupported(_))
    ));
}
