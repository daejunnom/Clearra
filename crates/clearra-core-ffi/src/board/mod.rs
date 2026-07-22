pub const C_BOARD_BACKEND_BOARD64: u32 = 1;
pub const C_BOARD_BACKEND_BOARD128: u32 = 2;
pub const C_BOARD_BACKEND_WIDE: u32 = 3;
pub const C_BOARD_BACKEND_BOARD256: u32 = 4;
pub const C_STANDARD_PC_BOARD_WIDTH: u16 = 10;
pub const C_STANDARD_PC_COMPACT_MAX_LINES: u16 = 6;
pub const C_STANDARD_PC_EXTENDED_MIN_LINES: u16 = 7;
pub const C_STANDARD_PC_MAX_LINES: u16 = 24;
pub const C_STANDARD_PC_BOARD_WORD_CAPACITY: usize = 4;
pub const C_BOARD_UNSUPPORTED_REASON_NONE: u32 = 0;
pub const C_BOARD_UNSUPPORTED_REASON_BOARD_WIDTH_OUT_OF_SCOPE: u32 = 1;
pub const C_BOARD_UNSUPPORTED_REASON_BOARD_BACKEND_NOT_CONNECTED: u32 = 2;
pub const C_BOARD_UNSUPPORTED_REASON_WIDE_BOARD_RUNTIME_NOT_CONNECTED: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CBoard64Status {
    Ok = 0,
    InvalidLayout = 1,
    OutOfBounds = 2,
    MaskOutsideLayout = 3,
    Collision = 4,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CBoardStatus {
    Ok = 0,
    InvalidLayout = 1,
    OutOfBounds = 2,
    MaskOutsideLayout = 3,
    Collision = 4,
    UnsupportedBackend = 5,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CBoard64Layout {
    pub width: u8,
    pub height: u8,
    pub cell_count: u16,
    pub all_cells_mask: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CBoard64LineClearResult {
    pub board: u64,
    pub deleted_row_mask: u16,
    pub cleared_lines: u8,
    pub reserved: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CBoardBackendCapability {
    pub backend_kind: u32,
    pub descriptor_supported: u8,
    pub basic_ops_supported: u8,
    pub operation_mask_supported: u8,
    pub runtime_connected: u8,
    pub packing_supported: u8,
    pub reserved: [u8; 3],
    pub unsupported_reason: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CBoard128Descriptor {
    pub width: u16,
    pub height: u16,
    pub cell_count: u16,
    pub reserved: u16,
    pub all_cells_mask_lo: u64,
    pub all_cells_mask_hi: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CBoard256Descriptor {
    pub width: u16,
    pub height: u16,
    pub cell_count: u16,
    pub word_count: u16,
    pub all_cells_mask: [u64; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CWideBoardDescriptor {
    pub width: u16,
    pub height: u16,
    pub cell_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CGenericBoardMask {
    pub backend_kind: u32,
    pub word_count: u32,
    pub words: [u64; 4],
    pub wide_start: u32,
    pub wide_len: u32,
}

mod standard_pc_extended_board_descriptor;
pub use standard_pc_extended_board_descriptor::CStandardPcExtendedBoardDescriptor;

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
