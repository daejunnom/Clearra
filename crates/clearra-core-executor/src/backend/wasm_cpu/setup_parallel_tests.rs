use super::super::{
    setup_all_paths::{SetupSolutionPath, SetupSolutionStep},
    setup_finder::SetupHoldAction,
};
use super::*;
use clearra_core_domain::piece::piece_kind::PieceKind;

#[test]
fn large_condition_mix_leaves_multiple_tasks_per_verifier() {
    let condition_words = [1_890, 473, 473, 473, 473];
    let tasks = plan_parallel_tasks(&condition_words, 11).expect("task plan");

    assert!(tasks.len() >= 30);
    assert_exact_condition_ranges(&condition_words, &tasks);
}

#[test]
fn small_conditions_keep_one_task_each() {
    let condition_words = [4, 32, 127];
    let tasks = plan_parallel_tasks(&condition_words, 12).expect("task plan");

    assert_eq!(tasks.len(), condition_words.len());
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
        solution_paths: Vec::new(),
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
fn result_wire_preserves_every_exact_solution_path_step() {
    let path = SetupSolutionPath {
        steps: vec![
            SetupSolutionStep {
                piece: PieceKind::T,
                rotation: 3,
                x: -1,
                y: 4,
                hold_action: SetupHoldAction::StoreCurrentUseNext,
                cleared_lines: 0,
            },
            SetupSolutionStep {
                piece: PieceKind::I,
                rotation: 1,
                x: 6,
                y: -2,
                hold_action: SetupHoldAction::SwapHeld,
                cleared_lines: 2,
            },
        ],
    };
    let encoded = encode_results(&[SetupParallelTaskResult {
        task_index: 0,
        condition_index: 0,
        word_start: 0,
        word_end: 1,
        global_pattern_count: 64,
        covered_shapes: Vec::new(),
        solution_paths: vec![path.clone()],
        peak_segment_pages: 1,
    }])
    .expect("encode result");
    let decoded = decode_results(&encoded).expect("decode result");

    assert_eq!(decoded[0].solution_paths, vec![path]);
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
