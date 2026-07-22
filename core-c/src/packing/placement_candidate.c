#include "packing_problem.h"
#include "target_frame_projection.h"

#include <string.h>
static ClearraPackingStatus packing_status_from_operation_status(
    ClearraOperationStatus status) {
    if (status == CLEARRA_OPERATION_OK) {
        return CLEARRA_PACKING_OK;
    }
    if (status == CLEARRA_OPERATION_INVALID_PIECE) {
        return CLEARRA_PACKING_INVALID_PIECE;
    }
    if (status == CLEARRA_OPERATION_OUT_OF_BOUNDS) {
        return CLEARRA_PACKING_OUT_OF_BOUNDS;
    }
    return CLEARRA_PACKING_INVALID_ARGUMENT;
}

static uint8_t count_deleted_rows(uint16_t deleted_row_mask) {
    uint8_t count = 0u;
    while (deleted_row_mask != 0u) {
        count = (uint8_t)(count + (uint8_t)(deleted_row_mask & 1u));
        deleted_row_mask = (uint16_t)(deleted_row_mask >> 1u);
    }
    return count;
}

ClearraPackingStatus clearra_placement_geometry_variants_at_deleted_rows(
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint64_t geometry_mask,
    uint16_t deleted_row_mask,
    ClearraPlacementCandidate
        out_variants[CLEARRA_PACKING_MAX_GEOMETRY_VARIANTS],
    uint8_t *out_count) {
    if (out_variants == 0 || out_count == 0 ||
        !clearra_board64_layout_is_valid(layout) || layout.height > 16u ||
        !clearra_piece_is_standard_tetromino(piece) || geometry_mask == 0u ||
        (geometry_mask & ~layout.all_cells_mask) != 0u ||
        (layout.height < 16u && (deleted_row_mask >> layout.height) != 0u)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    *out_count = 0u;
    uint8_t deleted_count = count_deleted_rows(deleted_row_mask);
    if (deleted_count >= layout.height) {
        return CLEARRA_PACKING_OK;
    }

    for (uint8_t rotation = 0u;
         rotation < clearra_rotation_count_for_piece(piece);
         ++rotation) {
        ClearraOperation operation;
        ClearraOperationStatus operation_status =
            clearra_operation_from_shape(piece, rotation, &operation);
        if (operation_status != CLEARRA_OPERATION_OK) {
            return packing_status_from_operation_status(operation_status);
        }
        int16_t max_x = (int16_t)layout.width - (int16_t)operation.bounds.width;
        int16_t current_height = (int16_t)layout.height - deleted_count;
        int16_t max_current_y =
            current_height - (int16_t)operation.bounds.height;
        if (max_x < 0 || max_current_y < 0) {
            continue;
        }
        for (int16_t current_y = 0; current_y <= max_current_y; ++current_y) {
            for (int16_t x = 0; x <= max_x; ++x) {
                uint64_t mask = 0u;
                int8_t target_y = 0;
                ClearraPackingStatus status =
                    clearra_target_frame_project_lock_operation(
                    layout,
                    piece,
                    rotation,
                    (int8_t)x,
                    (int8_t)current_y,
                    deleted_row_mask,
                    &mask,
                    &target_y);
                if (status != CLEARRA_PACKING_OK || mask != geometry_mask) {
                    continue;
                }
                if (*out_count >= CLEARRA_PACKING_MAX_GEOMETRY_VARIANTS) {
                    return CLEARRA_PACKING_CAPACITY_EXCEEDED;
                }
                out_variants[*out_count] = (ClearraPlacementCandidate){
                    .piece = piece,
                    .rotation = rotation,
                    .x = (int8_t)x,
                    .y = target_y,
                    .operation_id = operation.operation_id,
                    .required_deleted_row_mask = deleted_row_mask,
                    .mask = mask,
                };
                (*out_count)++;
            }
        }
    }
    return CLEARRA_PACKING_OK;
}

void clearra_placement_candidate_list_clear(ClearraPlacementCandidateList *list) {
    if (list != 0) {
        list->count = 0u;
    }
}ClearraPackingStatus clearra_placement_candidate_list_push(
    ClearraPlacementCandidateList *list,
    ClearraPlacementCandidate candidate) {
    if (list == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    for (uint16_t index = 0; index < list->count; index++) {
        const ClearraPlacementCandidate *existing = &list->candidates[index];
        if (existing->piece == candidate.piece &&
            existing->rotation == candidate.rotation &&
            existing->operation_id == candidate.operation_id &&
            existing->required_deleted_row_mask ==
                candidate.required_deleted_row_mask &&
            existing->mask == candidate.mask &&
            existing->x == candidate.x &&
            existing->y == candidate.y) {
            return CLEARRA_PACKING_OK;
        }
    }

    if (list->count >= CLEARRA_PACKING_MAX_PLACEMENT_CANDIDATES) {
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }

    list->candidates[list->count] = candidate;
    list->count++;
    return CLEARRA_PACKING_OK;
}
