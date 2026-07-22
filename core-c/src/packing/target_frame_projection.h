#ifndef CLEARRA_TARGET_FRAME_PROJECTION_H
#define CLEARRA_TARGET_FRAME_PROJECTION_H

#include "packing_problem.h"

ClearraPackingStatus clearra_target_frame_project_lock_operation(
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint8_t rotation,
    int8_t lock_x,
    int8_t lock_y,
    uint16_t deleted_row_mask,
    uint64_t *out_target_mask,
    int8_t *out_target_y);

ClearraPackingStatus clearra_target_frame_merge_deleted_rows(
    uint8_t target_height,
    uint16_t previous_deleted_row_mask,
    uint16_t current_deleted_row_mask,
    uint16_t *out_deleted_row_mask);

#endif
