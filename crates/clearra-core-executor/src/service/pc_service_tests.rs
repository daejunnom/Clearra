#![cfg_attr(not(feature = "native-c-core"), allow(dead_code, unused_imports))]

use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::ProblemCompiler;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::service::pc_service::PcService;

#[cfg(feature = "native-c-core")]
#[test]
fn scenario_result_exposes_terminal_supply_and_explicit_solution_availability() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0x1c0701c07),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::S,
            PieceKind::T,
            PieceKind::O,
            PieceKind::I,
            PieceKind::L,
            PieceKind::J,
            PieceKind::Z,
        ])),
        PieceWindow::new(7),
    )
    .with_exact_pieces(Some(7));
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let result = PcService::execute(&problem).expect("execution");
    let availability = result.execution_report().solution_set_availability();

    assert_eq!(result.field("search_output_policy"), Some("trace"));
    assert_eq!(
        result.field("supply_window_resolution"),
        Some("projected-terminal-lookahead")
    );
    assert_eq!(result.bool_field("projects_unplaced_lookahead"), Some(true));
    assert_eq!(
        result.bool_field("projects_standard_bag_lookahead"),
        Some(false)
    );
    assert_eq!(result.usize_field("source_sequence_length"), Some(7));
    assert_eq!(result.field("total_possible_pattern_count"), Some("1"));
    assert!(availability.uses_explicit_contract());
    assert!(availability.contract_valid());
    assert!(availability.solution_count_calculated());
    assert!(availability.solution_set_materialized());
    assert_eq!(
        availability.solution_keys_materialized_count(),
        result.normalized_solution_keys().len()
    );
    assert!(availability.solution_keys_complete());
    assert!(!availability.solution_page_available());
}

#[cfg(feature = "native-c-core")]
mod case_pc_service_runs_search_problem_through_packing_buildup_coverage_and_output_model {
    use super::*;

    #[test]
    fn pc_service_runs_search_problem_through_packing_buildup_coverage_and_output_model() {
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
        let result = PcService::execute(&problem).expect("execution");
        let fields = result.summary_fields();

        assert!(fields.contains(&("problem_layer".to_owned(), "clearra-problem".to_owned())));
        assert!(fields.contains(&(
            "executor_layer".to_owned(),
            "clearra-core-executor".to_owned()
        )));
        assert!(fields.contains(&("packing_runner".to_owned(), "PackingRunner::run".to_owned())));
        assert!(fields.contains(&("buildup_runner".to_owned(), "BuildUpRunner::run".to_owned())));
        assert!(fields.contains(&(
            "rust_objective_reducer".to_owned(),
            "ObjectiveReducer::reduce".to_owned()
        )));
        assert!(fields.contains(&(
            "rust_output_model".to_owned(),
            "CoreExecutionResult".to_owned()
        )));
        assert!(fields.contains(&(
            "solver_backend".to_owned(),
            expected_solver_backend().to_owned()
        )));
        assert!(fields.contains(&(
            "packing_execution_source".to_owned(),
            expected_packing_execution_source().to_owned()
        )));
        assert!(fields.contains(&(
            "buildup_execution_source".to_owned(),
            expected_buildup_execution_source().to_owned()
        )));
        assert!(fields.contains(&(
            "native_c_core_executed".to_owned(),
            expected_native_c_core_executed().to_owned()
        )));
        assert!(fields.contains(&(
            "native_c_core_fallback_policy".to_owned(),
            expected_native_c_core_fallback_policy().to_owned()
        )));
        assert!(fields.contains(&("chain_class".to_owned(), "opening-2l".to_owned())));
        assert!(fields.contains(&("chain_labels".to_owned(), "2L".to_owned())));
        assert!(fields.contains(&(
            "exact_target_policy".to_owned(),
            "2L-label-clear-to-empty".to_owned()
        )));
        assert!(fields.contains(&(
            "checkpoint_results".to_owned(),
            "not-executed-label-metadata".to_owned()
        )));
        assert!(fields.contains(&(
            "checkpoint_schedule_source".to_owned(),
            "clearra-pc-graph-labels".to_owned()
        )));
        assert!(fields.contains(&("checkpoint_schedule_partitions".to_owned(), "2".to_owned())));
        assert!(fields.contains(&(
            "compact_piece_source_kind".to_owned(),
            clearra_core_ffi::supply::C_PIECE_SOURCE_FIXED_QUEUE.to_string()
        )));
        let compact_supply_provenance_id = field_value(&fields, "compact_supply_provenance_id")
            .and_then(|value| value.parse::<u32>().ok())
            .expect("compact supply provenance id");
        assert_ne!(compact_supply_provenance_id, 0);
        assert!(fields.contains(&(
            "gpu_backend_scope".to_owned(),
            "native-gpu-packing".to_owned()
        )));
        assert!(fields.contains(&("gpu_larger_batch_planner".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_dominance_prefilter".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_shape_union_mask".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_readback_compression".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_result_deterministic".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_backend_available".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_result_cpu_confirmed".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("gpu_cpu_reference_match".to_owned(), "false".to_owned())));
        assert!(fields.contains(&("hybrid_scheduler".to_owned(), "false".to_owned())));
        assert!(fields.contains(&(
            "hybrid_gpu_readback_cpu_buildup_overlap".to_owned(),
            "false".to_owned()
        )));
        assert!(fields.contains(&(
            "hybrid_backend_metrics_reported".to_owned(),
            "false".to_owned()
        )));
        assert!(fields.contains(&(
            "hybrid_memory_leak_report_clean".to_owned(),
            "true".to_owned()
        )));
        assert!(fields.contains(&("coverage_pattern_count".to_owned(), "1".to_owned())));
        assert!(fields.contains(&(
            "covered_pattern_count".to_owned(),
            expected_covered_pattern_count().to_owned()
        )));
        assert!(fields.contains(&(
            "probability_complete".to_owned(),
            expected_probability_complete().to_owned()
        )));
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
}

#[cfg(feature = "native-c-core")]
mod case_product_acceptance_opening_2l_empty_fixture_uses_full_solver_flow {
    use super::*;

    #[test]
    fn product_acceptance_opening_2l_empty_fixture_uses_full_solver_flow() {
        assert!(
            include_str!("../../../../tests/fixtures/pc/opening_2l_empty.json")
                .contains("opening_2l_empty")
        );
        assert!(
            include_str!("../../../../tests/golden/pc/opening_2l_empty.json")
                .contains("packing_candidate_is_solution=false")
        );

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
        let result = PcService::execute(&problem).expect("execution");

        assert!(result.solution_found());
        assert_eq!(result.field("problem_preset"), Some("opening-pc"));
        assert_eq!(result.field("compiled_goal"), Some("clear-to-empty"));
        assert_eq!(result.field("compiled_piece_window"), Some("5"));
        assert_eq!(result.field("compiled_exact_pieces"), Some("5"));
        assert_eq!(
            result.field("compiled_initial_board_mask"),
            Some("0x0000000000000000")
        );
        assert_eq!(result.field("packing_candidate_is_solution"), Some("false"));
        assert_eq!(result.field("coverage_result"), Some("rust-coverage"));
        assert_eq!(
            result.field("objective_result"),
            Some("rust-objective-reducer")
        );
        assert_eq!(
            result.field("coverage_probability"),
            Some(expected_coverage_probability())
        );
        assert_eq!(
            result.field("count_complete"),
            Some(expected_probability_complete())
        );
        assert!(
            result
                .execution_report()
                .objective_result()
                .total_solution_count()
                > 0
        );
    }
}

#[cfg(feature = "native-c-core")]
mod case_product_acceptance_opening_4l_fixture_compiles_deterministic_schedule {
    use super::*;

    #[test]
    fn product_acceptance_opening_4l_fixture_compiles_deterministic_schedule() {
        let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

        assert_eq!(problem.preset().as_str(), "opening-pc");
        assert_eq!(
            problem.exact_target_policy().target(),
            Some(PcTarget::four_lines())
        );
        assert_eq!(problem.piece_window().max_pieces(), 10);
        assert_eq!(problem.exact_pieces(), Some(10));
        let schedule = problem.checkpoint_schedule().expect("4L schedule");
        assert_eq!(schedule.label(), "4L");
        assert_eq!(schedule.partition_labels(), vec!["4", "2+2"]);
        assert_eq!(schedule.checkpoint_count(), 3);
    }
}

mod case_product_acceptance_continuation_fixture_exports_next_pc_token_after_2l {
    use super::*;

    #[test]
    fn product_acceptance_continuation_fixture_exports_next_pc_token_after_2l() {
        assert!(include_str!(
            "../../../../tests/fixtures/continuation/pc_then_next_pc_available.json"
        )
        .contains("pc_then_next_pc_available"));
        assert!(
            include_str!("../../../../tests/golden/continuation/next_pc_available.json")
                .contains("continuation_token_version=pc2")
        );

        let fixed_pieces = vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
            PieceKind::O,
        ];
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(
                fixed_pieces.clone(),
            )))
            .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
        let pc_query = problem
            .scenario()
            .pc_query()
            .expect("opening problem preserves pc query");
        let fields = crate::service::pc_continuation_fields::opening_continuation_fields(
            pc_query,
            Some(&fixed_pieces),
            5,
        );

        assert_eq!(field_value(&fields, "remaining_queue_len"), Some("5"));
        assert_eq!(
            field_value(&fields, "remaining_queue_preview"),
            Some("IIOOO")
        );
        assert_eq!(field_value(&fields, "next_pc_available"), Some("true"));
        assert_eq!(field_value(&fields, "next_pc_candidate"), Some("2L"));
        assert_eq!(
            field_value(&fields, "continuation_token_available"),
            Some("true")
        );
        assert_eq!(
            field_value(&fields, "continuation_token_version"),
            Some("pc2")
        );
        assert!(field_value(&fields, "continuation_token")
            .is_some_and(|token| token.starts_with("pc2:l2:")));
    }
}

#[cfg(feature = "native-c-core")]
mod case_product_acceptance_scenario_simple_4l_fixture_solves_visible_tall_board {
    use super::*;
    use clearra_core_domain::solution::normalized_tiling_solution::{
        NormalizedTilingSolutionKey, NormalizedTilingSolutionSet, PiecePlacementMask,
    };

    #[test]
    fn product_acceptance_scenario_simple_4l_fixture_solves_visible_tall_board() {
        assert!(
            include_str!("../../../../tests/fixtures/pc/scenario_simple_4l.json")
                .contains("scenario_simple_4l")
        );
        assert!(
            include_str!("../../../../tests/golden/pc/scenario_simple_4l.json")
                .contains("scenario_replay_token_version=sr2")
        );

        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_retained_trace_limit(1);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        let result = PcService::execute(&problem).expect("execution");

        assert!(result.solution_found());
        assert_eq!(result.field("problem_preset"), Some("scenario-pc"));
        assert_eq!(result.field("board_width"), Some("10"));
        assert_eq!(result.field("visible_height"), Some("4"));
        assert_eq!(
            result.field("initial_board_mask"),
            Some("0x00000000000003f0")
        );
        assert_eq!(result.field("piece_window"), Some("1"));
        assert_eq!(result.field("exact_pieces"), Some("1"));
        assert_eq!(
            result.field("actual_solution_set_contract"),
            Some("normalized-tiling-set")
        );
        assert_eq!(result.field("normalized_unique_solution_count"), Some("1"));
        let independent_key = NormalizedTilingSolutionKey::from_placements(
            0x3f0,
            [PiecePlacementMask::new(PieceKind::I, 0x0f)],
        )
        .expect("one horizontal I fills the only four empty cells");
        assert_eq!(
            independent_key.as_str(),
            "ctk1|initial=00000000000003f0|placements=I:000000000000000f"
        );
        let independent_set = NormalizedTilingSolutionSet::new([independent_key.clone()]);
        let independent_hash = independent_set.hash();
        assert_eq!(
            result
                .execution_report()
                .objective_result()
                .total_solution_count(),
            1,
            "an empty hold cannot store the final current without a next piece"
        );
        assert_eq!(
            result.normalized_solution_keys(),
            &[independent_key.as_str().to_owned()]
        );
        assert_eq!(
            result.field("normalized_solution_set_hash"),
            Some(independent_hash)
        );
        assert_eq!(result.path_steps().len(), 1);
        assert_eq!(result.path_steps()[0].piece(), PieceKind::I);
        assert_eq!(result.path_steps()[0].hold(), "none");
        assert_eq!(
            result.field("coverage_probability"),
            Some(expected_coverage_probability())
        );
        assert_eq!(result.field("scenario_replay_token_version"), Some("sr2"));
        assert!(result
            .field("scenario_replay_token")
            .is_some_and(|token| token.starts_with("sr2:w10:v4:")));
    }
}

#[cfg(feature = "native-c-core")]
mod case_completed_initial_row_pc_equivalence {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use crate::service::pc_service::PcService;

    #[test]
    fn raw_completed_row_matches_normalized_result_and_ctk_initial_board() {
        let query = |board| {
            PcScenarioQuery::new(
                board,
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                    PieceKind::O,
                    PieceKind::I,
                    PieceKind::I,
                ])),
                PieceWindow::new(3),
            )
            .with_exact_pieces(Some(3))
            .with_retained_trace_limit(1)
        };
        let raw = query(PcScenarioBoard::standard_10(2, 0x0000_0000_0003_ffff));
        let normalized = query(PcScenarioBoard::standard_10(2, 0xff));
        let raw_problem = ProblemCompiler::compile_scenario_pc(&raw).expect("raw problem");
        let normalized_problem =
            ProblemCompiler::compile_scenario_pc(&normalized).expect("normalized problem");

        assert_eq!(raw_problem, normalized_problem);
        let raw_result = PcService::execute(&raw_problem).expect("raw execution");
        let normalized_result =
            PcService::execute(&normalized_problem).expect("normalized execution");

        assert_eq!(raw_result, normalized_result);
        assert_eq!(raw_result.field("visible_height"), Some("2"));
        assert_eq!(
            raw_result.field("initial_board_mask"),
            Some("0x00000000000000ff")
        );
        assert!(!raw_result.normalized_solution_keys().is_empty());
        assert!(raw_result
            .normalized_solution_keys()
            .iter()
            .all(|key| key.starts_with("ctk1|initial=00000000000000ff|placements=")));
    }
}

#[cfg(not(feature = "native-c-core"))]
mod case_pc_service_preserves_scenario_fixture_trace_key_contract {
    use super::*;

    #[cfg(not(feature = "native-c-core"))]
    #[test]
    fn pc_service_rejects_execution_when_native_runtime_is_unavailable() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_retained_trace_limit(1);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        assert_eq!(
            PcService::execute(&problem),
            Err(crate::service::pc_service::PcServiceError::Packing(
                crate::packing::PackingRunnerError::BackendExecutorUnavailable {
                    backend: crate::backend::SelectedSearchBackend::CpuGeometryExactCover,
                    reason: "native_geometry_exact_cover_not_connected",
                }
            ))
        );
    }
}

#[cfg(feature = "native-c-core")]
mod case_pc_service_native_scenario_uses_native_trace_key {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use crate::service::pc_service::PcService;

    #[cfg(feature = "native-c-core")]
    #[test]
    fn pc_service_native_scenario_uses_native_trace_key() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0x3f0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_retained_trace_limit(1);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
        let result = PcService::execute(&problem).expect("execution");

        assert!(result.solution_found());
        assert!(result.summary_fields().contains(&(
            "retained_trace_key_source".to_owned(),
            "native-c-core".to_owned()
        )));
    }
}

fn expected_solver_backend() -> &'static str {
    "core-c-cpu-packing-cpu-buildup"
}

fn expected_packing_execution_source() -> &'static str {
    "native-cpu-packing"
}

fn expected_buildup_execution_source() -> &'static str {
    "native-cpu-buildup"
}

fn expected_native_c_core_executed() -> &'static str {
    "true"
}

fn expected_native_c_core_fallback_policy() -> &'static str {
    "native-required-no-fallback"
}

fn expected_coverage_probability() -> &'static str {
    "1.0"
}

fn expected_covered_pattern_count() -> &'static str {
    "1"
}

fn expected_probability_complete() -> &'static str {
    "true"
}

fn field_value<'a>(fields: &'a [(String, String)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find(|(field_key, _)| field_key == key)
        .map(|(_, value)| value.as_str())
}

#[cfg(feature = "native-c-core")]
mod case_pc_service_hands_replay_seed_to_app_post_processing_without_running_scoring {
    use super::*;

    #[test]
    fn pc_service_hands_replay_seed_to_app_post_processing_without_running_scoring() {
        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled)
            .with_objective(ObjectivePolicy::all().with_score_summary());
        let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
        let result = PcService::execute(&problem).expect("execution");
        let fields = result.summary_fields();

        assert_eq!(result.field("score_post_processing"), None);
        assert_eq!(result.field("score_event_basis"), None);
        assert!(fields.contains(&(
            "postprocess_scoring_requested".to_owned(),
            "true".to_owned()
        )));
        assert!(fields.contains(&(
            "postprocess_execution_owner".to_owned(),
            "clearra-app->clearra-postprocess".to_owned()
        )));
        assert!(result.postprocess_replay_trace().is_some());
        assert_eq!(result.field("objective_best_score_by_pattern_count"), None);
    }
}
