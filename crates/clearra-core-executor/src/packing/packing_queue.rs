use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::PcQueueInput;

pub(crate) fn fixed_pieces(queue: &PcQueueInput) -> Option<&[PieceKind]> {
    queue
        .as_fixed_sequence()
        .map(clearra_supply::queue::fixed_sequence::FixedSequence::pieces)
}
