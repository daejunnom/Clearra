use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::template::build_slot::BuildSlotId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotDomain {
    slot_id: BuildSlotId,
    pieces: Vec<PieceKind>,
}

impl SlotDomain {
    pub fn new(slot_id: BuildSlotId, pieces: Vec<PieceKind>) -> Self {
        Self { slot_id, pieces }
    }
}
impl SlotDomain {
    pub fn slot_id(&self) -> BuildSlotId {
        self.slot_id
    }
}
impl SlotDomain {
    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }
}
impl SlotDomain {
    pub fn is_empty(&self) -> bool {
        self.pieces.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use crate::template::build_slot::BuildSlotId;

    use super::*;

    #[test]
    fn slot_domain_works() {
        let slot = BuildSlotId::new(7);
        let domain = SlotDomain::new(slot, vec![PieceKind::I, PieceKind::O]);

        assert_eq!(domain.slot_id(), slot);
        assert_eq!(domain.pieces(), &[PieceKind::I, PieceKind::O]);
        assert!(!domain.is_empty());
    }
}
