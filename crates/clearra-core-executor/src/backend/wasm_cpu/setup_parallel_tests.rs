use super::*;
#[cfg(not(target_family = "wasm"))]
use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_problem::{
    compile_setup_search_conditions, SetupCandidatePriority, SetupHoldPolicy,
    SetupLengthPreference, SetupLimits, SetupPathDetail,
};
use clearra_rules::profile::builtin_rules::{jstris_180, srs_x};
#[cfg(not(target_family = "wasm"))]
use std::sync::Arc;

use super::super::{
    setup_coverage_graph::{SetupCoverageGraph, SetupCoverageNode},
    setup_graph_builder::{clear_setup_graph_cache_for_test, install_setup_graph_cache_for_test},
    setup_partial_build::{PartialBuildGraph, SetupShape},
};

#[test]
fn parallel_setup_defers_exact_task_metadata_until_graph_preparation_advances() {
    let query = SetupSearchQuery::default().with_remaining_pieces(vec![
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
    ]);
    let mut coordinator =
        WasmSetupParallelCoordinator::new(&query, 4).expect("lazy setup coordinator");

    assert!(coordinator.tasks.is_empty());
    assert!(coordinator.pending_results.is_empty());
    assert_eq!(coordinator.task_count(), 3);
    assert!(matches!(
        coordinator
            .advance(1, 1, &ExecutionControl::default())
            .expect("first preparation batch"),
        WasmSetupParallelProduce::Pending
    ));
    assert!(
        coordinator.tasks.is_empty(),
        "task ranges require exact word counts and must not be guessed during construction"
    );
}

#[test]
#[cfg(not(target_family = "wasm"))]
fn default_setup_preparation_yields_progress_before_pattern_materialization_finishes() {
    let query = SetupSearchQuery::default().with_tablebase_requested(false);
    let token = ExecutionCancellationToken::new();
    let handle = token.handle();
    let control = ExecutionControl::new(token);
    let mut coordinator =
        WasmSetupParallelCoordinator::new(&query, 4).expect("lazy default setup coordinator");

    let initial = coordinator.build_progress();
    let mut observed = initial;
    for _ in 0..8 {
        assert!(matches!(
            coordinator
                .advance(1_024, 1, &control)
                .expect("bounded preparation batch"),
            WasmSetupParallelProduce::Pending
        ));
        observed = coordinator.build_progress();
        if observed.4 > 0 && observed.5 > observed.4 {
            break;
        }
    }

    assert_ne!(observed, initial, "preparation progress must be observable");
    assert!(
        observed.4 > 0 && observed.5 > observed.4,
        "the default multi-million-pattern expansion must remain in progress"
    );
    assert!(coordinator.tasks.is_empty());

    handle.cancel();
    assert!(matches!(
        coordinator
            .advance(1_024, 1, &control)
            .expect("cancelled preparation batch"),
        WasmSetupParallelProduce::Cancelled
    ));
}

#[test]
#[cfg(not(target_family = "wasm"))]
fn scheduled_setup_worker_matches_atomic_task_result_and_yields() {
    let graph = SetupCoverageGraph::from_wire_parts(
        vec![SetupCoverageNode::from_wire(0, 0, 0, 10, 1).expect("coverage node")],
        Vec::new(),
        0,
    )
    .expect("coverage graph");
    let shapes = vec![SetupShape::new(0, 0, 0)];
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T])
        .with_tablebase_requested(false);
    let initialization =
        encode_initialization(&query, &graph, &shapes).expect("worker initialization");
    let task = SetupParallelTask {
        task_index: 0,
        condition_index: 0,
        word_start: 0,
        word_end: 1,
    };
    let batch = encode_tasks(&[task]);
    let control = ExecutionControl::default();

    let mut atomic = WasmSetupParallelWorker::new(&initialization).expect("atomic worker");
    let expected = encode_results(&[atomic
        .consume_task(task, &control)
        .expect("atomic setup task")])
    .expect("atomic result");

    let mut scheduled = WasmSetupParallelWorker::new(&initialization).expect("scheduled worker");
    scheduled.enqueue(&batch).expect("enqueue task");
    let mut pending = 0;
    let (candidate_count, actual) = loop {
        match scheduled
            .advance_pending(1, &control)
            .expect("scheduled worker step")
        {
            WasmSetupParallelWorkerAdvance::Pending => pending += 1,
            WasmSetupParallelWorkerAdvance::Complete {
                candidate_count,
                partial,
            } => break (candidate_count, partial),
            WasmSetupParallelWorkerAdvance::Cancelled => {
                panic!("scheduled worker was not cancelled")
            }
        }
    };

    assert!(pending > 1, "one-pattern budget must yield repeatedly");
    assert_eq!(candidate_count, 1);
    assert_eq!(actual, expected);
    assert!(!scheduled.has_pending_work());
}

#[test]
#[cfg(not(target_family = "wasm"))]
fn scheduled_setup_worker_cancels_during_default_pattern_compile() {
    let graph = SetupCoverageGraph::from_wire_parts(
        vec![SetupCoverageNode::from_wire(0, 0, 0, 10, 1).expect("coverage node")],
        Vec::new(),
        0,
    )
    .expect("coverage graph");
    let shapes = vec![SetupShape::new(0, 0, 0)];
    let query = SetupSearchQuery::default().with_tablebase_requested(false);
    let initialization =
        encode_initialization(&query, &graph, &shapes).expect("worker initialization");
    let batch = encode_tasks(&[SetupParallelTask {
        task_index: 0,
        condition_index: 0,
        word_start: 0,
        word_end: 1,
    }]);
    let token = ExecutionCancellationToken::new();
    let handle = token.handle();
    let control = ExecutionControl::new(token);
    let mut worker = WasmSetupParallelWorker::new(&initialization).expect("worker");
    worker.enqueue(&batch).expect("enqueue task");

    assert!(matches!(
        worker.advance_pending(1, &control).expect("first step"),
        WasmSetupParallelWorkerAdvance::Pending
    ));
    handle.cancel();
    assert!(matches!(
        worker.advance_pending(1, &control).expect("cancel step"),
        WasmSetupParallelWorkerAdvance::Cancelled
    ));
}

#[test]
fn large_condition_mix_leaves_multiple_tasks_per_verifier() {
    let condition_words = [1_890, 473, 473, 473, 473];
    let tasks = plan_parallel_tasks(&condition_words, 11).expect("task plan");

    assert!(tasks.len() >= 30);
    assert_exact_condition_ranges(&condition_words, &tasks);
}

#[test]
fn small_conditions_are_overdecomposed_for_dynamic_tail_stealing() {
    let condition_words = [4, 32, 127];
    let tasks = plan_parallel_tasks(&condition_words, 12).expect("task plan");

    assert!(tasks.len() >= 33);
    assert_exact_condition_ranges(&condition_words, &tasks);
}

#[test]
fn standard_bag_condition_keeps_every_verifier_busy_until_the_tail() {
    let condition_words = [79];
    let tasks = plan_parallel_tasks(&condition_words, 11).expect("task plan");

    assert_eq!(tasks.len(), 40);
    assert!(tasks
        .iter()
        .all(|task| task.word_end.saturating_sub(task.word_start) <= 2));
    assert_exact_condition_ranges(&condition_words, &tasks);
}

#[test]
fn high_concurrency_plan_limits_merge_amplification() {
    let condition_words = [1_890, 473, 473, 473, 473];
    let tasks = plan_parallel_tasks(&condition_words, 41).expect("high concurrency task plan");

    assert!(tasks.len() >= 40);
    assert!(tasks.len() <= 100);
    assert_exact_condition_ranges(&condition_words, &tasks);
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn native_setup_progress_maps_graph_passes_to_stable_ui_phases() {
    assert_eq!(native_setup_build_progress_phase(0), ("setup-geometry", 1));
    assert_eq!(native_setup_build_progress_phase(1), ("setup-graph", 2));
    assert_eq!(native_setup_build_progress_phase(2), ("setup-graph", 2));
    assert_eq!(native_setup_build_progress_phase(3), ("setup-graph", 2));
}

#[cfg(not(target_family = "wasm"))]
#[test]
#[ignore = "full empty-4L serial/multiworker equivalence; run in the release acceptance suite"]
fn native_multiworker_matches_serial_setup_result() {
    let limits = SetupLimits::new(32, 32, 32, 32, 2_000_000, 32).expect("bounded setup limits");
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::I])
        .with_max_setup_pieces(1)
        .with_tablebase_requested(false)
        .with_limits(limits);
    let serial_control = ExecutionControl::new(ExecutionCancellationToken::new());
    let mut serial_session =
        super::super::WasmSetupSearchSession::new(&query).expect("serial setup session");
    let serial = loop {
        match serial_session
            .advance(8_192, &serial_control)
            .expect("serial setup advance")
        {
            super::super::WasmSetupSearchAdvance::Pending => {}
            super::super::WasmSetupSearchAdvance::Completed(result) => break result,
            super::super::WasmSetupSearchAdvance::Cancelled => {
                panic!("serial setup was not cancelled")
            }
        }
    };
    let parallel = execute_setup_parallel_native(
        &query,
        3,
        &ExecutionControl::new(ExecutionCancellationToken::new()),
    )
    .expect("native parallel setup");

    assert_eq!(
        serial.field("normalized_solution_set_hash"),
        parallel.field("normalized_solution_set_hash")
    );
    assert_eq!(
        serial
            .setup_finder_report()
            .expect("serial setup report")
            .hold_conditions(),
        parallel
            .setup_finder_report()
            .expect("parallel setup report")
            .hold_conditions()
    );
    assert!(parallel
        .usize_field("workers_used")
        .is_some_and(|workers| workers > 1));
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn native_two_worker_cached_path_detail_matches_single_worker_result() {
    fn run_serial(
        query: &SetupSearchQuery,
        control: &ExecutionControl,
    ) -> crate::CoreExecutionResult {
        let mut session =
            super::super::WasmSetupSearchSession::new(query).expect("single-worker setup session");
        loop {
            match session
                .advance(8_192, control)
                .expect("single-worker setup advance")
            {
                super::super::WasmSetupSearchAdvance::Pending => {}
                super::super::WasmSetupSearchAdvance::Completed(result) => return result,
                super::super::WasmSetupSearchAdvance::Cancelled => {
                    panic!("single-worker setup was not cancelled")
                }
            }
        }
    }

    clear_setup_graph_cache_for_test();
    let query = SetupSearchQuery::default()
        .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T])
        .with_hold_policy(SetupHoldPolicy::Disabled)
        .with_tablebase_requested(false);
    let condition = compile_setup_search_conditions(&query)
        .expect("cached setup condition")
        .into_iter()
        .next()
        .expect("one cached setup condition");
    let detail = SetupPathDetail::new(1, 0, 1, condition.condition_id())
        .expect("synthetic cached path detail");
    let graph = Arc::new(PartialBuildGraph::cached_path_detail_fixture(&detail));
    let coverage_graph =
        Arc::new(SetupCoverageGraph::compile(&graph).expect("synthetic cached coverage graph"));
    let candidate = SetupCandidateReport::new(
        detail.setup_id(),
        detail.board_mask(),
        0,
        0,
        1,
        1,
        "1.0".to_owned(),
        "1.0".to_owned(),
        "1.0".to_owned(),
        Vec::new(),
    );
    install_setup_graph_cache_for_test(
        &query,
        graph,
        coverage_graph,
        vec![CompletedSetupCoverage {
            report: SetupHoldConditionReport::new(
                condition.condition_id().to_owned(),
                condition.initial_hold(),
                condition.pattern_expression().to_owned(),
                1,
                1,
                false,
                true,
                vec![candidate],
            ),
            candidate_boards: vec![detail.board_mask()],
            observation_workers_used: 1,
        }],
    );
    let control = ExecutionControl::new(ExecutionCancellationToken::new());
    let detail_query = query.with_path_detail(detail);

    let single_worker = run_serial(&detail_query, &control);
    let two_workers = execute_setup_parallel_native(&detail_query, 2, &control)
        .expect("two-worker cached setup detail");

    for field in [
        "normalized_solution_set_hash",
        "parallel",
        "parallel_decision_reason",
        "workers_used",
    ] {
        assert_eq!(
            single_worker.field(field),
            two_workers.field(field),
            "{field}"
        );
    }
    assert_eq!(
        single_worker
            .setup_finder_report()
            .expect("single-worker detail report")
            .hold_conditions(),
        two_workers
            .setup_finder_report()
            .expect("two-worker detail report")
            .hold_conditions()
    );
    clear_setup_graph_cache_for_test();
}

#[test]
fn result_wire_preserves_qb_conditioned_setup_depth_range() {
    let encoded = encode_results(&[SetupParallelTaskResult {
        task_index: 2,
        condition_index: 1,
        word_start: 4,
        word_end: 7,
        global_pattern_count: 5040,
        covered_shapes: vec![SetupParallelShapeResult {
            shape_index: 9,
            build_covered_patterns: 128,
            joint_covered_patterns: 96,
            build_weight: 0.8,
            joint_weight: 0.6,
            min_covered_locks: 4,
            max_covered_locks: 7,
            witness_pattern_id: 13,
        }],
        peak_segment_pages: 3,
    }])
    .expect("encode result");
    let decoded = decode_results(&encoded).expect("decode result");
    let result = &decoded[0];
    let shape = &result.covered_shapes[0];

    assert_eq!(result.task_index, 2);
    assert_eq!(shape.shape_index, 9);
    assert_eq!((shape.min_covered_locks, shape.max_covered_locks), (4, 7));
    assert_eq!(shape.witness_pattern_id, 13);
}

#[test]
fn initialization_wire_preserves_observed_qb_terminal_inventory_and_setup_piece_limit() {
    let graph = SetupCoverageGraph::from_wire_parts(
        vec![SetupCoverageNode::from_wire(0, 0, 0, 10, 1).expect("coverage node")],
        Vec::new(),
        0,
    )
    .expect("coverage graph");
    let shapes = vec![SetupShape::new(0, 0, 0)];
    let query = SetupSearchQuery::default()
        .with_rule(srs_x())
        .with_remaining_pieces(vec![PieceKind::T, PieceKind::I])
        .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::T))
        .with_queue_based_pieces(vec![PieceKind::O, PieceKind::S])
        .with_next_cycle_remaining_pieces(vec![
            PieceKind::O,
            PieceKind::O,
            PieceKind::S,
            PieceKind::I,
            PieceKind::T,
            PieceKind::Z,
        ])
        .with_max_setup_pieces(10)
        .with_tablebase_requested(true)
        .with_path_detail(SetupPathDetail::new(0, 3, 1, "hold-T").expect("exact path detail"));

    let encoded = encode_initialization(&query, &graph, &shapes).expect("encode initialization");
    let decoded = decode_initialization(&encoded).expect("decode initialization");

    assert_eq!(decoded.query.hold_policy(), query.hold_policy());
    assert_eq!(decoded.query.rule(), srs_x());
    assert_eq!(decoded.query.max_setup_pieces(), 10);
    assert!(decoded.query.tablebase_requested());
    assert_eq!(decoded.query.path_detail(), query.path_detail());
    assert_eq!(
        decoded
            .query
            .queue()
            .as_fixed_sequence()
            .expect("observed QB queue")
            .pieces(),
        query
            .queue()
            .as_fixed_sequence()
            .expect("source observed QB queue")
            .pieces()
    );
    assert_eq!(
        decoded.query.next_cycle_remaining_pieces(),
        query.next_cycle_remaining_pieces()
    );
}

#[test]
fn setup_parallel_wire_preserves_jstris_180_rule_identity() {
    let graph = SetupCoverageGraph::from_wire_parts(
        vec![SetupCoverageNode::from_wire(0, 0, 0, 10, 1).expect("coverage node")],
        Vec::new(),
        0,
    )
    .expect("coverage graph");
    let shapes = vec![SetupShape::new(0, 0, 0)];
    let query = SetupSearchQuery::default()
        .with_rule(jstris_180())
        .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T]);

    let encoded = encode_initialization(&query, &graph, &shapes).expect("encode initialization");
    let decoded = decode_initialization(&encoded).expect("decode initialization");

    assert_eq!(decoded.query.rule(), jstris_180());
}

#[test]
fn completed_condition_keeps_more_than_the_legacy_result_page() {
    const SHAPE_COUNT: usize = 300;

    let shapes = (0..SHAPE_COUNT)
        .map(|index| SetupShape::new(index as u64 + 1, index as u32, 0))
        .collect::<Vec<_>>();
    let covered_shapes = (0..SHAPE_COUNT)
        .map(|index| SetupParallelShapeResult {
            shape_index: index as u32,
            build_covered_patterns: 1,
            joint_covered_patterns: 1,
            build_weight: 1.0,
            joint_weight: 1.0,
            min_covered_locks: 1,
            max_covered_locks: 1,
            witness_pattern_id: 0,
        })
        .collect();
    let mut merge = SetupConditionMerge::new(SHAPE_COUNT, 1, 1).expect("condition merge");
    merge
        .absorb(
            SetupParallelTaskResult {
                task_index: 0,
                condition_index: 0,
                word_start: 0,
                word_end: 1,
                global_pattern_count: 1,
                covered_shapes,
                peak_segment_pages: 1,
            },
            SHAPE_COUNT,
        )
        .expect("condition result");

    let completed = merge
        .finish(
            &shapes,
            SetupCandidatePriority::All,
            SetupLengthPreference::Auto,
            None,
            &ExecutionControl::default(),
        )
        .expect("completed condition");

    assert_eq!(completed.candidate_count, SHAPE_COUNT);
    assert_eq!(completed.selected_shapes.len(), SHAPE_COUNT);
    assert_eq!(completed.candidate_boards.len(), SHAPE_COUNT);
}

#[test]
fn zero_word_condition_finishes_as_complete_empty_result() {
    let completed = SetupConditionMerge::new(0, 0, 37)
        .expect("zero-task condition merge")
        .finish(
            &[],
            SetupCandidatePriority::All,
            SetupLengthPreference::Auto,
            None,
            &ExecutionControl::default(),
        )
        .expect("zero-task condition completion");

    assert_eq!(completed.global_pattern_count, 37);
    assert_eq!(completed.candidate_count, 0);
    assert!(completed.candidate_boards.is_empty());
    assert!(completed.selected_shapes.is_empty());
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn completed_condition_honors_cancellation_during_finalize() {
    let shapes = vec![SetupShape::new(1, 0, 0)];
    let mut merge = SetupConditionMerge::new(1, 1, 1).expect("condition merge");
    merge
        .absorb(
            SetupParallelTaskResult {
                task_index: 0,
                condition_index: 0,
                word_start: 0,
                word_end: 1,
                global_pattern_count: 1,
                covered_shapes: vec![SetupParallelShapeResult {
                    shape_index: 0,
                    build_covered_patterns: 1,
                    joint_covered_patterns: 1,
                    build_weight: 1.0,
                    joint_weight: 1.0,
                    min_covered_locks: 1,
                    max_covered_locks: 1,
                    witness_pattern_id: 0,
                }],
                peak_segment_pages: 1,
            },
            1,
        )
        .expect("condition result");
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    let control = ExecutionControl::new(token);

    let result = merge.finish(
        &shapes,
        SetupCandidatePriority::All,
        SetupLengthPreference::Auto,
        None,
        &control,
    );

    assert!(matches!(result, Err(WasmExactSearchError::Cancelled)));
}

#[test]
fn completed_condition_uses_shape_index_to_break_equal_board_ties() {
    let shapes = vec![SetupShape::new(1, 0, 0), SetupShape::new(1, 1, 0)];
    let equal_coverage = |shape_index| SetupParallelShapeResult {
        shape_index,
        build_covered_patterns: 1,
        joint_covered_patterns: 1,
        build_weight: 1.0,
        joint_weight: 1.0,
        min_covered_locks: 1,
        max_covered_locks: 1,
        witness_pattern_id: 0,
    };
    let mut merge = SetupConditionMerge::new(2, 1, 1).expect("condition merge");
    merge
        .absorb(
            SetupParallelTaskResult {
                task_index: 0,
                condition_index: 0,
                word_start: 0,
                word_end: 1,
                global_pattern_count: 1,
                covered_shapes: vec![equal_coverage(1), equal_coverage(0)],
                peak_segment_pages: 1,
            },
            2,
        )
        .expect("condition result");

    let completed = merge
        .finish(
            &shapes,
            SetupCandidatePriority::All,
            SetupLengthPreference::Auto,
            None,
            &ExecutionControl::default(),
        )
        .expect("completed condition");

    assert_eq!(completed.candidate_count, 1);
    assert_eq!(completed.selected_shapes[0].shape_index, 0);
}

fn assert_exact_condition_ranges(condition_words: &[usize], tasks: &[SetupParallelTask]) {
    for (task_index, task) in tasks.iter().enumerate() {
        assert_eq!(task.task_index as usize, task_index);
    }
    for (condition_index, word_count) in condition_words.iter().copied().enumerate() {
        let condition_tasks = tasks
            .iter()
            .filter(|task| task.condition_index as usize == condition_index)
            .collect::<Vec<_>>();
        assert!(!condition_tasks.is_empty());
        let mut cursor = 0;
        for task in condition_tasks {
            assert_eq!(task.word_start as usize, cursor);
            assert!(task.word_end > task.word_start);
            cursor = task.word_end as usize;
        }
        assert_eq!(cursor, word_count);
    }
}
