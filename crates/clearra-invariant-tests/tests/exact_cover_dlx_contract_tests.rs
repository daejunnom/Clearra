use clearra_build_coverage::{
    assignment::assignment_exact_cover::AssignmentExactCoverBridge,
    domain::{slot_constraint::SlotConstraint, slot_domain::SlotDomain},
    template::build_slot::BuildSlotId,
};
use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_ffi::{
    problem::{C_PIECE_I, C_PIECE_O},
    CPackingOperation, DlxBuildUpBridge, DlxBuildUpOperationCandidate,
};
use clearra_exact_cover::{
    bridge::{
        CustomPieceBridge, CustomPiecePlacementColumns, GenericExactCoverBridge,
        GenericExactCoverBridgeError, SetupTilingBridge,
    },
    builder::CellUniverseBuilder,
    model::{
        ExactCoverCandidate, ExactCoverProblem, ExactCoverSolution, GenericExactCoverCandidate,
    },
    solver::{DlxSearchLimits, DlxSolver, DlxTruncatedReason},
};
use clearra_pc_graph::request::OpeningPcSearchQuery;
use clearra_problem::ProblemCompiler;

#[test]
fn dlx_solver_enumerates_exact_cover_solutions_with_completeness_contract() {
    let problem = ExactCoverProblem::new(
        4,
        vec![
            ExactCoverCandidate::new(10, vec![0, 1]),
            ExactCoverCandidate::new(11, vec![2, 3]),
            ExactCoverCandidate::new(12, vec![0, 2]),
            ExactCoverCandidate::new(13, vec![1, 3]),
        ],
    );

    let report = DlxSolver::solve_all_limited(&problem, DlxSearchLimits::new(8, 128))
        .expect("dlx exact cover");

    assert!(report.complete());
    assert_eq!(report.solution_count(), 2);
    assert_eq!(report.solutions()[0].candidate_ids(), &[10, 11]);
    assert_eq!(report.solutions()[1].candidate_ids(), &[12, 13]);
}

#[test]
fn dlx_solver_reports_truncation_without_claiming_complete_enumeration() {
    let problem = ExactCoverProblem::new(
        2,
        vec![
            ExactCoverCandidate::new(1, vec![0]),
            ExactCoverCandidate::new(2, vec![1]),
            ExactCoverCandidate::new(3, vec![0, 1]),
        ],
    );

    let report = DlxSolver::solve_all_limited(&problem, DlxSearchLimits::new(1, 128))
        .expect("dlx exact cover");

    assert!(!report.complete());
    assert_eq!(
        report.truncated_reason(),
        Some(DlxTruncatedReason::MaxSolutions)
    );
}

#[test]
fn setup_tiling_bridge_uses_dlx_after_sparse_shape_column_remap() {
    let shape_mask = (1_u64 << 0) | (1_u64 << 2) | (1_u64 << 5) | (1_u64 << 9);
    let candidate_masks = vec![
        (1_u64 << 0) | (1_u64 << 5),
        (1_u64 << 2) | (1_u64 << 9),
        (1_u64 << 0) | (1_u64 << 2),
        (1_u64 << 5) | (1_u64 << 9),
    ];

    let report =
        SetupTilingBridge::enumerate(shape_mask, candidate_masks, DlxSearchLimits::new(8, 128))
            .expect("setup tiling dlx");

    assert!(report.complete());
    assert_eq!(report.solution_count(), 2);
    assert_eq!(report.solutions()[0].candidate_ids(), &[0, 1]);
    assert_eq!(report.solutions()[1].candidate_ids(), &[2, 3]);
}

#[test]
fn custom_piece_bridge_uses_dlx_for_tiling_enumeration_without_pc_runtime_search() {
    let report = CustomPieceBridge::enumerate_tilings(
        4,
        vec![
            CustomPiecePlacementColumns::new(100, vec![0, 1]),
            CustomPiecePlacementColumns::new(101, vec![2, 3]),
            CustomPiecePlacementColumns::new(102, vec![0, 2]),
            CustomPiecePlacementColumns::new(103, vec![1, 3]),
        ],
        DlxSearchLimits::new(8, 128),
    )
    .expect("custom piece tiling");

    assert!(report.complete());
    assert_eq!(report.solution_count(), 2);
    assert_eq!(report.solutions()[0].candidate_ids(), &[100, 101]);
    assert_eq!(report.solutions()[1].candidate_ids(), &[102, 103]);
}

#[test]
fn generic_exact_cover_candidate_represents_custom_piece_tiling_cells() {
    let universe = CellUniverseBuilder::universe_from_cells([0, 1, 4, 5]).expect("universe");
    let candidates = vec![
        GenericExactCoverCandidate::from_cells(200, "custom:square-a", 2, vec![0, 1], &universe)
            .expect("candidate"),
        GenericExactCoverCandidate::from_cells(201, "custom:square-b", 2, vec![4, 5], &universe)
            .expect("candidate"),
    ];

    let report = GenericExactCoverBridge::enumerate_tilings(
        &universe,
        &candidates,
        [2, 2],
        DlxSearchLimits::new(8, 128),
    )
    .expect("generic exact-cover candidate tiling");

    assert!(report.complete());
    assert_eq!(report.solution_count(), 1);
    assert_eq!(report.solutions()[0].candidate_ids(), &[200, 201]);
    assert_eq!(candidates[0].piece_id(), "custom:square-a");
}

#[test]
fn area_infeasible_shape_rejected_before_expensive_search() {
    let universe = CellUniverseBuilder::universe_from_cells([0, 1, 2, 3, 4]).expect("universe");
    let err = GenericExactCoverBridge::enumerate_tilings(
        &universe,
        &[],
        [4, 4],
        DlxSearchLimits::new(8, 128),
    )
    .expect_err("area infeasible before dlx");

    assert_eq!(
        err,
        GenericExactCoverBridgeError::AreaInfeasibleShape {
            target_area: 5,
            available_piece_areas: vec![4, 4]
        }
    );
}

#[test]
fn dlx_result_maps_to_buildup_problem() {
    let problem =
        ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::two_lines()))
            .expect("problem");
    let solution = ExactCoverSolution::new(vec![10, 11]);
    let operations = vec![
        DlxBuildUpOperationCandidate::new(
            10,
            CPackingOperation {
                piece: C_PIECE_O,
                rotation: 0,
                x: 0,
                y: 0,
                operation_id: 4,
                required_deleted_row_mask: 0,
                mask: 0x0c03,
            },
        ),
        DlxBuildUpOperationCandidate::new(
            11,
            CPackingOperation {
                piece: C_PIECE_I,
                rotation: 0,
                x: 2,
                y: 0,
                operation_id: 0,
                required_deleted_row_mask: 0,
                mask: 0x003c,
            },
        ),
    ];

    let buildup =
        DlxBuildUpBridge::buildup_problem_from_solution(&problem, &solution, &operations, 99)
            .expect("buildup");

    assert_eq!(buildup.operation_set.operation_count, 2);
    assert_eq!(buildup.operation_set.operations[0].piece, C_PIECE_O);
    assert_eq!(buildup.operation_set.operations[1].piece, C_PIECE_I);
    assert_eq!(buildup.coverage_pattern_id, 99);
}

#[test]
fn build_slot_assignment_can_use_exact_cover_without_moving_csp_into_cli_or_search() {
    let slot_a = BuildSlotId::new(1);
    let slot_b = BuildSlotId::new(2);
    let bridge = AssignmentExactCoverBridge::new(
        vec![
            SlotDomain::new(slot_a, vec![PieceKind::I, PieceKind::O]),
            SlotDomain::new(slot_b, vec![PieceKind::T, PieceKind::S]),
        ],
        vec![SlotConstraint::required(slot_a, PieceKind::I)],
    );

    let result = bridge
        .solve(DlxSearchLimits::new(8, 128))
        .expect("assignment exact cover");

    assert!(result.complete());
    assert_eq!(result.assignments().len(), 2);
    assert!(result.assignments().iter().all(|assignment| {
        assignment
            .assigned_slots()
            .iter()
            .any(|slot| slot.slot_id() == slot_a && slot.piece() == PieceKind::I)
    }));
}
