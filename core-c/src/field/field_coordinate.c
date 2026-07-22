#include "clr_field.h"
clr_field_status clr_field_bit_index(
    clr_occupancy_field field,
    uint8_t x,
    uint8_t y,
    uint8_t *out_index) {
    if (out_index == 0) {
        return CLR_FIELD_INVALID_ARGUMENT;
    }
    if (field.width == 0u || field.height == 0u) {
        return CLR_FIELD_EMPTY_DIMENSIONS;
    }
    if (x >= field.width || y >= field.height) {
        return CLR_FIELD_COORDINATE_OUT_OF_BOUNDS;
    }

    *out_index = (uint8_t)(y * field.width + x);
    return CLR_FIELD_OK;
}clr_field_status clr_field_is_occupied(
    clr_occupancy_field field,
    uint8_t x,
    uint8_t y,
    bool *out_occupied) {
    if (out_occupied == 0) {
        return CLR_FIELD_INVALID_ARGUMENT;
    }

    uint8_t index = 0;
    clr_field_status status = clr_field_bit_index(field, x, y, &index);
    if (status != CLR_FIELD_OK) {
        return status;
    }

    *out_occupied = (field.mask & (UINT64_C(1) << index)) != 0u;
    return CLR_FIELD_OK;
}