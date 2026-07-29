use super::*;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_problem::{SetupHoldPolicy, SetupPathDetail};
use clearra_rules::profile::builtin_rules::{jstris_180, srs_x};

use super::super::{setup_coverage_graph::SetupCoverageNode, setup_partial_build::SetupShape};

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
