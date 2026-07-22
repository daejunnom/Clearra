pub use crate::packing_problem::{CPackingCandidate, CPackingOperation, CPackingProblem};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CPackingState {
    pub board_mask: u64,
    pub cursor: u16,
    pub hold_piece: u8,
    pub hold_empty: u8,
    pub placed_pieces: u16,
    pub cleared_lines: u8,
    pub reserved: u8,
}

impl CPackingState {
    pub fn empty(board_mask: u64) -> Self {
        Self {
            board_mask,
            hold_empty: 1,
            ..Self::default()
        }
    }
}
