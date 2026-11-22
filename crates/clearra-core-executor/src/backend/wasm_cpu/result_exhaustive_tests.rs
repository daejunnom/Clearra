use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::{
    queue::{fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression},
    QueueObservationPolicy,
};

use crate::WasmCpuSearchBackend;

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
    // Full6L uses Unique + CountUnique. The default All objective instead
    // retains a traversal-order digest, and ordinary parallel product seeds
    // need not follow the serial emission order. Require the actual Unique
    // multiset-digest contract before comparing it across worker counts.
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
