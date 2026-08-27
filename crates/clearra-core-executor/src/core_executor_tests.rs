#![cfg_attr(not(feature = "native-c-core"), allow(dead_code, unused_imports))]

use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::ProblemCompiler;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::*;

#[cfg(feature = "native-c-core")]
#[test]
fn core_executor_routes_opening_problem_to_pc_service() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
    let result = CoreExecutor::execute(&problem).expect("execution");
    let fields = result.summary_fields();

    assert!(fields.contains(&("problem_layer".to_owned(), "clearra-problem".to_owned())));
    assert!(fields.contains(&(
        "executor_layer".to_owned(),
        "clearra-core-executor".to_owned()
    )));
    assert!(fields.contains(&(
        "solver_backend".to_owned(),
        expected_solver_backend().to_owned()
    )));
    assert!(fields.contains(&("chain_class".to_owned(), "opening-2l".to_owned())));
    assert!(fields.contains(&(
        "checkpoint_schedule_source".to_owned(),
        "clearra-pc-graph-labels".to_owned()
    )));
    assert!(fields.contains(&("checkpoint_schedule_partitions".to_owned(), "2".to_owned())));
    assert!(fields.contains(&(
        "coverage_probability".to_owned(),
        expected_coverage_probability().to_owned()
    )));
    assert_eq!(
        result
            .execution_report()
            .backend_report()
            .backend_requested(),
        "auto"
    );
}

#[cfg(not(feature = "native-c-core"))]
#[test]
fn default_build_reports_native_runtime_unavailable() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_retained_trace_limit(1);
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    assert_eq!(
        CoreExecutor::execute(&problem),
        Err(CoreExecutionError::RuntimeUnavailable {
            component: "core_c_packing_runtime_unavailable"
        })
    );
}

#[cfg(feature = "native-c-core")]
#[test]
fn scenario_pc_native_result_uses_native_trace_key() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_retained_trace_limit(1);
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let result = CoreExecutor::execute(&problem).expect("execution");

    assert!(result.solution_found());
    assert!(result.summary_fields().contains(&(
        "retained_trace_key_source".to_owned(),
        "native-c-core".to_owned()
    )));
}

#[cfg(feature = "native-c-core")]
#[test]
fn scenario_coverage_summary_routes_to_percent_service() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_exact_pieces(Some(1))
    .with_retained_trace_limit(0);
    let problem = ProblemCompiler::compile_scenario_percent(&query).expect("problem");
    let result = CoreExecutor::execute(&problem).expect("percent execution");

    assert_eq!(result.field("status"), Some("percent-executed"));
    assert_eq!(
        result.field("coverage_reducer"),
        Some("pattern-bitset-union")
    );
    assert_eq!(result.coverage_pattern_words(), &[1]);
    assert_eq!(
        result.field("search_output_policy"),
        Some("coverage-summary")
    );
    assert_eq!(
        result.field("unique_solution_count"),
        Some("not-calculated")
    );
    assert_eq!(result.bool_field("solution_count_calculated"), Some(false));
    assert_eq!(result.bool_field("solution_set_materialized"), Some(false));
}

#[cfg(feature = "native-c-core")]
#[test]
fn opening_coverage_summary_routes_to_percent_service() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ])))
        .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
    let problem = ProblemCompiler::compile_opening_percent(&query).expect("problem");
    let result = CoreExecutor::execute(&problem).expect("percent execution");

    assert_eq!(result.field("status"), Some("percent-executed"));
    assert_eq!(result.field("problem_preset"), Some("opening-pc"));
    assert_eq!(result.field("probability_complete"), Some("true"));
    assert_eq!(result.coverage_pattern_words(), &[1]);
    assert_eq!(
        result.field("search_output_policy"),
        Some("coverage-summary")
    );
    assert_eq!(
        result.field("unique_solution_count"),
        Some("not-calculated")
    );
    assert_eq!(result.bool_field("solution_count_calculated"), Some(false));
    assert_eq!(result.bool_field("solution_keys_complete"), Some(false));
}

fn expected_solver_backend() -> &'static str {
    "core-c-cpu-packing-cpu-buildup"
}

fn expected_coverage_probability() -> &'static str {
    "1.0"
}

#[cfg(not(feature = "native-c-core"))]
#[test]
fn core_executor_routes_build_coverage_query_to_cover_service() {
    use clearra_build_coverage::{
        domain::slot_domain::SlotDomain,
        query::{
            build_coverage_limits::BuildCoverageLimits, build_coverage_query::BuildCoverageQuery,
        },
        template::{BuildSlot, BuildSlotId, BuildTemplate},
    };
    use clearra_core_domain::{board::board_size::BoardSize, board::cell::CellCoord};
    use clearra_problem::{BuildProblemLimits, BuildQuery, BuildTemplateBridge};

    let board = BoardSize::new(10, 4).expect("board");
    let bridge_query = BuildQuery::coverage_bridge(
        BuildTemplateBridge::new("template-a", board, 1),
        4,
        BuildProblemLimits::new(12, 4),
    );
    let problem = ProblemCompiler::compile_build(&bridge_query).expect("problem");
    let slot = BuildSlotId::new(1);
    let coverage_query = BuildCoverageQuery::new(
        BuildTemplate::new(
            "template-a",
            vec![BuildSlot::new(
                slot,
                vec![CellCoord::new(0, 0, board).expect("cell")],
            )],
        )
        .with_board_size(board),
        vec![SlotDomain::new(slot, vec![PieceKind::I])],
        Vec::new(),
        4,
        BuildCoverageLimits::new(12, 4),
    );

    assert_eq!(
        CoreExecutor::execute_build_coverage(&problem, &coverage_query),
        Err(CoreExecutionError::RuntimeUnavailable {
            component: "core_c_packing_runtime_unavailable"
        })
    );
}
