use crate::board::{C_BOARD_BACKEND_BOARD128, C_BOARD_BACKEND_BOARD64, C_BOARD_BACKEND_WIDE};

use super::{CBoardDescriptor, C_BUILDUP_MAX_OPERATIONS};

pub const C_BUILDUP_STATUS_OK: u32 = 0;
pub const C_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE: u32 = 17;

pub fn buildup_runtime_status_for_board(board: &CBoardDescriptor) -> u32 {
    match board.backend_kind {
        C_BOARD_BACKEND_BOARD64 => C_BUILDUP_STATUS_OK,
        C_BOARD_BACKEND_BOARD128 | C_BOARD_BACKEND_WIDE => C_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE,
        _ => C_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE,
    }
}

pub fn buildup_operation_set_runtime_status(operation_count: u32) -> u32 {
    if operation_count as usize <= C_BUILDUP_MAX_OPERATIONS {
        C_BUILDUP_STATUS_OK
    } else {
        C_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE
    }
}

#[cfg(test)]
#[path = "generic_buildup_tests.rs"]
mod tests;
