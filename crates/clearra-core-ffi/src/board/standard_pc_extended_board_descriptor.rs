use clearra_core_domain::board::standard_pc_board::StandardPcBoardStorageKind;
use clearra_pc_graph::request::ExtendedPcScenarioBoard;

use super::{
    C_BOARD_BACKEND_BOARD128, C_BOARD_BACKEND_BOARD256, C_STANDARD_PC_BOARD_WIDTH,
    C_STANDARD_PC_BOARD_WORD_CAPACITY,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CStandardPcExtendedBoardDescriptor {
    pub width: u16,
    pub target_lines: u16,
    pub cell_count: u16,
    pub word_count: u16,
    pub backend_kind: u32,
    pub reserved: u32,
    pub initial_words: [u64; C_STANDARD_PC_BOARD_WORD_CAPACITY],
}

impl From<ExtendedPcScenarioBoard> for CStandardPcExtendedBoardDescriptor {
    fn from(board: ExtendedPcScenarioBoard) -> Self {
        let target_lines = u16::from(board.visible_height());
        let (word_count, backend_kind) = match board.storage_kind() {
            StandardPcBoardStorageKind::Board128 => (2, C_BOARD_BACKEND_BOARD128),
            StandardPcBoardStorageKind::Board256 => (4, C_BOARD_BACKEND_BOARD256),
            StandardPcBoardStorageKind::Board64 => {
                unreachable!("extended scenario boards cannot use Board64 storage")
            }
        };
        Self {
            width: C_STANDARD_PC_BOARD_WIDTH,
            target_lines,
            cell_count: C_STANDARD_PC_BOARD_WIDTH * target_lines,
            word_count,
            backend_kind,
            reserved: 0,
            initial_words: board.occupied_words(),
        }
    }
}
