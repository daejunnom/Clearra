#include "buildup_internal.h"

clr_buildup_status clearra_buildup_order_from_problem(
    const clr_buildup_problem *problem,
    ClearraBuildUpOrder *out_order) {
    if (problem == 0 || out_order == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }

    uint16_t count = problem->operation_set.operation_count;
    if (count == 0 || count > CLR_BUILDUP_MAX_OPERATIONS) {
        return CLR_BUILDUP_INVALID_ORDER;
    }

    bool seen[CLR_BUILDUP_MAX_OPERATIONS] = {false};
    out_order->count = count;
    for (uint16_t index = 0; index < count; index++) {
        uint16_t operation_index = problem->operation_set.representative_order_hint[index];
        if (operation_index >= count || seen[operation_index]) {
            return CLR_BUILDUP_INVALID_ORDER;
        }
        seen[operation_index] = true;
        out_order->indices[index] = operation_index;
    }

    return CLR_BUILDUP_OK;
}

#include "buildup_internal.h"

clr_buildup_status clearra_buildup_check_line_clear_dependency(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t placement_mask) {
    bool collision = false;
    ClearraBoard64Status status =
        clearra_board64_collision(layout, board, placement_mask, &collision);
    if (status == CLEARRA_BOARD64_MASK_OUTSIDE_LAYOUT ||
        status == CLEARRA_BOARD64_OUT_OF_BOUNDS) {
        return CLR_BUILDUP_LINE_CLEAR_DEPENDENCY_IMPOSSIBLE;
    }
    if (status != CLEARRA_BOARD64_OK) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    if (collision) {
        return CLR_BUILDUP_LINE_CLEAR_DEPENDENCY_IMPOSSIBLE;
    }
    return CLR_BUILDUP_OK;
}
