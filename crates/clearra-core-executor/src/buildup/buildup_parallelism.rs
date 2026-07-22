use clearra_problem::SearchProblem;

pub(crate) fn worker_count(problem: &SearchProblem, candidate_count: usize) -> usize {
    const MIN_CANDIDATES_PER_WORKER: usize = 32;

    if candidate_count < MIN_CANDIDATES_PER_WORKER * 2 || cfg!(target_family = "wasm") {
        return 1;
    }

    let requested = problem.backend_policy().workers();
    let useful = candidate_count.div_ceil(MIN_CANDIDATES_PER_WORKER);
    requested.min(useful).max(1)
}
