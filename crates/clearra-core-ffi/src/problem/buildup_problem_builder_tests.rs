use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::packing_problem::{CPackingCandidate, CPackingOperation};

use super::super::{C_GOAL_CLEAR_TO_EMPTY, C_PIECE_I, C_PIECE_O, C_RULE_SRS_PLUS};
use super::*;

#[test]
fn buildup_problem_wraps_compact_packing_problem() {
    let problem =
        ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::two_lines()))
            .expect("problem");
    let buildup = CBuildUpProblemBuilder::from_search_problem(&problem).expect("buildup");

    assert_eq!(buildup.packing.problem_kind, 1);
    assert_eq!(buildup.packing.piece_window.max_pieces, 5);
    assert_eq!(buildup.initial_board.width, 10);
    assert_eq!(
        buildup.piece_source.piece_source_id,
        buildup.packing.piece_source.piece_source_id
    );
    assert_eq!(
        buildup.initial_hold_automaton.piece_source_id,
        buildup.piece_source.piece_source_id
    );
    assert_eq!(
        buildup.rule.kick_profile_id,
        buildup.packing.rule.kick_profile_id
    );
    assert_eq!(buildup.line_clear_policy, C_LINE_CLEAR_POLICY_STANDARD);
    assert_eq!(buildup.goal, buildup.packing.goal);
    assert_eq!(
        buildup.buildup_flags,
        crate::problem::C_BUILDUP_FLAG_HOLD_ENABLED
    );
    assert_eq!(
        buildup.terminal_projection_policy_version,
        crate::problem::C_BUILDUP_TERMINAL_PROJECTION_POLICY_VERSION
    );
    assert_eq!(
        buildup.terminal_projection_policy,
        crate::problem::C_BUILDUP_TERMINAL_PROJECTION_DISABLED
    );
}

#[test]
fn packing_candidate_converts_to_buildup_problem() {
    let problem =
        ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::two_lines()))
            .expect("problem");
    let mut candidate = CPackingCandidate {
        candidate_id: 3,
        canonical_operation_set_id: 5,
        operation_count: 2,
        ..Default::default()
    };
    candidate.operations[0] = CPackingOperation {
        piece: C_PIECE_O,
        rotation: 0,
        x: 0,
        y: 0,
        operation_id: 4,
        required_deleted_row_mask: 0,
        mask: 0x0c03,
    };
    candidate.operations[1] = CPackingOperation {
        piece: C_PIECE_I,
        rotation: 0,
        x: 2,
        y: 0,
        operation_id: 0,
        required_deleted_row_mask: 0,
        mask: 0x003c,
    };

    let buildup = CBuildUpProblemBuilder::from_packing_candidate(&problem, &candidate, 0, 42)
        .expect("buildup");

    assert_eq!(buildup.initial_board.initial_mask, 0);
    assert_eq!(buildup.operation_set.operation_count, 2);
    assert_eq!(buildup.candidate_id, 3);
    assert_eq!(buildup.canonical_operation_set_id, 5);
    assert_eq!(buildup.operation_set.representative_order_hint[0], 0);
    assert_eq!(buildup.operation_set.representative_order_hint[1], 1);
    assert_eq!(buildup.operation_set.operations[0].piece, C_PIECE_O);
    assert_eq!(buildup.operation_set.operations[1].piece, C_PIECE_I);
    assert_eq!(
        buildup.piece_source.piece_source_id,
        buildup.packing.piece_source.piece_source_id
    );
    assert_eq!(buildup.initial_hold_automaton.hold_empty, 1);
    assert_eq!(buildup.rule.rule_profile_id, C_RULE_SRS_PLUS);
    assert_eq!(buildup.line_clear_policy, C_LINE_CLEAR_POLICY_STANDARD);
    assert_eq!(buildup.piece_window.max_pieces, 5);
    assert_eq!(buildup.goal, C_GOAL_CLEAR_TO_EMPTY);
    assert_eq!(buildup.coverage_pattern_id, 42);
    assert_eq!(buildup.piece_source_pattern_id, 0);
    assert_eq!(buildup.packing.piece_source_pattern_id, 0);
    assert_eq!(buildup.packing.piece_multiset_family.count, 0);
    assert_eq!(buildup.packing.piece_multiset_window.total_count, 2);
    assert_eq!(buildup.packing.piece_multiset_window.exact_count, 2);
    assert_eq!(
        buildup.packing.piece_multiset_window.counts[C_PIECE_I as usize],
        1
    );
    assert_eq!(
        buildup.packing.piece_multiset_window.counts[C_PIECE_O as usize],
        1
    );
}

#[test]
fn buildup_problem_preserves_actual_piece_source_sequence() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::T,
            PieceKind::I,
            PieceKind::O,
        ])),
        PieceWindow::new(3),
    )
    .with_exact_pieces(Some(3));
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let buildup = CBuildUpProblemBuilder::from_search_problem(&problem).expect("buildup");

    assert_eq!(buildup.piece_source_pattern_len, 3);
    assert_eq!(buildup.piece_source_pattern_complete, 1);
    assert_eq!(
        &buildup.piece_source_pattern_pieces[..3],
        &[super::super::C_PIECE_T, C_PIECE_I, C_PIECE_O]
    );
    assert!(problem.supply().projects_unplaced_lookahead());
    assert!(!problem.supply().projects_standard_bag_lookahead());
    assert_eq!(
        buildup.terminal_projection_policy_version,
        crate::problem::C_BUILDUP_TERMINAL_PROJECTION_POLICY_VERSION
    );
    assert_eq!(
        buildup.terminal_projection_policy,
        crate::problem::C_BUILDUP_TERMINAL_PROJECTION_RELEASE_FINITE_HELD
    );
}

#[test]
fn occupied_initial_hold_selects_the_finite_terminal_projection_policy() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::Z,
        ])),
        PieceWindow::new(5),
    )
    .with_hold_piece(Some(PieceKind::S))
    .with_exact_pieces(Some(5));
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let buildup = CBuildUpProblemBuilder::from_search_problem(&problem).expect("buildup");

    assert!(problem.supply().projects_unplaced_lookahead());
    assert!(!problem.supply().projects_standard_bag_lookahead());
    assert_eq!(
        buildup.initial_hold_automaton.hold_piece,
        super::super::C_PIECE_S
    );
    assert_eq!(buildup.initial_hold_automaton.hold_empty, 0);
    assert_eq!(
        buildup.terminal_projection_policy,
        crate::problem::C_BUILDUP_TERMINAL_PROJECTION_RELEASE_FINITE_HELD
    );
}

#[test]
fn inferred_standard_bag_current_does_not_select_finite_terminal_projection() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0x80787),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(8),
    )
    .with_hold_piece(Some(PieceKind::S))
    .with_supply_window_size(SupplyWindowSize::new(7))
    .with_exact_pieces(Some(8));
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let buildup = CBuildUpProblemBuilder::from_search_problem(&problem).expect("buildup");

    assert!(problem.supply().projects_unplaced_lookahead());
    assert!(problem.supply().projects_standard_bag_lookahead());
    assert_eq!(
        buildup.terminal_projection_policy,
        crate::problem::C_BUILDUP_TERMINAL_PROJECTION_DISABLED
    );
}

#[test]
fn terminal_projection_fields_are_the_immutable_tail_of_the_c_problem_abi() {
    assert_eq!(
        core::mem::offset_of!(CBuildUpProblem, terminal_projection_policy_version),
        core::mem::size_of::<CBuildUpProblem>() - 4
    );
    assert_eq!(
        core::mem::offset_of!(CBuildUpProblem, terminal_projection_policy),
        core::mem::size_of::<CBuildUpProblem>() - 2
    );
    assert_eq!(
        core::mem::offset_of!(CBuildUpProblem, terminal_projection_reserved),
        core::mem::size_of::<CBuildUpProblem>() - 1
    );
    assert_eq!(
        core::mem::size_of::<crate::supply::CHoldAutomatonStateDescriptor>(),
        40
    );
    assert_eq!(
        core::mem::offset_of!(crate::supply::CHoldAutomatonStateDescriptor, hold_piece),
        32
    );
    assert_eq!(
        core::mem::offset_of!(crate::supply::CHoldAutomatonStateDescriptor, hold_empty),
        33
    );
    assert_eq!(
        core::mem::offset_of!(
            crate::supply::CHoldAutomatonStateDescriptor,
            terminal_projection_consumed
        ),
        34
    );
    assert_eq!(
        core::mem::offset_of!(
            crate::supply::CHoldAutomatonStateDescriptor,
            terminal_projection_provenance
        ),
        35
    );
    assert_eq!(
        core::mem::offset_of!(crate::supply::CHoldAutomatonStateDescriptor, reserved),
        36
    );
}
