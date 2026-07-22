use crate::solver::BitsetExactCoverSolver;

use super::*;

#[test]
fn remaps_sparse_shape_bits_to_compact_exact_cover_columns() {
    let shape_mask = (1_u64 << 0) | (1_u64 << 2) | (1_u64 << 5);
    let candidate_masks = vec![(1_u64 << 0) | (1_u64 << 5), 1_u64 << 2];

    let problem = SetupTilingBridge::problem_from_shape_and_candidates(shape_mask, candidate_masks);

    assert_eq!(problem.column_count(), 3);
    assert_eq!(problem.candidates()[0].columns(), &[0, 2]);
    assert_eq!(problem.candidates()[1].columns(), &[1]);

    let solution = BitsetExactCoverSolver::solve_first(&problem).expect("sparse cover solves");
    assert_eq!(solution.candidate_ids(), &[0, 1]);
}

#[test]
fn ignores_candidate_bits_outside_shape_universe() {
    let shape_mask = (1_u64 << 4) | (1_u64 << 8);
    let candidate_masks = vec![(1_u64 << 4) | (1_u64 << 40), 1_u64 << 8];

    let problem = SetupTilingBridge::problem_from_shape_and_candidates(shape_mask, candidate_masks);

    assert_eq!(problem.column_count(), 2);
    assert_eq!(problem.candidates()[0].columns(), &[0]);
    assert_eq!(problem.candidates()[1].columns(), &[1]);
}

#[test]
fn enumerate_uses_dlx_solver_for_setup_shape_tiling_candidates() {
    let shape_mask = 0b1111;
    let candidate_masks = vec![0b0011, 0b1100, 0b0101, 0b1010];

    let report = SetupTilingBridge::enumerate(
        shape_mask,
        candidate_masks,
        crate::solver::DlxSearchLimits::new(8, 128),
    )
    .expect("dlx report");

    assert!(report.complete());
    assert_eq!(report.solution_count(), 2);
    assert_eq!(report.solutions()[0].candidate_ids(), &[0, 1]);
    assert_eq!(report.solutions()[1].candidate_ids(), &[2, 3]);
}

#[test]
fn standard_setup_tiling_still_works() {
    let schema =
        SetupTilingBridge::problem_schema_from_shape_and_candidates(0b1111, vec![0b0011, 0b1100])
            .expect("schema");
    let report = DlxSolver::solve_all_limited(
        &schema.to_problem(),
        crate::solver::DlxSearchLimits::new(4, 32),
    )
    .expect("dlx report");

    assert!(report.complete());
    assert_eq!(report.solution_count(), 1);
}
