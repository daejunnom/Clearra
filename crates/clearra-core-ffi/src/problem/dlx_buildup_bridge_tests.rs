use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_exact_cover::model::ExactCoverSolution;
use clearra_pc_graph::request::OpeningPcSearchQuery;
use clearra_problem::ProblemCompiler;

use crate::{
    packing_problem::CPackingOperation,
    problem::{C_PIECE_I, C_PIECE_O},
};

use super::*;

#[test]
fn dlx_result_maps_to_buildup_problem_without_treating_dlx_as_solution() {
    assert!(DlxBuildUpBridge::dlx_solution_is_not_build_variant());

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
        DlxBuildUpBridge::buildup_problem_from_solution(&problem, &solution, &operations, 77)
            .expect("buildup");

    assert_eq!(buildup.operation_set.operation_count, 2);
    assert_eq!(buildup.operation_set.operations[0].piece, C_PIECE_O);
    assert_eq!(buildup.operation_set.operations[1].piece, C_PIECE_I);
    assert_eq!(buildup.coverage_pattern_id, 77);
}

#[test]
fn dlx_buildup_bridge_rejects_missing_operation_candidate() {
    let solution = ExactCoverSolution::new(vec![42]);

    assert_eq!(
        DlxBuildUpBridge::packing_candidate_from_solution(&solution, &[]),
        Err(DlxBuildUpBridgeError::MissingCandidate { candidate_id: 42 })
    );
}
