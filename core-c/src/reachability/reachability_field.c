#include "reachability_field.h"

static ClearraReachabilityStatus operation_for(
    uint8_t piece,
    uint8_t rotation,
    ClearraOperation *out_operation) {
    ClearraOperationStatus status =
        clearra_operation_from_shape(piece, rotation, out_operation);
    if (status == CLEARRA_OPERATION_OK) {
        return CLEARRA_REACHABILITY_OK;
    }
    if (status == CLEARRA_OPERATION_INVALID_ARGUMENT) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    return CLEARRA_REACHABILITY_INVALID_OPERATION;
}

static ClearraReachabilityStatus placeable_with_mask(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_placeable,
    uint64_t *out_visible_mask) {
    if (!clearra_board64_layout_is_valid(layout) || out_placeable == 0 ||
        out_visible_mask == 0 || (board & ~layout.all_cells_mask) != 0u) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    *out_placeable = false;
    *out_visible_mask = 0u;

    ClearraOperation operation;
    ClearraReachabilityStatus status =
        operation_for(piece, rotation, &operation);
    if (status != CLEARRA_REACHABILITY_OK) {
        return status;
    }
    if (x < 0 || y < 0 ||
        (uint16_t)x + operation.bounds.width > layout.width) {
        return CLEARRA_REACHABILITY_OK;
    }

    uint64_t visible_mask = 0u;
    for (uint8_t index = 0u; index < operation.area; ++index) {
        int16_t cell_x = (int16_t)x + operation.cells[index].x;
        int16_t cell_y = (int16_t)y + operation.cells[index].y;
        if (cell_x < 0 || cell_x >= layout.width || cell_y < 0) {
            return CLEARRA_REACHABILITY_OK;
        }
        if (cell_y >= layout.height) {
            continue;
        }

        uint64_t cell_mask = 0u;
        if (clearra_board64_cell_mask(
                layout, (uint8_t)cell_x, (uint8_t)cell_y, &cell_mask) !=
            CLEARRA_BOARD64_OK) {
            return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
        }
        if ((board & cell_mask) != 0u) {
            return CLEARRA_REACHABILITY_OK;
        }
        visible_mask |= cell_mask;
    }

    *out_visible_mask = visible_mask;
    *out_placeable = true;
    return CLEARRA_REACHABILITY_OK;
}

ClearraReachabilityStatus clearra_reachability_field_is_placeable(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_placeable) {
    uint64_t ignored_mask = 0u;
    return placeable_with_mask(
        layout, board, piece, rotation, x, y, out_placeable, &ignored_mask);
}

ClearraReachabilityStatus clearra_reachability_field_is_grounded(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_grounded) {
    if (out_grounded == 0) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    *out_grounded = y == 0;
    if (*out_grounded) {
        return CLEARRA_REACHABILITY_OK;
    }

    bool below_placeable = false;
    ClearraReachabilityStatus status =
        clearra_reachability_field_is_placeable(
            layout, board, piece, rotation, x, (int8_t)(y - 1),
            &below_placeable);
    if (status == CLEARRA_REACHABILITY_OK) {
        *out_grounded = !below_placeable;
    }
    return status;
}

ClearraReachabilityStatus clearra_reachability_field_has_harddrop_path(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_reachable) {
    if (out_reachable == 0) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    *out_reachable = false;

    bool placeable = false;
    ClearraReachabilityStatus status =
        clearra_reachability_field_is_placeable(
            layout, board, piece, rotation, x, y, &placeable);
    if (status != CLEARRA_REACHABILITY_OK || !placeable) {
        return status;
    }
    bool grounded = false;
    status = clearra_reachability_field_is_grounded(
        layout, board, piece, rotation, x, y, &grounded);
    if (status != CLEARRA_REACHABILITY_OK || !grounded) {
        return status;
    }

    for (int16_t cursor_y = (int16_t)y + 1; cursor_y <= layout.height;
         ++cursor_y) {
        status = clearra_reachability_field_is_placeable(
            layout, board, piece, rotation, x, (int8_t)cursor_y, &placeable);
        if (status != CLEARRA_REACHABILITY_OK) {
            return status;
        }
        if (!placeable) {
            return CLEARRA_REACHABILITY_OK;
        }
    }

    *out_reachable = true;
    return CLEARRA_REACHABILITY_OK;
}

ClearraReachabilityStatus clearra_reachability_field_first_success_kick(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    int8_t x,
    int8_t y,
    const ClearraReachabilityKickTable *kick_table,
    ClearraCandidateOperation *out_operation) {
    if (out_operation == 0) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    const ClearraKickOffset *offsets = 0;
    uint8_t offset_count = 0u;
    ClearraReachabilityStatus status =
        clearra_reachability_kick_offsets_for_transition(
            kick_table, from_rotation, to_rotation, &offsets, &offset_count);
    if (status != CLEARRA_REACHABILITY_OK) {
        return status;
    }

    for (uint8_t index = 0u; index < offset_count; ++index) {
        int8_t normalized_dx = 0;
        int8_t normalized_dy = 0;
        if (clearra_candidate_normalized_kick_delta(
                piece, from_rotation, to_rotation, offsets[index].dx,
                offsets[index].dy, &normalized_dx, &normalized_dy) !=
            CLEARRA_CANDIDATE_OK) {
            return CLEARRA_REACHABILITY_INVALID_OPERATION;
        }
        int8_t candidate_x = (int8_t)(x + normalized_dx);
        int8_t candidate_y = (int8_t)(y + normalized_dy);
        bool placeable = false;
        uint64_t visible_mask = 0u;
        status = placeable_with_mask(
            layout, board, piece, to_rotation, candidate_x, candidate_y,
            &placeable, &visible_mask);
        if (status != CLEARRA_REACHABILITY_OK) {
            return status;
        }
        if (!placeable) {
            continue;
        }

        ClearraRotationTransitionKind transition =
            CLEARRA_ROTATION_TRANSITION_NONE;
        if (clearra_candidate_transition_kind(
                from_rotation, to_rotation, &transition) !=
            CLEARRA_CANDIDATE_OK) {
            return CLEARRA_REACHABILITY_INVALID_OPERATION;
        }
        *out_operation = (ClearraCandidateOperation){
            piece,
            to_rotation,
            candidate_x,
            candidate_y,
            visible_mask,
            (uint8_t)transition,
            index,
            offsets[index].dx,
            offsets[index].dy,
        };
        return CLEARRA_REACHABILITY_OK;
    }
    return CLEARRA_REACHABILITY_UNREACHABLE;
}
