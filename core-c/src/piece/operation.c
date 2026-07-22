#include "operation.h"

#include <string.h>
static ClearraOperationBounds bounds_for_shape(const ClearraPieceShape *shape) {
    ClearraOperationBounds bounds;
    bounds.min_x = shape->cells[0].x;
    bounds.min_y = shape->cells[0].y;
    bounds.max_x = shape->cells[0].x;
    bounds.max_y = shape->cells[0].y;

    for (uint8_t index = 1; index < shape->area; index++) {
        if (shape->cells[index].x < bounds.min_x) {
            bounds.min_x = shape->cells[index].x;
        }
        if (shape->cells[index].x > bounds.max_x) {
            bounds.max_x = shape->cells[index].x;
        }
        if (shape->cells[index].y < bounds.min_y) {
            bounds.min_y = shape->cells[index].y;
        }
        if (shape->cells[index].y > bounds.max_y) {
            bounds.max_y = shape->cells[index].y;
        }
    }

    bounds.width = (uint8_t)(bounds.max_x - bounds.min_x + 1);
    bounds.height = (uint8_t)(bounds.max_y - bounds.min_y + 1);
    return bounds;
}static ClearraOperationStatus base_shape_mask(
    const ClearraPieceShape *shape,
    uint64_t *out_mask) {
    ClearraBoard64Layout layout;
    ClearraBoard64Status layout_status =
        clearra_board64_make_layout(4, 4, &layout);
    if (layout_status != CLEARRA_BOARD64_OK) {
        return CLEARRA_OPERATION_INVALID_ARGUMENT;
    }

    uint64_t mask = 0;
    for (uint8_t index = 0; index < shape->area; index++) {
        uint64_t cell_mask = 0;
        ClearraBoard64Status cell_status = clearra_board64_cell_mask(
            layout,
            (uint8_t)shape->cells[index].x,
            (uint8_t)shape->cells[index].y,
            &cell_mask);
        if (cell_status != CLEARRA_BOARD64_OK) {
            return CLEARRA_OPERATION_OUT_OF_BOUNDS;
        }
        mask |= cell_mask;
    }

    *out_mask = mask;
    return CLEARRA_OPERATION_OK;
}ClearraOperationStatus clearra_operation_id(
    uint8_t piece,
    uint8_t rotation,
    uint16_t *out_operation_id) {
    if (out_operation_id == 0) {
        return CLEARRA_OPERATION_INVALID_ARGUMENT;
    }
    if (!clearra_piece_is_standard_tetromino(piece)) {
        return CLEARRA_OPERATION_INVALID_PIECE;
    }
    if (!clearra_rotation_state_is_valid(rotation)) {
        return CLEARRA_OPERATION_INVALID_ROTATION;
    }

    *out_operation_id =
        (uint16_t)((uint16_t)(piece - CLR_PIECE_I) * CLEARRA_ROTATION_STATE_COUNT +
                   (uint16_t)rotation);
    return CLEARRA_OPERATION_OK;
}ClearraOperationStatus clearra_operation_from_shape(
    uint8_t piece,
    uint8_t rotation,
    ClearraOperation *out_operation) {
    const ClearraPieceShape *shape = 0;
    ClearraOperationStatus status = clearra_tetromino_shape(piece, rotation, &shape);
    if (status != CLEARRA_OPERATION_OK) {
        return status;
    }
    if (out_operation == 0) {
        return CLEARRA_OPERATION_INVALID_ARGUMENT;
    }

    memset(out_operation, 0, sizeof(*out_operation));
    out_operation->piece = shape->piece;
    out_operation->rotation = shape->rotation;
    out_operation->area = shape->area;
    for (uint8_t index = 0; index < shape->area; index++) {
        out_operation->cells[index] = shape->cells[index];
    }
    out_operation->bounds = bounds_for_shape(shape);

    status = clearra_operation_id(piece, rotation, &out_operation->operation_id);
    if (status != CLEARRA_OPERATION_OK) {
        return status;
    }
    return base_shape_mask(shape, &out_operation->shape_mask);
}ClearraOperationStatus clearra_operation_mask(
    ClearraBoard64Layout layout,
    const ClearraOperation *operation,
    int8_t x,
    int8_t y,
    uint64_t *out_mask) {
    if (operation == 0 || out_mask == 0) {
        return CLEARRA_OPERATION_INVALID_ARGUMENT;
    }
    if (!clearra_board64_layout_is_valid(layout)) {
        return CLEARRA_OPERATION_INVALID_ARGUMENT;
    }
    if (x < 0 || y < 0) {
        return CLEARRA_OPERATION_OUT_OF_BOUNDS;
    }
    if ((uint16_t)x + operation->bounds.width > layout.width ||
        (uint16_t)y + operation->bounds.height > layout.height) {
        return CLEARRA_OPERATION_OUT_OF_BOUNDS;
    }

    uint64_t mask = 0;
    for (uint8_t index = 0; index < operation->area; index++) {
        uint64_t cell_mask = 0;
        uint8_t cell_x = (uint8_t)(x + operation->cells[index].x);
        uint8_t cell_y = (uint8_t)(y + operation->cells[index].y);
        ClearraBoard64Status cell_status =
            clearra_board64_cell_mask(layout, cell_x, cell_y, &cell_mask);
        if (cell_status != CLEARRA_BOARD64_OK) {
            return CLEARRA_OPERATION_OUT_OF_BOUNDS;
        }
        mask |= cell_mask;
    }

    *out_mask = mask;
    return CLEARRA_OPERATION_OK;
}