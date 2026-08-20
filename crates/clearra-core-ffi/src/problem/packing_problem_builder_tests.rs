use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_pc_graph::request::{
    GpuDeviceSelection, OpeningPcSearchQuery, PcExecutionPolicy, PcQueueInput, PcScenarioBoard,
    PcScenarioQuery, PieceWindow, RequestedSearchBackend,
};
use clearra_problem::{ProblemCompileError, ProblemCompiler};
use clearra_rules::profile::builtin_rules::srs_x;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::board::C_BOARD_BACKEND_BOARD64;
use crate::problem::{
    C_BACKEND_AUTO, C_BACKEND_CPU, C_BAG_STANDARD_7_BAG, C_GOAL_CLEAR_TO_EMPTY,
    C_KICK_SRS_PLUS_180, C_KICK_SRS_X, C_PIECE_I, C_PIECE_O, C_PIECE_S,
    C_PIECE_SET_STANDARD_TETROMINOES, C_PIECE_T, C_PIECE_Z, C_RULE_SRS_PLUS, C_RULE_SRS_X,
    C_SPAWN_STANDARD_10,
};

use super::*;

#[test]
fn opening_search_problem_converts_to_c_packing_problem() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
        ])),
    );
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");
    let compact = CPackingProblemBuilder::from_search_problem(&problem).expect("compact");

    assert_eq!(compact.problem_kind, CPackingProblem::OPENING_PC);
    assert_eq!(compact.board.width, 10);
    assert_eq!(compact.board.visible_height, 2);
    assert_eq!(problem.search_height(), 20);
    assert_eq!(compact.board.search_height, 2);
    assert_eq!(compact.board.initial_mask, 0);
    assert_eq!(compact.board.initial_mask_hi, 0);
    assert_eq!(compact.board.backend_kind, C_BOARD_BACKEND_BOARD64);
    assert_eq!(compact.board.cell_count, 20);
    assert_eq!(compact.goal_region_mask, (1_u64 << 20) - 1);
    assert_eq!(compact.required_fill_mask, compact.goal_region_mask);
    assert_eq!(compact.forbidden_mask, 0);
    assert_eq!(compact.exact_pieces, 5);
    assert_eq!(compact.piece_window.max_pieces, 5);
    assert_eq!(compact.piece_window.exact_pieces, 5);
    assert_eq!(compact.piece_window.has_exact_pieces, 1);
    assert_eq!(compact.piece_source.source_kind, 1);
    assert_eq!(compact.piece_source.fixed_sequence_len, 5);
    assert_eq!(compact.piece_source_pattern_len, 5);
    assert_eq!(compact.piece_source_pattern_complete, 1);
    assert_eq!(
        &compact.piece_source_pattern_pieces[..5],
        &[C_PIECE_I, C_PIECE_O, C_PIECE_T, C_PIECE_S, C_PIECE_Z]
    );
    assert_eq!(compact.piece_multiset_window.total_count, 5);
    assert_eq!(compact.piece_multiset_window.exact_count, 5);
    assert_ne!(compact.piece_source.provenance_id, 0);
    assert_eq!(
        compact.piece_multiset_window.counts[usize::from(C_PIECE_I)],
        1
    );
    assert_eq!(
        compact.piece_multiset_window.counts[usize::from(C_PIECE_O)],
        1
    );
    assert_eq!(
        compact.piece_multiset_window.counts[usize::from(C_PIECE_T)],
        1
    );
    assert_eq!(
        compact.piece_multiset_window.counts[usize::from(C_PIECE_S)],
        1
    );
    assert_eq!(
        compact.piece_multiset_window.counts[usize::from(C_PIECE_Z)],
        1
    );
    assert_eq!(compact.rule.rule_profile_id, C_RULE_SRS_PLUS);
    assert_eq!(compact.rule.kick_profile_id, C_KICK_SRS_PLUS_180);
    assert_eq!(
        compact.rule.piece_set_profile_id,
        C_PIECE_SET_STANDARD_TETROMINOES
    );
    assert_eq!(compact.rule.bag_profile_id, C_BAG_STANDARD_7_BAG);
    assert_eq!(compact.goal, C_GOAL_CLEAR_TO_EMPTY);
    assert_eq!(compact.backend.requested_backend, C_BACKEND_AUTO);
    assert_eq!(compact.checkpoint.label_count, 1);
    assert_eq!(compact.checkpoint.partition_count, 1);
}

#[test]
fn packing_problem_builder_preserves_board_descriptor() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let compact = CPackingProblemBuilder::from_search_problem(&problem).expect("compact");

    assert_eq!(compact.board.width, 10);
    assert_eq!(compact.board.visible_height, 1);
    assert_eq!(compact.board.search_height, 1);
    assert_eq!(compact.board.initial_mask, 0x3f0);
    assert_eq!(compact.board.initial_mask_hi, 0);
    assert_eq!(compact.board.backend_kind, C_BOARD_BACKEND_BOARD64);
    assert_eq!(compact.board.cell_count, 10);
    assert_eq!(compact.goal_region_mask, 0x3ff);
    assert_eq!(compact.required_fill_mask, 0x00f);
}

#[test]
fn packing_problem_uses_piece_multiset_not_fixed_order() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::T,
            PieceKind::I,
            PieceKind::O,
        ])),
        PieceWindow::new(3),
    )
    .with_hold_piece(Some(PieceKind::L));
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let compact = CPackingProblemBuilder::from_search_problem(&problem).expect("compact");

    assert_eq!(compact.piece_source.source_kind, 1);
    assert_ne!(compact.piece_source.provenance_id, 0);
    assert_eq!(compact.piece_source_pattern_len, 3);
    assert_eq!(
        &compact.piece_source_pattern_pieces[..3],
        &[C_PIECE_T, C_PIECE_I, C_PIECE_O]
    );
    assert_eq!(compact.piece_multiset_window.total_count, 3);
    assert_eq!(compact.piece_multiset_window.exact_count, 0);
    assert_eq!(
        compact.piece_multiset_window.counts[usize::from(C_PIECE_I)],
        1
    );
    assert_eq!(
        compact.piece_multiset_window.counts[usize::from(C_PIECE_O)],
        1
    );
    assert_eq!(
        compact.piece_multiset_window.counts[usize::from(C_PIECE_T)],
        1
    );
}

#[test]
fn packing_problem_builder_preserves_rule_profile() {
    let problem =
        ProblemCompiler::compile_opening_pc(&OpeningPcSearchQuery::new(PcTarget::two_lines()))
            .expect("problem");
    let compact = CPackingProblemBuilder::from_search_problem(&problem).expect("compact");

    assert_eq!(
        compact.rule.piece_set_profile_id,
        C_PIECE_SET_STANDARD_TETROMINOES
    );
    assert_eq!(compact.rule.bag_profile_id, C_BAG_STANDARD_7_BAG);
    assert_eq!(compact.rule.rule_profile_id, C_RULE_SRS_PLUS);
    assert_eq!(compact.rule.kick_profile_id, C_KICK_SRS_PLUS_180);
    assert_eq!(compact.rule.spawn_profile_id, C_SPAWN_STANDARD_10);
    assert_eq!(compact.rule.has_verified_kick_profile, 0);
}

#[test]
fn scenario_search_problem_converts_budget_backend_hold_and_mask() {
    let execution = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_workers(3)
        .with_gpu_device(GpuDeviceSelection::Index(1))
        .with_max_memory_mib(Some(256));
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::T, PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_hold_piece(Some(PieceKind::L))
    .with_execution_policy(execution);
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");
    let compact = CPackingProblemBuilder::from_search_problem(&problem).expect("compact");

    assert_eq!(compact.problem_kind, CPackingProblem::SCENARIO_PC);
    assert_eq!(compact.board.initial_mask, 0x3f0);
    assert_eq!(compact.board.backend_kind, C_BOARD_BACKEND_BOARD64);
    assert_eq!(compact.board.visible_height, 1);
    assert_eq!(compact.board.search_height, 1);
    assert_eq!(compact.board.cell_count, 10);
    assert_eq!(compact.goal_region_mask, 0x3ff);
    assert_eq!(compact.required_fill_mask, 0x00f);
    assert_eq!(compact.forbidden_mask, 0);
    assert_eq!(compact.exact_pieces, 0);
    assert_ne!(compact.piece_source.provenance_id, 0);
    assert_eq!(compact.piece_multiset_window.total_count, 2);
    assert_eq!(
        compact.piece_multiset_window.counts[usize::from(C_PIECE_T)],
        1
    );
    assert_eq!(
        compact.piece_multiset_window.counts[usize::from(C_PIECE_I)],
        1
    );
    assert_eq!(compact.backend.requested_backend, C_BACKEND_CPU);
    assert_eq!(compact.backend.workers, 3);
    assert_eq!(compact.backend.gpu_device_kind, 1);
    assert_eq!(compact.backend.gpu_device_index, 1);
    assert_eq!(compact.budget.has_max_memory_mib, 1);
    assert_eq!(compact.budget.max_memory_mib, 256);
}

#[test]
fn packing_problem_builder_rejects_unsupported_board() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(7, 1_u64 << 63),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    assert_eq!(
        CPackingProblemBuilder::from_search_problem(&problem),
        Err(FfiProblemError::UnsupportedBoardBackend {
            backend_kind: "board128",
            cell_count: 70,
        })
    );
}

#[test]
fn packing_problem_builder_projects_builtin_srs_x_as_verified_kick_profile() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_rule(srs_x());
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

    let compact = CPackingProblemBuilder::from_search_problem(&problem).expect("SRS-X compact");

    assert_eq!(compact.rule.rule_profile_id, C_RULE_SRS_X);
    assert_eq!(compact.rule.kick_profile_id, C_KICK_SRS_X);
    assert_eq!(compact.rule.has_verified_kick_profile, 1);
    assert_eq!(compact.rule.verified_supports_180, 1);
    assert_eq!(compact.rule.verified_transition_count, 80);
}

#[test]
fn oversized_piece_window_is_rejected_before_c_boundary() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::default(),
        PieceWindow::new(usize::from(u16::MAX) + 1),
    );
    assert_eq!(
        ProblemCompiler::compile_scenario_pc(&query),
        Err(ProblemCompileError::PackingPieceWindowTooLarge {
            max_pieces: usize::from(u16::MAX) + 1
        })
    );
}

#[test]
fn board_descriptor_uses_active_packing_height_for_backend_selection() {
    let board64_query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(6, 0),
        PcQueueInput::default(),
        PieceWindow::new(1),
    );
    let tall_visible_query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(12, 0x3f0),
        PcQueueInput::default(),
        PieceWindow::new(1),
    );
    let board64_problem =
        ProblemCompiler::compile_scenario_pc(&board64_query).expect("board64 problem");
    let tall_visible_problem =
        ProblemCompiler::compile_scenario_pc(&tall_visible_query).expect("tall problem");
    let board64 = CPackingProblemBuilder::from_search_problem(&board64_problem).expect("compact");
    let tall_visible =
        CPackingProblemBuilder::from_search_problem(&tall_visible_problem).expect("compact");

    assert_eq!(board64.board.backend_kind, C_BOARD_BACKEND_BOARD64);
    assert_eq!(board64.board.cell_count, 10);
    assert_eq!(tall_visible.board.visible_height, 1);
    assert_eq!(tall_visible.board.search_height, 1);
    assert_eq!(tall_visible.board.backend_kind, C_BOARD_BACKEND_BOARD64);
    assert_eq!(tall_visible.board.cell_count, 10);
    assert_eq!(tall_visible.board.initial_mask_hi, 0);
}
