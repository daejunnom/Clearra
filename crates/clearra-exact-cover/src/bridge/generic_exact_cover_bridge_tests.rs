use crate::{
    builder::CellUniverseBuilder,
    model::GenericExactCoverCandidate,
    solver::{DlxSearchLimits, DlxTruncatedReason},
};

use super::*;

#[test]
fn generic_exact_cover_bridge_enumerates_custom_piece_tiling_candidates() {
    let universe = CellUniverseBuilder::universe_from_cells([0, 1, 2, 3]).expect("universe");
    let candidates = vec![
        GenericExactCoverCandidate::from_cells(10, "custom:domino-a", 2, vec![0, 1], &universe)
            .expect("candidate"),
        GenericExactCoverCandidate::from_cells(11, "custom:domino-b", 2, vec![2, 3], &universe)
            .expect("candidate"),
        GenericExactCoverCandidate::from_cells(12, "custom:domino-c", 2, vec![0, 2], &universe)
            .expect("candidate"),
        GenericExactCoverCandidate::from_cells(13, "custom:domino-d", 2, vec![1, 3], &universe)
            .expect("candidate"),
    ];

    let report = GenericExactCoverBridge::enumerate_tilings(
        &universe,
        &candidates,
        [2, 2],
        DlxSearchLimits::new(8, 128),
    )
    .expect("dlx report");

    assert!(report.complete());
    assert_eq!(report.solution_count(), 2);
    assert_eq!(report.solutions()[0].candidate_ids(), &[10, 11]);
    assert_eq!(report.solutions()[1].candidate_ids(), &[12, 13]);
}

#[test]
fn generic_exact_cover_bridge_rejects_area_infeasible_shape_before_dlx() {
    let universe = CellUniverseBuilder::universe_from_cells([0, 1, 2, 3, 4]).expect("universe");
    let err = GenericExactCoverBridge::enumerate_tilings(
        &universe,
        &[],
        [4, 4],
        DlxSearchLimits::new(8, 128),
    )
    .expect_err("infeasible before dlx");

    assert_eq!(
        err,
        GenericExactCoverBridgeError::AreaInfeasibleShape {
            target_area: 5,
            available_piece_areas: vec![4, 4]
        }
    );
}

#[test]
fn area_infeasible_shape_rejected_before_search() {
    let universe = CellUniverseBuilder::universe_from_cells([0, 1, 2]).expect("universe");
    let err = GenericExactCoverBridge::enumerate_tilings(
        &universe,
        &[],
        [4],
        DlxSearchLimits::new(8, 128),
    )
    .expect_err("infeasible before search");

    assert_eq!(
        err,
        GenericExactCoverBridgeError::AreaInfeasibleShape {
            target_area: 3,
            available_piece_areas: vec![4]
        }
    );
}

#[test]
fn generic_exact_cover_bridge_preserves_dlx_truncation_contract() {
    let universe = CellUniverseBuilder::universe_from_cells([0, 1]).expect("universe");
    let candidates = vec![
        GenericExactCoverCandidate::from_cells(1, "mono-a", 1, vec![0], &universe)
            .expect("candidate"),
        GenericExactCoverCandidate::from_cells(2, "mono-b", 1, vec![1], &universe)
            .expect("candidate"),
        GenericExactCoverCandidate::from_cells(3, "domino", 2, vec![0, 1], &universe)
            .expect("candidate"),
    ];

    let report = GenericExactCoverBridge::enumerate_tilings(
        &universe,
        &candidates,
        [1, 1],
        DlxSearchLimits::new(1, 128),
    )
    .expect("dlx report");

    assert!(!report.complete());
    assert_eq!(
        report.truncated_reason(),
        Some(DlxTruncatedReason::MaxSolutions)
    );
}
