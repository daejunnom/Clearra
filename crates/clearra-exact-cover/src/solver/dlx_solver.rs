mod error {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum DlxSolverError {
        ZeroMaxSolutions,
        ZeroMaxNodes,
        EmptyCandidate {
            candidate_id: usize,
        },
        CandidateColumnOutOfRange {
            candidate_id: usize,
            column: usize,
            column_count: usize,
        },
        DuplicateColumnInCandidate {
            candidate_id: usize,
            column: usize,
        },
    }
}
mod limits {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct DlxSearchLimits {
        max_solutions: usize,
        max_nodes: usize,
    }

    impl DlxSearchLimits {
        pub const fn new(max_solutions: usize, max_nodes: usize) -> Self {
            Self {
                max_solutions,
                max_nodes,
            }
        }
    }
    impl DlxSearchLimits {
        pub const fn max_solutions(self) -> usize {
            self.max_solutions
        }
    }
    impl DlxSearchLimits {
        pub const fn max_nodes(self) -> usize {
            self.max_nodes
        }
    }

    impl Default for DlxSearchLimits {
        fn default() -> Self {
            Self {
                max_solutions: 1024,
                max_nodes: 1_000_000,
            }
        }
    }
}
mod report {
    use crate::model::ExactCoverSolution;

    use super::DlxTruncatedReason;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct DlxSolveReport {
        solutions: Vec<ExactCoverSolution>,
        searched_nodes: usize,
        complete: bool,
        truncated_reason: Option<DlxTruncatedReason>,
    }

    impl DlxSolveReport {
        pub fn new(
            solutions: Vec<ExactCoverSolution>,
            searched_nodes: usize,
            complete: bool,
            truncated_reason: Option<DlxTruncatedReason>,
        ) -> Self {
            Self {
                solutions,
                searched_nodes,
                complete,
                truncated_reason,
            }
        }
    }
    impl DlxSolveReport {
        pub fn solutions(&self) -> &[ExactCoverSolution] {
            &self.solutions
        }
    }
    impl DlxSolveReport {
        pub fn solution_count(&self) -> usize {
            self.solutions.len()
        }
    }
    impl DlxSolveReport {
        pub fn searched_nodes(&self) -> usize {
            self.searched_nodes
        }
    }
    impl DlxSolveReport {
        pub fn complete(&self) -> bool {
            self.complete
        }
    }
    impl DlxSolveReport {
        pub fn truncated_reason(&self) -> Option<DlxTruncatedReason> {
            self.truncated_reason
        }
    }
    impl DlxSolveReport {
        pub fn truncation_reason(&self) -> Option<DlxTruncatedReason> {
            self.truncated_reason()
        }
    }
}
mod row_index {
    use crate::model::ExactCoverProblem;

    use super::DlxSolverError;

    pub(super) fn build_rows_by_column(
        problem: &ExactCoverProblem,
    ) -> Result<Vec<Vec<usize>>, DlxSolverError> {
        let mut rows_by_column = vec![Vec::new(); problem.column_count()];

        for (candidate_index, candidate) in problem.candidates().iter().enumerate() {
            if candidate.columns().is_empty() {
                return Err(DlxSolverError::EmptyCandidate {
                    candidate_id: candidate.id(),
                });
            }

            let mut sorted_columns = candidate.columns().to_vec();
            sorted_columns.sort_unstable();
            for window in sorted_columns.windows(2) {
                if window[0] == window[1] {
                    return Err(DlxSolverError::DuplicateColumnInCandidate {
                        candidate_id: candidate.id(),
                        column: window[0],
                    });
                }
            }

            for column in candidate.columns() {
                if *column >= problem.column_count() {
                    return Err(DlxSolverError::CandidateColumnOutOfRange {
                        candidate_id: candidate.id(),
                        column: *column,
                        column_count: problem.column_count(),
                    });
                }
                rows_by_column[*column].push(candidate_index);
            }
        }

        Ok(rows_by_column)
    }
}
mod search {
    use crate::model::{ExactCoverCandidate, ExactCoverProblem, ExactCoverSolution};

    use super::{DlxSearchLimits, DlxSolveReport, DlxTruncatedReason};

    pub(super) struct DlxSearch<'a> {
        problem: &'a ExactCoverProblem,
        rows_by_column: Vec<Vec<usize>>,
        limits: DlxSearchLimits,
        covered: Vec<bool>,
        chosen_candidate_ids: Vec<usize>,
        solutions: Vec<ExactCoverSolution>,
        searched_nodes: usize,
        truncated_reason: Option<DlxTruncatedReason>,
    }

    impl<'a> DlxSearch<'a> {
        pub(super) fn new(
            problem: &'a ExactCoverProblem,
            rows_by_column: Vec<Vec<usize>>,
            limits: DlxSearchLimits,
        ) -> Self {
            Self {
                problem,
                rows_by_column,
                limits,
                covered: vec![false; problem.column_count()],
                chosen_candidate_ids: Vec::new(),
                solutions: Vec::new(),
                searched_nodes: 0,
                truncated_reason: None,
            }
        }
    }
    impl<'a> DlxSearch<'a> {
        pub(super) fn run(mut self) -> DlxSolveReport {
            self.search();
            self.finish()
        }
    }
    impl<'a> DlxSearch<'a> {
        fn search(&mut self) {
            if self.truncated_reason.is_some() {
                return;
            }
            if self.searched_nodes >= self.limits.max_nodes() {
                self.truncated_reason = Some(DlxTruncatedReason::MaxNodes);
                return;
            }
            self.searched_nodes += 1;

            if self.covered[..self.problem.required_column_count()]
                .iter()
                .all(|value| *value)
            {
                self.solutions
                    .push(ExactCoverSolution::new(self.chosen_candidate_ids.clone()));
                if self.solutions.len() >= self.limits.max_solutions() {
                    self.truncated_reason = Some(DlxTruncatedReason::MaxSolutions);
                }
                return;
            }

            let Some(column) = self.choose_uncovered_column_with_fewest_rows() else {
                return;
            };
            let candidate_indexes = self.rows_by_column[column].clone();
            for candidate_index in candidate_indexes {
                if self.truncated_reason.is_some() {
                    return;
                }
                let candidate = &self.problem.candidates()[candidate_index];
                if !self.can_choose(candidate) {
                    continue;
                }
                self.choose(candidate);
                self.search();
                self.unchoose(candidate);
            }
        }
    }
    impl<'a> DlxSearch<'a> {
        fn choose_uncovered_column_with_fewest_rows(&self) -> Option<usize> {
            let mut best: Option<(usize, usize)> = None;
            for column in 0..self.problem.required_column_count() {
                if self.covered[column] {
                    continue;
                }
                let count = self.rows_by_column[column]
                    .iter()
                    .filter(|index| self.can_choose(&self.problem.candidates()[**index]))
                    .count();
                if count == 0 {
                    return None;
                }
                if best.is_none_or(|(_, best_count)| count < best_count) {
                    best = Some((column, count));
                }
            }
            best.map(|(column, _)| column)
        }
    }
    impl<'a> DlxSearch<'a> {
        fn can_choose(&self, candidate: &ExactCoverCandidate) -> bool {
            candidate
                .columns()
                .iter()
                .all(|column| !self.covered[*column])
        }
    }
    impl<'a> DlxSearch<'a> {
        fn choose(&mut self, candidate: &ExactCoverCandidate) {
            for column in candidate.columns() {
                self.covered[*column] = true;
            }
            self.chosen_candidate_ids.push(candidate.id());
        }
    }
    impl<'a> DlxSearch<'a> {
        fn unchoose(&mut self, candidate: &ExactCoverCandidate) {
            self.chosen_candidate_ids.pop();
            for column in candidate.columns() {
                self.covered[*column] = false;
            }
        }
    }
    impl<'a> DlxSearch<'a> {
        fn finish(self) -> DlxSolveReport {
            DlxSolveReport::new(
                self.solutions,
                self.searched_nodes,
                self.truncated_reason.is_none(),
                self.truncated_reason,
            )
        }
    }
}
mod solver {
    use crate::model::{ExactCoverProblem, ExactCoverSolution};

    use super::{
        row_index::build_rows_by_column, search::DlxSearch, DlxSearchLimits, DlxSolverError,
        DlxSolverResult,
    };

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct DlxSolver;

    impl DlxSolver {
        pub fn solve_first(
            problem: &ExactCoverProblem,
        ) -> Result<Option<ExactCoverSolution>, DlxSolverError> {
            let report = Self::solve_all_limited(problem, DlxSearchLimits::new(1, usize::MAX))?;
            Ok(report.solutions().first().cloned())
        }
    }
    impl DlxSolver {
        pub fn solve_all(problem: &ExactCoverProblem) -> DlxSolverResult {
            Self::solve_all_limited(problem, DlxSearchLimits::default())
        }
    }
    impl DlxSolver {
        pub fn solve_all_limited(
            problem: &ExactCoverProblem,
            limits: DlxSearchLimits,
        ) -> DlxSolverResult {
            if limits.max_solutions() == 0 {
                return Err(DlxSolverError::ZeroMaxSolutions);
            }
            if limits.max_nodes() == 0 {
                return Err(DlxSolverError::ZeroMaxNodes);
            }
            let rows_by_column = build_rows_by_column(problem)?;
            Ok(DlxSearch::new(problem, rows_by_column, limits).run())
        }
    }
}
mod truncated_reason {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum DlxTruncatedReason {
        MaxSolutions,
        MaxNodes,
    }
}

pub use error::DlxSolverError;
pub use limits::DlxSearchLimits;
pub use report::DlxSolveReport;
pub use solver::DlxSolver;
pub use truncated_reason::DlxTruncatedReason;

pub type DlxSolverResult = Result<DlxSolveReport, DlxSolverError>;

#[cfg(test)]
use crate::model::ExactCoverProblem;
#[cfg(test)]
#[path = "dlx_solver_tests.rs"]
mod tests;
