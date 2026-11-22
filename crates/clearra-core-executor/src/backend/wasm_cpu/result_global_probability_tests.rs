use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
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

use crate::{PcGlobalProbabilityCompletion, WasmCpuSearchBackend};

fn query(queue: PcQueueInput, workers: usize) -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0xfc3f_0fc3_f0),
        queue,
        PieceWindow::new(4),
    )
    .with_rule(srs_plus())
    .with_allow_hold(false)
    .with_exact_pieces(Some(4))
    .with_supply_window_size(SupplyWindowSize::new(4))
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
fn global_factorized_partial_coverage_exhausts_serial_and_parallel_candidate_domains() {
    let source = PcQueueInput::pattern_expression(QueuePatternExpression::parse("P4", 0).unwrap());
    let reference = ProblemCompiler::compile_scenario_percent(&query(source.clone(), 1)).unwrap();
    let expected =
        WasmCpuSearchBackend::execute_with_control(&reference, &ExecutionControl::default())
            .unwrap();
    let count = expected.usize_field("covered_pattern_count").unwrap();
    assert!(count > 0 && count < 840);
    assert_eq!(expected.bool_field("count_complete"), Some(true));

    for workers in [1, 2] {
        let global = ProblemCompiler::compile_scenario_percent(&query(source.clone(), workers))
            .unwrap()
            .with_pc_global_probability_v1_evidence();
        let control = ExecutionControl::default();
        let result = WasmCpuSearchBackend::execute_with_control(&global, &control).unwrap();
        assert!(!control.is_cancelled());
        assert_eq!(
            result.coverage_pattern_words(),
            expected.coverage_pattern_words()
        );
        assert_eq!(result.usize_field("workers_used"), Some(workers));
        assert_eq!(result.bool_field("probability_complete"), Some(true));
        assert_eq!(result.bool_field("count_complete"), Some(false));
        assert_eq!(result.bool_field("enumeration_complete"), Some(true));
        assert!(result.normalized_solution_identities().is_empty());
        assert!(result.pc_chance_coverage_evidence().is_none());
        assert_eq!(
            result
                .pc_global_probability_evidence()
                .unwrap()
                .completion(),
            PcGlobalProbabilityCompletion::CandidateDomainExhausted,
        );
    }
}

#[test]
fn global_full_coverage_finishes_normally_without_count_or_cancellation_authority() {
    let source = PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 4]));
    for workers in [1, 2] {
        let global = ProblemCompiler::compile_scenario_percent(&query(source.clone(), workers))
            .unwrap()
            .with_pc_global_probability_v1_evidence();
        let control = ExecutionControl::default();
        let result = WasmCpuSearchBackend::execute_with_control(&global, &control).unwrap();
        assert!(
            !control.is_cancelled(),
            "positive completion is not cancellation"
        );
        assert_eq!(result.coverage_pattern_words(), &[1]);
        assert_eq!(result.bool_field("probability_complete"), Some(true));
        assert_eq!(result.bool_field("count_complete"), Some(false));
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
        assert!(result.path_steps().is_empty());
        assert!(result.representative_solution_identity().is_none());
        assert_eq!(
            result
                .pc_global_probability_evidence()
                .unwrap()
                .completion(),
            PcGlobalProbabilityCompletion::AllOriginalQueuesCovered,
        );
    }
}

#[test]
fn global_resource_stop_or_cancellation_cannot_seal_completion() {
    let source = PcQueueInput::pattern_expression(QueuePatternExpression::parse("P4", 0).unwrap());
    let query = query(source, 1).with_execution_policy(
        PcExecutionPolicy::default()
            .with_workers(1)
            .with_tablebase_requested(false)
            .with_max_nodes(1),
    );
    let global = ProblemCompiler::compile_scenario_percent(&query)
        .unwrap()
        .with_pc_global_probability_v1_evidence();
    let result =
        WasmCpuSearchBackend::execute_with_control(&global, &ExecutionControl::default()).unwrap();
    assert_eq!(result.bool_field("resource_truncated"), Some(true));
    assert_eq!(result.bool_field("probability_complete"), Some(false));
    assert!(result.pc_global_probability_evidence().is_none());

    let control = ExecutionControl::default();
    control.cancellation.handle().cancel();
    assert!(matches!(
        WasmCpuSearchBackend::execute_with_control(&global, &control),
        Err(crate::WasmCpuSearchError::Cancelled),
    ));
}
