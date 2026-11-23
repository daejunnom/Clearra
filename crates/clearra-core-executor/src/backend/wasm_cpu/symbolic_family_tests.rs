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
        let universe = problem.piece_source().materialized_universe().unwrap();
        if universe.pattern_count() <= 64 {
            let identity = coverage.identity();
            let mut rows = Vec::new();
            for index in 0..identity.placement_count() {
                let placement = identity.placement(index).unwrap();
                rows.push(
                    symbolic
                        .catalog
                        .skeleton_id(placement.piece(), placement.cells_mask())
                        .unwrap(),
                );
            }
            rows.sort_unstable();
            let row_count = symbolic.catalog.skeleton_count() as u32;
            for ordinal in 0..universe.pattern_count() {
                let queue = universe.sequence_at(ordinal);
                let variables = rows.iter().copied().chain(queue.iter().enumerate().map(
                    |(position, &piece)| {
                        row_count + position as u32 * 7 + super::super::piece_index(piece) as u32
                    },
                ));
                let expected = coverage.covered_patterns().words()[ordinal / 64]
                    & (1_u64 << (ordinal % 64))
                    != 0;
                assert_eq!(
                    symbolic
                        .sets
                        .contains(symbolic.accepted, variables)
                        .unwrap(),
                    expected,
                    "exact identity/source pair at ordinal {ordinal}",
                );
            }
        }
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
fn symbolic_family_shared_row_checks_preserve_distinct_source_and_hold_relations() {
    // A six-column well can be tiled in different ways. Two fresh draws plus
    // the initial I exercise distinct source tails and terminal Hold release.
    let required = 0x3f | (0x3f << 10);
    let request = query(2, 0xfffff ^ required, 3, PcQueueInput::standard_7_bag())
        .with_supply_window_size(SupplyWindowSize::new(2))
        .with_allow_hold(true)
        .with_hold_piece(Some(PieceKind::I))
        .with_rule(jstris_180());
    let result = compare(&request);
    assert_eq!(result.report.source_count, 42);
    assert!(result.report.unique_solution_count > 1);
    assert!(result.report.covered_source_count > 1);
    assert!(result.report.covered_source_count < result.report.source_count);
    assert!(result
        .report
        .ranks
        .iter()
        .any(|rank| rank.source_semijoins > 0));
}

fn assert_same_joint_family(
    actual: &mut SymbolicFamilyResult,
    expected: &mut SymbolicFamilyResult,
    problem: &SearchProblem,
) {
    let actual_ids: BTreeSet<_> = actual
        .identity_page(0, 10_000)
        .unwrap()
        .into_iter()
        .collect();
    let expected_ids: BTreeSet<_> = expected
        .identity_page(0, 10_000)
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(
        actual_ids, expected_ids,
        "physical quotient preserves identities"
    );
    assert_eq!(
        actual.report.unique_solution_count,
        expected.report.unique_solution_count
    );
    assert_eq!(
        actual.report.covered_source_count,
        expected.report.covered_source_count
    );
    assert_eq!(
        actual.report.solution_source_pairs,
        expected.report.solution_source_pairs
    );
    let universe = problem.piece_source().materialized_universe().unwrap();
    assert!(universe.pattern_count() <= 64);
    for identity in actual_ids {
        let mut rows = Vec::new();
        for index in 0..identity.placement_count() {
            let placement = identity.placement(index).unwrap();
            let row = actual
                .catalog
                .skeleton_id(placement.piece(), placement.cells_mask())
                .unwrap();
            assert_eq!(
                expected
                    .catalog
                    .skeleton_id(placement.piece(), placement.cells_mask()),
                Some(row)
            );
            rows.push(row);
        }
        rows.sort_unstable();
        let row_count = actual.catalog.skeleton_count() as u32;
        for ordinal in 0..universe.pattern_count() {
            let queue = universe.sequence_at(ordinal);
            let variables =
                rows.iter()
                    .copied()
                    .chain(queue.iter().enumerate().map(|(position, &piece)| {
                        row_count + position as u32 * 7 + super::super::piece_index(piece) as u32
                    }));
            assert_eq!(
                actual
                    .sets
                    .contains(actual.accepted, variables.clone())
                    .unwrap(),
                expected
                    .sets
                    .contains(expected.accepted, variables)
                    .unwrap(),
                "original identity/source pair at ordinal {ordinal}",
            );
        }
    }
}

#[test]
fn symbolic_family_cell_occupancy_preserves_full_source_pairs_for_all_builtin_kicks() {
    let required = 0x3f | (0x3f << 10);
    for rule in [no_kick(), srs(), srs_plus(), srs_x(), jstris_180()] {
        let request = query(2, 0xfffff ^ required, 3, PcQueueInput::standard_7_bag())
            .with_supply_window_size(SupplyWindowSize::new(2))
            .with_allow_hold(true)
            .with_hold_piece(Some(PieceKind::I))
            .with_rule(rule);
        let mut expected = compare(&request);
        let problem = ProblemCompiler::compile_scenario_pc(&request).unwrap();
        let mut actual = solve_symbolic_family_with_cell_occupancy(
            &problem,
            limits(),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(actual.report.movement_variable_space, "original-cells");
        assert_same_joint_family(&mut actual, &mut expected, &problem);
    }
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
#[test]
fn symbolic_family_cell_occupancy_keeps_parallel_owner_and_cofactor_lifetimes() {
    let required = 0x3f | (0x3f << 10);
    let request = query(2, 0xfffff ^ required, 3, PcQueueInput::standard_7_bag())
        .with_supply_window_size(SupplyWindowSize::new(2))
        .with_allow_hold(true)
        .with_hold_piece(Some(PieceKind::I))
        .with_rule(jstris_180());
    let policy = request
        .execution_policy()
        .clone()
        .with_use_all_logical_processors(true)
        .with_workers(9);
    let request = request.with_execution_policy(policy);
    let problem = ProblemCompiler::compile_scenario_pc(&request).unwrap();
    let mut expected = compare(&request);
    let mut actual = solve_symbolic_family_with_cell_occupancy(
        &problem,
        SymbolicFamilyLimits::default(),
        &ExecutionControl::default(),
    )
    .unwrap();
    assert_same_joint_family(&mut actual, &mut expected, &problem);
    if problem.backend_policy().workers() > 1 {
        assert!(actual
            .report
            .ranks
            .iter()
            .any(|rank| rank.movement_parallel.spawned_workers > 1));
    }
}

#[test]
fn symbolic_family_cell_occupancy_refusal_and_cancellation_never_fall_back() {
    let request = query(2, 0xfc3f0, 2, fixed(vec![PieceKind::I; 2])).with_rule(jstris_180());
    let problem = ProblemCompiler::compile_scenario_pc(&request).unwrap();
    let refusal = SymbolicFamilyLimits {
        // Two terminal IDs are the minimum valid arena. The first primitive
        // cell must then fail with a resource error, not invalid configuration.
        max_predicate_nodes: 2,
        ..limits()
    };
    assert_eq!(
        solve_symbolic_family_with_cell_occupancy(&problem, refusal, &ExecutionControl::default())
            .err(),
        Some(SymbolicFamilyError::Resource(
            "symbolic_predicate_node_limit"
        )),
    );
    let cancellation = CancellationToken::new();
    cancellation.handle().cancel();
    assert_eq!(
        solve_symbolic_family_with_cell_occupancy(
            &problem,
            limits(),
            &ExecutionControl::new(cancellation)
        )
        .err(),
        Some(SymbolicFamilyError::Cancelled),
    );
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
        assert_eq!(result.source_length(), 2);
        assert_eq!(result.covers_source(&[PieceKind::I; 2]), Ok(true));
        assert_eq!(
            result.covers_source(&[PieceKind::I, PieceKind::O]),
            Ok(false),
        );
    }
}

#[test]
fn symbolic_family_retains_queue_correlations_through_hold_and_terminal_projection() {
    use PieceKind::{I, J, L, O, S, T, Z};

    for hold in [false, true] {
        // Hidden P7 completions retain their original source identities. The
        // separated holes can consume their two visible draws only as II.
        let result = compare(&query(2, 0xfff0, 2, pattern("[IO]1[IO]1P7")).with_allow_hold(hold));
        assert_eq!(result.report.source_count, 20_160);
        assert_eq!(result.report.covered_source_count, 5_040);
        assert_eq!(result.source_length(), 9);
        assert_eq!(result.covers_source(&[I, I, I, O, T, S, Z, J, L]), Ok(true));
        assert_eq!(result.covers_source(&[I, I, L, J, Z, S, T, O, I]), Ok(true));
        assert_eq!(
            result.covers_source(&[I, O, I, O, T, S, Z, J, L]),
            Ok(false)
        );
        assert_eq!(
            result.covers_source(&[I, I, I, I, T, S, Z, J, L]),
            Ok(false)
        );
        for wrong_length in [&[][..], &[I, I], &[I, I, I, O, T, S, Z, J, L, I]] {
            assert_eq!(
                result.covers_source(wrong_length),
                Err(SymbolicFamilyError::InvalidProblem(
                    "symbolic_output_source_length",
                )),
            );
        }
    }
    let held = compare(
        &query(2, 0xfc3f0, 2, fixed(vec![PieceKind::O, PieceKind::I]))
            .with_allow_hold(true)
            .with_hold_piece(Some(PieceKind::I)),
    );
    assert_eq!(held.report.covered_source_count, 1);
    assert_eq!(held.covers_source(&[PieceKind::O, PieceKind::I]), Ok(true),);
    assert_eq!(held.covers_source(&[PieceKind::I; 2]), Ok(false));
}

#[test]
fn symbolic_family_empty_refusal_and_cancellation_are_different_outcomes() {
    let impossible = query(2, 0xfc3f0, 2, fixed(vec![PieceKind::I, PieceKind::O]));
    let result = compare(&impossible);
    assert_eq!(result.report.unique_solution_count, 0);
    assert_eq!(
        result.covers_source(&[PieceKind::I, PieceKind::O]),
        Ok(false),
    );
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

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
#[test]
fn symbolic_family_parallel_layers_preserve_complete_identity_source_relation() {
    use PieceKind::{I, O};
    let request = query(2, 0, 5, fixed(vec![I, I, O, O, O]))
        .with_allow_hold(true)
        .with_rule(jstris_180());
    let policy = request
        .execution_policy()
        .clone()
        .with_use_all_logical_processors(true)
        .with_workers(9);
    let request = request.with_execution_policy(policy);
    let mut reference = compare(&request);
    let problem = ProblemCompiler::compile_scenario_pc(&request).unwrap();
    let mut parallel = solve_symbolic_family(
        &problem,
        SymbolicFamilyLimits::default(),
        &ExecutionControl::default(),
    )
    .unwrap();
    let expected: BTreeSet<_> = reference
        .identity_page(0, 10_000)
        .unwrap()
        .into_iter()
        .collect();
    let actual: BTreeSet<_> = parallel
        .identity_page(0, 10_000)
        .unwrap()
        .into_iter()
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(
        parallel.report.unique_solution_count,
        reference.report.unique_solution_count
    );
    assert_eq!(
        parallel.report.covered_source_count,
        reference.report.covered_source_count
    );
    assert_eq!(
        parallel.report.solution_source_pairs,
        reference.report.solution_source_pairs
    );
    for identity in expected {
        assert_eq!(
            parallel.source_count_for_identity(&identity).unwrap(),
            reference.source_count_for_identity(&identity).unwrap(),
        );
    }
    if problem.backend_policy().workers() > 1 {
        assert!(parallel
            .report
            .ranks
            .iter()
            .any(|rank| rank.movement_parallel.spawned_workers > 1));
        assert!(reference.report.ranks.iter().all(|rank| {
            rank.movement_parallel.fallback_reason == Some("explicit-predicate-work-or-memo-bounds")
        }));
    }
}
