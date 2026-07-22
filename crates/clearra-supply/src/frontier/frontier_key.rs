use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{bag::bag_state::BagState, hold::hold_slot::HoldSlot};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FrontierKey {
    next_index: usize,
    bag_epoch: u16,
    bag_remainder_key: u64,
    hold_piece: Option<PieceKind>,
}

impl FrontierKey {
    pub fn new(next_index: usize, bag_state: BagState, hold_slot: HoldSlot) -> Self {
        Self {
            next_index,
            bag_epoch: bag_state.epoch(),
            bag_remainder_key: bag_state.packed_remainder_key(),
            hold_piece: hold_slot.piece(),
        }
    }
}
impl FrontierKey {
    pub fn next_index(&self) -> usize {
        self.next_index
    }
}
impl FrontierKey {
    pub fn bag_epoch(&self) -> u16 {
        self.bag_epoch
    }
}
impl FrontierKey {
    pub fn bag_remainder_key(&self) -> u64 {
        self.bag_remainder_key
    }
}
impl FrontierKey {
    pub fn hold_piece(&self) -> Option<PieceKind> {
        self.hold_piece
    }
}
