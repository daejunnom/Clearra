use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::template::build_slot::BuildSlotId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlotConstraint {
    slot_id: BuildSlotId,
    required_piece: Option<PieceKind>,
}

impl SlotConstraint {
    pub fn any(slot_id: BuildSlotId) -> Self {
        Self {
            slot_id,
            required_piece: None,
        }
    }
}
impl SlotConstraint {
    pub fn required(slot_id: BuildSlotId, piece: PieceKind) -> Self {
        Self {
            slot_id,
            required_piece: Some(piece),
        }
    }
}
impl SlotConstraint {
    pub fn slot_id(self) -> BuildSlotId {
        self.slot_id
    }
}
impl SlotConstraint {
    pub fn required_piece(self) -> Option<PieceKind> {
        self.required_piece
    }
}
impl SlotConstraint {
    pub fn allows(self, piece: PieceKind) -> bool {
        self.required_piece.is_none_or(|required| required == piece)
    }
}
