use super::*;

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
