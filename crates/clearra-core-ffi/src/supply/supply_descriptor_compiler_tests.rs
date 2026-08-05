use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::ProblemCompiler;
use clearra_supply::queue::{
    bag_aligned_pattern::BagAlignedPattern, fixed_sequence::FixedSequence,
    observed_queue::ObservedQueue,
};

use super::*;

#[test]
fn fixed_sequence_passed_to_c_queue_view() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::O])),
        PieceWindow::new(2),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let compact = SupplyDescriptorCompiler::compile(&problem).expect("compact supply");

    assert_eq!(compact.queue().mode, C_QUEUE_FIXED_SEQUENCE);
    assert_eq!(
        compact.queue().provenance_id,
        C_SUPPLY_PROVENANCE_FIXED_SEQUENCE
    );
    assert_eq!(compact.queue().stored_len, 2);
    assert_eq!(compact.queue().pieces[0], C_PIECE_I);
    assert_eq!(compact.queue().pieces[1], C_PIECE_O);
}

#[test]
fn fixed_sequence_passed_to_c() {
    fixed_sequence_passed_to_c_queue_view();
}

#[test]
fn piece_source_and_hold_automaton_share_compact_provenance_identity() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::O])),
        PieceWindow::new(2),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let compact = SupplyDescriptorCompiler::compile(&problem).expect("descriptors");

    assert_eq!(
        u64::from(compact.piece_source().provenance_id),
        compact.initial_hold_automaton().provenance_id
    );
}

#[test]
fn bag_pattern_passed_to_c_queue_view() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::bag_aligned_pattern(BagAlignedPattern::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
        ])),
        PieceWindow::new(3),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let compact = SupplyDescriptorCompiler::compile(&problem).expect("compact supply");
    let bag_window =
        SupplyDescriptorCompiler::bag_window_from_queue(&compact.queue(), &compact.piece_window());

    assert_eq!(compact.queue().mode, C_QUEUE_BAG_ALIGNED_PATTERN);
    assert_eq!(
        compact.queue().provenance_id,
        C_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN
    );
    assert_eq!(bag_window.boundary_known, 1);
}

#[test]
fn bag_pattern_passed_to_c() {
    bag_pattern_passed_to_c_queue_view();
}

#[test]
fn observed_expansion_remains_rust_owned_before_c_queue_view() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(
        PcQueueInput::observed(ObservedQueue::new(vec![PieceKind::T, PieceKind::I])),
    );
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

    let compact = SupplyDescriptorCompiler::compile(&problem).expect("compact supply");
    let bag_window =
        SupplyDescriptorCompiler::bag_window_from_queue(&compact.queue(), &compact.piece_window());

    assert_eq!(compact.queue().mode, C_QUEUE_OBSERVED);
    assert_eq!(
        compact.queue().provenance_id,
        C_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED
    );
    assert_eq!(compact.queue().stored_len, 2);
    assert_eq!(bag_window.boundary_known, 0);
}

#[test]
fn observed_expansion_remains_rust_owned() {
    observed_expansion_remains_rust_owned_before_c_queue_view();
}

#[test]
fn hold_state_passed_to_c() {
    let board = PcScenarioBoard::standard_10(4, 0);
    let query = PcScenarioQuery::new(
        board,
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_hold_piece(Some(PieceKind::T));
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    let compact = SupplyDescriptorCompiler::compile(&problem).expect("compact supply");

    assert_eq!(compact.hold().enabled, 1);
    assert_eq!(compact.hold().empty, 0);
    assert_eq!(compact.hold().piece, C_PIECE_T);
}

#[test]
fn piece_window_and_bag_window_compile_to_c_views() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines());
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

    let compact = SupplyDescriptorCompiler::compile(&problem).expect("compact supply");
    let bag_window =
        SupplyDescriptorCompiler::bag_window_from_queue(&compact.queue(), &compact.piece_window());

    assert_eq!(compact.piece_window().max_pieces, 5);
    assert_eq!(compact.piece_window().exact_pieces, 5);
    assert_eq!(compact.piece_window().has_exact_pieces, 1);
    assert_eq!(bag_window.start, 0);
}

#[test]
fn setup_and_build_presets_keep_raw_queue_separate_from_materialized_piece_source() {
    let setup_query = clearra_problem::query::SetupSearchQuery::default().with_queue(
        clearra_problem::query::SetupQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L,
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
        ])),
    );
    let setup_problem = ProblemCompiler::compile_setup(&setup_query).expect("setup problem");
    let setup = SupplyDescriptorCompiler::compile(&setup_problem).expect("setup supply");

    assert_eq!(setup.queue().mode, C_QUEUE_FIXED_SEQUENCE);
    assert_eq!(
        usize::from(setup.queue().stored_len),
        setup_problem.core_query().remaining_queue().len()
    );
    assert_eq!(setup.queue().pieces[0], C_PIECE_I);

    let build_query = clearra_problem::query::BuildQuery::coverage_bridge(
        clearra_problem::query::BuildTemplateBridge::new(
            "template-a",
            clearra_core_domain::board::board_size::BoardSize::new(10, 4).expect("board"),
            3,
        ),
        4,
        clearra_problem::query::BuildProblemLimits::new(12, 4),
    );
    let build_problem = ProblemCompiler::compile_build(&build_query).expect("build problem");
    let build = SupplyDescriptorCompiler::compile(&build_problem).expect("build supply");

    assert_eq!(build.queue().mode, C_QUEUE_OBSERVED);
    assert_eq!(build.queue().stored_len, 0);
    assert_eq!(build.piece_source().materialized_pattern_count, 4);
    assert_eq!(build.piece_source().complete, 0);
}

#[test]
fn queue_too_long_is_rejected_before_c_boundary() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 70_000])),
        PieceWindow::new(1),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    assert_eq!(
        SupplyDescriptorCompiler::compile(&problem),
        Err(FfiProblemError::QueueTooLong { len: 70_000 })
    );
}

#[test]
fn queue_truncated_but_exact_needed_is_rejected() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 70])),
        PieceWindow::new(70),
    )
    .with_exact_pieces(Some(70));
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("problem");

    assert_eq!(
        SupplyDescriptorCompiler::compile(&problem),
        Err(FfiProblemError::QueueTruncatedButExactNeeded {
            len: 70,
            stored_len: C_QUEUE_VIEW_CAPACITY,
            required_pieces: 70,
        })
    );
}
