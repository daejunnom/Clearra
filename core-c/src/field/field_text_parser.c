#include "clr_field.h"

#include <stddef.h>
static size_t clr_strlen_local(const char *value) {
    size_t len = 0u;
    if (value == 0) {
        return 0u;
    }
    while (value[len] != '\0') {
        len++;
    }
    return len;
}static clr_field_status clr_field_cell_occupied(char cell, bool *out_occupied) {
    if (out_occupied == 0) {
        return CLR_FIELD_INVALID_ARGUMENT;
    }

    switch (cell) {
        case '.':
        case '0':
        case '_':
        case ' ':
            *out_occupied = false;
            return CLR_FIELD_OK;
        case '#':
        case 'X':
        case 'x':
        case '1':
            *out_occupied = true;
            return CLR_FIELD_OK;
        default:
            return CLR_FIELD_INVALID_CELL;
    }
}clr_field_status clr_field_parse_top_down_rows(
    const char *const *rows,
    uint8_t row_count,
    clr_occupancy_field *out_field) {
    if (rows == 0 || out_field == 0) {
        return CLR_FIELD_INVALID_ARGUMENT;
    }
    if (row_count == 0u || rows[0] == 0) {
        return CLR_FIELD_EMPTY_DIMENSIONS;
    }

    size_t width = clr_strlen_local(rows[0]);
    if (width == 0u || width > UINT8_MAX) {
        return CLR_FIELD_EMPTY_DIMENSIONS;
    }
    if ((uint16_t)width * (uint16_t)row_count > 64u) {
        return CLR_FIELD_BOARD64_LIMIT_EXCEEDED;
    }

    uint64_t mask = 0;
    for (uint8_t top_down_row = 0; top_down_row < row_count; top_down_row++) {
        if (rows[top_down_row] == 0) {
            return CLR_FIELD_INVALID_ARGUMENT;
        }
        if (clr_strlen_local(rows[top_down_row]) != width) {
            return CLR_FIELD_ROW_WIDTH_MISMATCH;
        }

        uint8_t internal_y = (uint8_t)(row_count - 1u - top_down_row);
        for (uint8_t x = 0; x < (uint8_t)width; x++) {
            bool occupied = false;
            clr_field_status status =
                clr_field_cell_occupied(rows[top_down_row][x], &occupied);
            if (status != CLR_FIELD_OK) {
                return status;
            }
            if (occupied) {
                uint8_t index = (uint8_t)(internal_y * (uint8_t)width + x);
                mask |= UINT64_C(1) << index;
            }
        }
    }

    return clr_occupancy_field_init((uint8_t)width, row_count, mask, out_field);
}