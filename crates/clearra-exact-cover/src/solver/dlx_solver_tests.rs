use crate::model::ExactCoverCandidate;

use super::*;

#[test]
fn dlx_solver_enumerates_all_exact_covers_with_complete_report() {
    let problem = ExactCoverProblem::new(
        4,
        vec![
            ExactCoverCandidate::new(10, vec![0, 1]),
            ExactCoverCandidate::new(11, vec![2, 3]),
            ExactCoverCandidate::new(12, vec![0, 2]),
            ExactCoverCandidate::new(13, vec![1, 3]),
        ],
    );

    let report = DlxSolver::solve_all(&problem).expect("solves");

    assert!(report.complete());
    assert_eq!(report.truncated_reason(), None);
    assert_eq!(report.solution_count(), 2);
    assert_eq!(report.solutions()[0].candidate_ids(), &[10, 11]);
    assert_eq!(report.solutions()[1].candidate_ids(), &[12, 13]);
    assert!(report.searched_nodes() > 0);
}

#[test]
fn dlx_solver_optional_columns_are_conflict_only_not_required() {
    let problem = ExactCoverProblem::with_optional_columns(
        2,
        1,
        vec![
            ExactCoverCandidate::new(10, vec![0, 2]),
            ExactCoverCandidate::new(11, vec![1, 2]),
            ExactCoverCandidate::new(12, vec![1]),
        ],
    );

    let report = DlxSolver::solve_all(&problem).expect("solves");

    assert!(report.complete());
    assert_eq!(problem.required_column_count(), 2);
    assert_eq!(problem.optional_column_count(), 1);
    assert_eq!(report.solution_count(), 1);
    assert_eq!(report.solutions()[0].candidate_ids(), &[10, 12]);
}

#[test]
fn dlx_solver_returns_complete_flag() {
    let problem = ExactCoverProblem::new(1, vec![ExactCoverCandidate::new(1, vec![0])]);

    let report =
        DlxSolver::solve_all_limited(&problem, DlxSearchLimits::new(1, 8)).expect("solves");

    assert!(!report.complete());
    assert_eq!(
        report.truncation_reason(),
        Some(DlxTruncatedReason::MaxSolutions)
    );
    assert!(report.searched_nodes() > 0);
}

#[test]
fn dlx_solver_reports_no_solution_as_complete_empty_result() {
    let problem = ExactCoverProblem::new(
        2,
        vec![
            ExactCoverCandidate::new(1, vec![0]),
            ExactCoverCandidate::new(2, vec![0]),
        ],
    );

    let report = DlxSolver::solve_all(&problem).expect("valid problem");

    assert!(report.complete());
    assert_eq!(report.solution_count(), 0);
}

#[test]
fn dlx_solver_reports_solution_limit_truncation() {
    let problem = ExactCoverProblem::new(
        2,
        vec![
            ExactCoverCandidate::new(1, vec![0]),
            ExactCoverCandidate::new(2, vec![1]),
            ExactCoverCandidate::new(3, vec![0, 1]),
        ],
    );

    let report =
        DlxSolver::solve_all_limited(&problem, DlxSearchLimits::new(1, 100)).expect("solves");

    assert!(!report.complete());
    assert_eq!(report.solution_count(), 1);
    assert_eq!(
        report.truncated_reason(),
        Some(DlxTruncatedReason::MaxSolutions)
    );
}

#[test]
fn dlx_solver_rejects_invalid_candidates_before_search() {
    let out_of_range = ExactCoverProblem::new(2, vec![ExactCoverCandidate::new(1, vec![2])]);
    assert_eq!(
        DlxSolver::solve_all(&out_of_range),
        Err(DlxSolverError::CandidateColumnOutOfRange {
            candidate_id: 1,
            column: 2,
            column_count: 2
        })
    );

    let duplicate = ExactCoverProblem::new(2, vec![ExactCoverCandidate::new(2, vec![0, 0])]);
    assert_eq!(
        DlxSolver::solve_all(&duplicate),
        Err(DlxSolverError::DuplicateColumnInCandidate {
            candidate_id: 2,
            column: 0
        })
    );
}
