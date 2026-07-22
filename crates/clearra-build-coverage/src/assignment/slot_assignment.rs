use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::template::build_slot::BuildSlotId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AssignedSlot {
    slot_id: BuildSlotId,
    piece: PieceKind,
}

impl AssignedSlot {
    pub fn new(slot_id: BuildSlotId, piece: PieceKind) -> Self {
        Self { slot_id, piece }
    }
}
impl AssignedSlot {
    pub fn slot_id(self) -> BuildSlotId {
        self.slot_id
    }
}
impl AssignedSlot {
    pub fn piece(self) -> PieceKind {
        self.piece
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotAssignment {
    assigned_slots: Vec<AssignedSlot>,
}

impl SlotAssignment {
    pub fn new(assigned_slots: Vec<AssignedSlot>) -> Self {
        Self { assigned_slots }
    }
}
impl SlotAssignment {
    pub fn assigned_slots(&self) -> &[AssignedSlot] {
        &self.assigned_slots
    }
}
