use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_core_ffi::supply::C_PIECE_SOURCE_FIXED_QUEUE;
use clearra_pc_graph::request::{OpeningPcSearchQuery, PcQueueInput};
use clearra_problem::ProblemCompiler;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::*;

#[test]
fn search_problem_lowers_to_packing_problem() {
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

    let packing = PackingProblemLowering::lower(&problem).expect("packing");

    assert_eq!(packing.problem_kind, CPackingProblem::OPENING_PC);
    assert_eq!(packing.piece_source.source_kind, C_PIECE_SOURCE_FIXED_QUEUE);
    assert_eq!(packing.piece_window.max_pieces, 5);
    assert_eq!(packing.piece_multiset_window.total_count, 5);
    assert_eq!(
        packing.piece_multiset_window.counts[usize::from(clearra_core_ffi::problem::C_PIECE_I)],
        1
    );
}

#[test]
fn packing_problem_uses_piece_multiset_not_fixed_order() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines()).with_queue(
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::T,
        ])),
    );
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("problem");

    let packing = PackingProblemLowering::lower(&problem).expect("packing");

    assert_eq!(packing.piece_multiset_window.total_count, 5);
    assert_eq!(
        packing.piece_multiset_window.counts[usize::from(clearra_core_ffi::problem::C_PIECE_I)],
        2
    );
    assert_eq!(
        packing.piece_multiset_window.counts[usize::from(clearra_core_ffi::problem::C_PIECE_T)],
        2
    );
}
