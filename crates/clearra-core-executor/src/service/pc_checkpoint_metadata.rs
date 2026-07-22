use clearra_problem::SearchProblem;

pub(crate) fn checkpoint_partition_labels(problem: &SearchProblem) -> String {
    problem
        .checkpoint_schedule()
        .map(|schedule| schedule.partition_labels().join("|"))
        .unwrap_or_else(|| "none".to_owned())
}
