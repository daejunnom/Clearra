use clearra_core_domain::piece::piece_kind::PieceKind;

use super::hold_slot::HoldSlot;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HoldTransition {
    before: HoldSlot,
    after: HoldSlot,
    active: PieceKind,
}

impl HoldTransition {
    pub fn no_swap(active: PieceKind, hold: HoldSlot) -> Self {
        Self {
            before: hold,
            after: hold,
            active,
        }
    }
}
impl HoldTransition {
    pub fn swap(current: PieceKind, hold: HoldSlot) -> Self {
        let active = hold.piece().unwrap_or(current);
        Self {
            before: hold,
            after: HoldSlot::Occupied(current),
            active,
        }
    }
}
impl HoldTransition {
    pub fn before(self) -> HoldSlot {
        self.before
    }
}
impl HoldTransition {
    pub fn after(self) -> HoldSlot {
        self.after
    }
}
impl HoldTransition {
    pub fn active(self) -> PieceKind {
        self.active
    }
}
