use crate::solver::DlxSearchLimits;

use super::*;

#[test]
fn custom_piece_bridge_builds_exact_cover_problem_from_placement_columns() {
    let problem = CustomPieceBridge::problem_from_placements(
        4,
        vec![
            CustomPiecePlacementColumns::new(100, vec![0, 1]),
            CustomPiecePlacementColumns::new(101, vec![2, 3]),
        ],
    )
    .expect("problem");

    assert_eq!(problem.column_count(), 4);
    assert_eq!(problem.candidates()[0].id(), 100);
    assert_eq!(problem.candidates()[0].columns(), &[0, 1]);
}

#[test]
fn custom_piece_bridge_enumerates_tilings_with_dlx_without_search_runtime() {
    let result = CustomPieceBridge::enumerate_tilings(
        4,
        vec![
            CustomPiecePlacementColumns::new(100, vec![0, 1]),
            CustomPiecePlacementColumns::new(101, vec![2, 3]),
            CustomPiecePlacementColumns::new(102, vec![0, 2]),
            CustomPiecePlacementColumns::new(103, vec![1, 3]),
        ],
        DlxSearchLimits::new(8, 128),
    )
    .expect("bridge result");

    assert!(result.complete());
    assert_eq!(result.solution_count(), 2);
    assert_eq!(result.solutions()[0].candidate_ids(), &[100, 101]);
    assert_eq!(result.solutions()[1].candidate_ids(), &[102, 103]);
}
