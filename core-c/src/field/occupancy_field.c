#include "clr_field.h"
static uint64_t clr_field_mask_for_cell_count(uint16_t cells) {
    return cells == 64u ? UINT64_MAX : ((UINT64_C(1) << cells) - UINT64_C(1));
}clr_field_status clr_occupancy_field_init(
    uint8_t width,
    uint8_t height,
    uint64_t mask,
    clr_occupancy_field *out_field) {
    if (out_field == 0) {
        return CLR_FIELD_INVALID_ARGUMENT;
    }
    if (width == 0u || height == 0u) {
        return CLR_FIELD_EMPTY_DIMENSIONS;
    }

    uint16_t cells = (uint16_t)width * (uint16_t)height;
    if (cells > 64u) {
        return CLR_FIELD_BOARD64_LIMIT_EXCEEDED;
    }
    uint64_t field_mask = clr_field_mask_for_cell_count(cells);
    if ((mask & ~field_mask) != 0u) {
        return CLR_FIELD_MASK_OUTSIDE_FIELD;
    }

    out_field->mask = mask;
    out_field->width = width;
    out_field->height = height;
    out_field->reserved = 0u;
    return CLR_FIELD_OK;
}