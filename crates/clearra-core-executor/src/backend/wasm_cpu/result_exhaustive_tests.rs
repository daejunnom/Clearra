use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
    WorkerPolicy,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblem};
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::{
    queue::{fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression},
    QueueObservationPolicy,
};

use crate::{CoreExecutionResult, WasmCpuSearchBackend};

use super::{ExactSearchAdvance, GeometryAdvance, WasmExactSearchSession};

fn query(queue: PcQueueInput, workers: usize) -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0x3_c0f0_3c0f),
        queue,
        PieceWindow::new(6),
    )
    .with_rule(srs_plus())
    .with_allow_hold(false)
    .with_exact_pieces(Some(6))
    .with_supply_window_size(SupplyWindowSize::new(6))
    .with_objective(ObjectivePolicy::unique())
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_workers(workers)
            .with_tablebase_requested(false)
            .with_exact_legal_board_enabled(false)
            .with_conditioned_reachability_enabled(false),
    )
}

#[test]
fn factorized_partial_coverage_preserves_exhaustive_serial_and_parallel_results() {
    // P6 has 5,040 originals and exceeds the parser's explicit-storage
    // threshold. P4's 840 originals would exercise the explicit fallback.
    let source = PcQueueInput::pattern_expression(QueuePatternExpression::parse("P6", 0).unwrap());
    let reference = ProblemCompiler::compile_scenario_pc(&query(source.clone(), 1)).unwrap();
    let universe = reference
        .piece_source()
        .materialized_universe()
        .expect("finite source universe");
    assert_eq!(universe.pattern_count(), 5_040);
    assert!(universe.complete());
    assert!(universe.complete_factorized_layout().is_some());
    assert!(
        super::super::standard_bag_coverage::StandardBagCoverage::supports(
            universe,
            reference.initial_hold(),
        )
    );
    // with_count_policy(CountUnique) also selects the Unique objective.
    // Commutative candidate evidence additionally requires a supported
    // factorized source; P4's explicit storage does not satisfy that gate.
    // Require the complete contract before comparing across worker counts.
    assert!(super::super::uses_order_independent_pc_candidate_digest(
        &reference
    ));
    let expected =
        WasmCpuSearchBackend::execute_with_control(&reference, &ExecutionControl::default())
            .unwrap();
    let count = expected.usize_field("covered_pattern_count").unwrap();
    assert!(count > 0 && count < 5_040);
    assert_eq!(expected.bool_field("count_complete"), Some(true));
    assert!(!expected.normalized_solution_identities().is_empty());

    for workers in [1, 2] {
        let problem =
            ProblemCompiler::compile_scenario_pc(&query(source.clone(), workers)).unwrap();
        let control = ExecutionControl::default();
        let result = WasmCpuSearchBackend::execute_with_control(&problem, &control).unwrap();
        assert!(!control.is_cancelled());
        assert_eq!(
            result.coverage_pattern_words(),
            expected.coverage_pattern_words()
        );
        assert_eq!(result.usize_field("workers_used"), Some(workers));
        assert_eq!(result.bool_field("probability_complete"), Some(true));
        assert_eq!(result.bool_field("count_complete"), Some(true));
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
        assert_eq!(
            result.normalized_solution_identities(),
            expected.normalized_solution_identities()
        );
        for field in [
            "normalized_unique_solution_count",
            "normalized_solution_set_hash",
            "packing_candidate_count",
            "packing_candidate_set_digest",
            "geometry_candidate_family_count",
        ] {
            let reference = expected.field(field).expect("complete reference field");
            assert_eq!(result.field(field), Some(reference), "{field}");
        }
    }
}

#[cfg(all(not(target_family = "wasm"), target_pointer_width = "64"))]
#[test]
fn full6l_physical_prefix_pruning_matches_harness_and_rejects_other_contracts() {
    let full6l_query = |queue| {
        PcScenarioQuery::new(
            PcScenarioBoard::standard_10(6, 0),
            queue,
            PieceWindow::new(15),
        )
        .with_rule(srs_plus())
        .with_exact_pieces(Some(15))
        .with_supply_window_size(SupplyWindowSize::new(15))
        .with_min_remaining_queue(0)
        .with_hold_piece(None)
        .with_allow_hold(true)
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
        .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
        .with_retained_trace_limit(1)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_allow_backend_fallback(false)
                .with_worker_policy(WorkerPolicy::Auto)
                .with_worker_hardware_limit(2)
                .with_use_all_logical_processors(true)
                .with_cpu_warmup(false)
                .with_exact_legal_board_enabled(false)
                .with_conditioned_reachability_enabled(false)
                .with_tablebase_requested(false)
                .with_max_patterns(0),
        )
    };
    let query = full6l_query(PcQueueInput::pattern_expression(
        QueuePatternExpression::parse("P1P7P7", 0).unwrap(),
    ));
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let universe = problem.piece_source().materialized_universe().unwrap();
    assert_eq!(universe.pattern_count(), 177_811_200);
    assert!(universe.complete_factorized_layout().is_some());
    assert_eq!(problem.objective(), ObjectivePolicy::unique());
    assert_eq!(problem.replay_trace_policy().retained_trace_limit(), 1);

    // Construction checks the actual product gate without compiling or
    // traversing the enormous Full6L candidate family.
    let session = WasmExactSearchSession::new(&problem).unwrap();
    assert!(session.physical_prefix_pruning_active);
    assert!(session.geometry.physical_prefix_pruning_enabled());
    assert_eq!(session.geometry.expanded_nodes(), 0);

    let explicit_query = full6l_query(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
        PieceKind::I,
        PieceKind::I,
        PieceKind::I,
        PieceKind::O,
        PieceKind::O,
        PieceKind::T,
        PieceKind::T,
        PieceKind::S,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::J,
        PieceKind::L,
        PieceKind::L,
    ])));
    for (contract, other) in [
        (
            "explicit source",
            ProblemCompiler::compile_scenario_pc(&explicit_query).unwrap(),
        ),
        (
            "candidate coverage evidence",
            problem.clone().with_pc_chance_probability_v2_evidence(),
        ),
        (
            "All objective",
            ProblemCompiler::compile_scenario_pc(
                &query.clone().with_objective(ObjectivePolicy::all()),
            )
            .unwrap(),
        ),
        (
            "multiple retained traces",
            ProblemCompiler::compile_scenario_pc(&query.clone().with_retained_trace_limit(2))
                .unwrap(),
        ),
        (
            "coverage-only output",
            problem
                .clone()
                .with_output_policy(SearchOutputPolicy::CoverageSummary),
        ),
    ] {
        assert!(
            !super::full6l_physical_prefix_pruning_eligible(&other, &session.catalog),
            "{contract}",
        );
    }
    let external = WasmExactSearchSession::new_external_geometry(&problem).unwrap();
    assert!(!external.physical_prefix_pruning_active);
    assert!(!external.geometry.physical_prefix_pruning_enabled());
}

#[test]
fn physical_prefix_pruning_preserves_exhaustive_serial_and_parallel_results() {
    assert_square_piece_prefix_parity(4);
}

#[test]
fn physical_row_order_preserves_exhaustive_six_o_serial_and_parallel_results() {
    assert_square_piece_prefix_parity(6);
}

fn assert_square_piece_prefix_parity(height: u16) {
    fn execute(problem: &SearchProblem, workers: usize, filtered: bool) -> CoreExecutionResult {
        let mut session = WasmExactSearchSession::new(problem).unwrap();
        assert!(!session.physical_prefix_pruning_active);
        #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
        let profile = (filtered && session.catalog.height() == 6)
            .then(|| crate::ExecutorSearchProfileSession::start().expect("row-order profile"));
        if filtered {
            // This tiny fixture deliberately lies outside the Full6L product
            // gate, but exercises the same enumerator and result integration.
            session.geometry.enable_physical_prefix_pruning();
            session.physical_prefix_pruning_active = true;
        }
        let control = ExecutionControl::default();
        let result = if workers > 1 {
            session
                .execute_parallel_with_identity_storage(workers, &control, true)
                .unwrap()
                .expect("the square-piece family has multiple parallel branches")
        } else {
            loop {
                match session.advance(64, &control).unwrap() {
                    ExactSearchAdvance::Pending => {}
                    ExactSearchAdvance::Completed(result) => break result,
                    ExactSearchAdvance::Cancelled => panic!("the control is not cancelled"),
                }
            }
        };
        #[cfg(any(feature = "search-stage-profiling", feature = "wasm-stage-profiling"))]
        if let Some(profile) = profile {
            let stages = profile.finish();
            for stage_name in [
                "wasm.geometry.row_order_prepare",
                "wasm.geometry.row_order_pruned_subtrees",
            ] {
                assert!(
                    stages
                        .iter()
                        .any(|stage| { stage.name == stage_name && stage.work_item_count > 0 }),
                    "{stage_name} must run with {workers} workers",
                );
            }
            // Preparation alone can also report a failed optional attempt.
            // A table-only prune proves this exercised the ready 24-order
            // table in each execution, rather than the closure fallback.
        }
        result
    }

    // The six initially filled rightmost cells per row leave a four-column
    // rectangle. Height four exercises the closure fallback; height six
    // admits the 24-order table. Interleaved O rows include dependency cycles
    // alongside physical solutions in both finite candidate families.
    let pieces = usize::from(height);
    let initial = (0..height).fold(0_u64, |mask, row| mask | (0x3f0_u64 << (10 * row)));
    let problem = ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(height, initial),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; pieces])),
            PieceWindow::new(pieces),
        )
        .with_rule(srs_plus())
        .with_allow_hold(false)
        .with_exact_pieces(Some(pieces))
        .with_supply_window_size(SupplyWindowSize::new(pieces))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
        .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
        .with_retained_trace_limit(1)
        .with_execution_policy(
            PcExecutionPolicy::default()
                .with_workers(2)
                .with_tablebase_requested(false)
                .with_exact_legal_board_enabled(false)
                .with_conditioned_reachability_enabled(false),
        ),
    )
    .unwrap();
    let reference = execute(&problem, 1, false);
    assert!(!reference.normalized_solution_identities().is_empty());
    assert_eq!(reference.coverage_pattern_words(), &[1]);
    assert_eq!(reference.path_steps().len(), pieces);
    let raw_family: u128 = reference
        .field("geometry_candidate_family_count")
        .unwrap()
        .parse()
        .unwrap();
    let raw_emitted = reference.usize_field("packing_candidate_count").unwrap();
    assert_eq!(raw_family, raw_emitted as u128);
    let mut filtered_emitted = None;

    for (workers, filtered) in [(1, false), (2, false), (1, true), (2, true)] {
        let result = execute(&problem, workers, filtered);
        assert_eq!(result.usize_field("workers_used"), Some(workers));
        assert_eq!(result.bool_field("probability_complete"), Some(true));
        assert_eq!(result.bool_field("count_complete"), Some(true));
        assert_eq!(result.bool_field("packing_count_complete"), Some(true));
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
        assert_eq!(
            result.normalized_solution_identities(),
            reference.normalized_solution_identities(),
        );
        assert_eq!(
            result.normalized_solution_keys(),
            reference.normalized_solution_keys(),
        );
        assert_eq!(
            result.coverage_pattern_words(),
            reference.coverage_pattern_words()
        );
        assert_eq!(result.path_steps(), reference.path_steps());
        for field in [
            "normalized_unique_solution_count",
            "normalized_solution_set_hash",
            "geometry_candidate_family_count",
        ] {
            assert_eq!(result.field(field), reference.field(field), "{field}");
        }
        assert_eq!(
            result.bool_field("physical_prefix_pruning_active"),
            Some(filtered),
        );
        assert_eq!(
            result.bool_field("packing_candidate_set_digest_calculated"),
            Some(!filtered),
        );
        let emitted = result.usize_field("packing_candidate_count").unwrap();
        if filtered {
            assert!(
                emitted < raw_emitted,
                "the fixture must exercise a real prune"
            );
            assert_eq!(
                result.field("packing_candidate_domain"),
                Some("physical-prefix-filtered"),
            );
            assert!(result
                .field("packing_candidate_set_digest")
                .unwrap()
                .starts_with("not-calculated"));
            assert_eq!(
                result.bool_field("geometry_prefix_pruned_candidate_count_calculated"),
                Some(true),
            );
            let pruned: u128 = result
                .field("geometry_prefix_pruned_candidate_count")
                .unwrap()
                .parse()
                .unwrap();
            assert_eq!(pruned, raw_family - emitted as u128);
            if let Some(expected) = filtered_emitted {
                assert_eq!(emitted, expected);
            } else {
                filtered_emitted = Some(emitted);
            }
        } else {
            assert_eq!(emitted, raw_emitted);
            assert_eq!(
                result.field("packing_candidate_domain"),
                Some("unfiltered-geometric-family"),
            );
            assert_eq!(
                result.bool_field("geometry_prefix_pruned_candidate_count_calculated"),
                Some(false),
            );
            assert_eq!(
                result.field("geometry_prefix_pruned_candidate_count"),
                Some("not-calculated"),
            );
        }
    }
}

#[test]
fn cancelled_candidate_cannot_gain_complete_authority_with_a_fresh_control() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_rule(srs_plus())
    .with_allow_hold(false)
    .with_exact_pieces(Some(1))
    .with_supply_window_size(SupplyWindowSize::new(1))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_workers(1)
            .with_tablebase_requested(false)
            .with_exact_legal_board_enabled(false)
            .with_conditioned_reachability_enabled(false),
    );
    let problem = ProblemCompiler::compile_scenario_percent(&query).unwrap();
    let expected =
        WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default()).unwrap();
    assert_eq!(expected.usize_field("packing_candidate_count"), Some(1));
    assert_eq!(expected.coverage_pattern_words(), &[1]);

    let mut session = WasmExactSearchSession::new(&problem).unwrap();
    let original_control = ExecutionControl::default();
    session
        .begin_internal_observation_family(&original_control)
        .unwrap();
    // Geometry has consumed the only candidate, but its physical verification
    // has not committed coverage. Cancellation here needs no timing race or
    // production test hook and exercises the actual verifier cancellation.
    let candidate = loop {
        match session.geometry.advance(&session.catalog) {
            GeometryAdvance::Pending => {}
            GeometryAdvance::Candidate(candidate) => break candidate,
            _ => panic!("the one-I candidate must be produced"),
        }
    };
    original_control.cancellation.handle().cancel();
    session.execution_control = original_control.clone();
    assert!(matches!(
        session
            .process_candidate(candidate, &original_control)
            .unwrap(),
        Some(ExactSearchAdvance::Cancelled),
    ));
    assert_eq!(session.packing_candidate_count, 1);

    let resumed = ExecutionControl::default();
    let result = loop {
        match session.advance(64, &resumed).unwrap() {
            ExactSearchAdvance::Pending => {}
            ExactSearchAdvance::Completed(result) => break result,
            ExactSearchAdvance::Cancelled => panic!("the replacement control is not cancelled"),
        }
    };
    assert_eq!(result.coverage_pattern_words(), &[0]);
    assert_eq!(result.bool_field("probability_complete"), Some(false));
    assert_eq!(result.bool_field("count_complete"), Some(false));
    assert_eq!(
        session.observation_family_production,
        super::ObservationFamilyProduction::ExternalOrPartial,
    );
}
