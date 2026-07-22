#include "generic_buildup.h"
uint16_t clearra_buildup_mvp1_max_operations(void) {
    return CLR_BUILDUP_MAX_OPERATIONS;
}clr_buildup_status clearra_buildup_operation_set_runtime_status(
    uint32_t operation_count) {
    if (operation_count <= CLR_BUILDUP_MAX_OPERATIONS) {
        return CLR_BUILDUP_OK;
    }
    return CLR_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE;
}clr_buildup_status clearra_buildup_runtime_status_for_board(
    const clr_board_descriptor *board) {
    if (board == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (board->backend_kind == CLR_BOARD_BACKEND_BOARD64) {
        return CLR_BUILDUP_OK;
    }
    if (board->backend_kind == CLR_BOARD_BACKEND_BOARD128 ||
        board->backend_kind == CLR_BOARD_BACKEND_WIDE) {
        return CLR_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE;
    }
    return CLR_BUILDUP_INVALID_PROBLEM;
}
