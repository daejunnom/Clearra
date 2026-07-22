#include "target_frame_projection.h"

static bool target_row_for_current_row(
    uint8_t target_height,
    uint16_t deleted_row_mask,
    uint8_t current_row,
    uint8_t *out_target_row) {
    uint8_t visible_row = 0u;
    for (uint8_t target_row = 0u; target_row < target_height; ++target_row) {
        if ((deleted_row_mask & (uint16_t)(UINT16_C(1) << target_row)) != 0u) {
            continue;
        }
        if (visible_row == current_row) {
            *out_target_row = target_row;
            return true;
        }
        visible_row++;
    }
    return false;
}

ClearraPackingStatus clearra_target_frame_project_lock_operation(
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint8_t rotation,
    int8_t lock_x,
    int8_t lock_y,
    uint16_t deleted_row_mask,
    uint64_t *out_target_mask,
    int8_t *out_target_y) {
    if (!clearra_board64_layout_is_valid(layout) || layout.height > 16u ||
        out_target_mask == 0 ||
        out_target_y == 0 || lock_x < 0 || lock_y < 0 ||
        (layout.height < 16u &&
         (deleted_row_mask >> layout.height) != 0u)) {
        return CLEARRA_PACKING_OUT_OF_BOUNDS;
    }

    ClearraOperation operation;
    if (clearra_operation_from_shape(piece, rotation, &operation) !=
        CLEARRA_OPERATION_OK) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    uint8_t target_y = 0u;
    if (!target_row_for_current_row(
            layout.height,
            deleted_row_mask,
            (uint8_t)lock_y,
            &target_y)) {
        return CLEARRA_PACKING_OUT_OF_BOUNDS;
    }

    uint64_t target_mask = 0u;
    for (uint8_t index = 0u; index < operation.area; ++index) {
        int16_t current_x = (int16_t)lock_x + operation.cells[index].x;
        int16_t current_y = (int16_t)lock_y + operation.cells[index].y;
        if (current_x < 0 || current_x >= layout.width || current_y < 0 ||
            current_y >= layout.height) {
            return CLEARRA_PACKING_OUT_OF_BOUNDS;
        }
        uint8_t target_cell_y = 0u;
        if (!target_row_for_current_row(
                layout.height,
                deleted_row_mask,
                (uint8_t)current_y,
                &target_cell_y)) {
            return CLEARRA_PACKING_OUT_OF_BOUNDS;
        }
        uint64_t cell = 0u;
        if (clearra_board64_cell_mask(
                layout,
                (uint8_t)current_x,
                target_cell_y,
                &cell) != CLEARRA_BOARD64_OK) {
            return CLEARRA_PACKING_OUT_OF_BOUNDS;
        }
        target_mask |= cell;
    }

    *out_target_mask = target_mask;
    *out_target_y = (int8_t)target_y;
    return CLEARRA_PACKING_OK;
}

ClearraPackingStatus clearra_target_frame_merge_deleted_rows(
    uint8_t target_height,
    uint16_t previous_deleted_row_mask,
    uint16_t current_deleted_row_mask,
    uint16_t *out_deleted_row_mask) {
    if (out_deleted_row_mask == 0 || target_height == 0u ||
        target_height > 16u ||
        (previous_deleted_row_mask >> target_height) != 0u ||
        (current_deleted_row_mask >> target_height) != 0u) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    uint16_t original_rows = 0u;
    for (uint8_t current_row = 0u;
         current_row < target_height;
         ++current_row) {
        uint16_t current_bit =
            (uint16_t)(UINT16_C(1) << current_row);
        if ((current_deleted_row_mask & current_bit) == 0u) {
            continue;
        }
        uint8_t target_row = 0u;
        if (!target_row_for_current_row(
                target_height,
                previous_deleted_row_mask,
                current_row,
                &target_row)) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        original_rows |= (uint16_t)(UINT16_C(1) << target_row);
    }
    *out_deleted_row_mask =
        (uint16_t)(previous_deleted_row_mask | original_rows);
    return CLEARRA_PACKING_OK;
}
