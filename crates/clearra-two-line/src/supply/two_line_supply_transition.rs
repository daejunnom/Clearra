use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TwoLineSupplyTransition {
    queue_index_before: usize,
    queue_index_after: usize,
    consumed: PieceKind,
}

impl TwoLineSupplyTransition {
    pub fn consume(queue_index_before: usize, consumed: PieceKind) -> Self {
        Self {
            queue_index_before,
            queue_index_after: queue_index_before + 1,
            consumed,
        }
    }
}
impl TwoLineSupplyTransition {
    pub fn queue_index_before(self) -> usize {
        self.queue_index_before
    }
}
impl TwoLineSupplyTransition {
    pub fn queue_index_after(self) -> usize {
        self.queue_index_after
    }
}
impl TwoLineSupplyTransition {
    pub fn consumed(self) -> PieceKind {
        self.consumed
    }
}
