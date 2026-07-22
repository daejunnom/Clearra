use crate::model::{ExactCoverProblem, ExactCoverSolution};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BitsetExactCoverSolver;

impl BitsetExactCoverSolver {
    pub fn solve_first(problem: &ExactCoverProblem) -> Option<ExactCoverSolution> {
        let mut covered = vec![false; problem.column_count()];
        let mut chosen = Vec::new();
        solve_at(problem, 0, &mut covered, &mut chosen)
    }
}

fn solve_at(
    problem: &ExactCoverProblem,
    start: usize,
    covered: &mut [bool],
    chosen: &mut Vec<usize>,
) -> Option<ExactCoverSolution> {
    if covered.iter().all(|column| *column) {
        return Some(ExactCoverSolution::new(chosen.clone()));
    }

    for index in start..problem.candidates().len() {
        let candidate = &problem.candidates()[index];
        if candidate
            .columns()
            .iter()
            .any(|column| *column >= covered.len() || covered[*column])
        {
            continue;
        }

        for column in candidate.columns() {
            covered[*column] = true;
        }
        chosen.push(candidate.id());

        if let Some(solution) = solve_at(problem, index + 1, covered, chosen) {
            return Some(solution);
        }

        chosen.pop();
        for column in candidate.columns() {
            covered[*column] = false;
        }
    }

    None
}
