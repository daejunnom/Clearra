#ifndef CLR_FIELD_H
#define CLR_FIELD_H

#include <stdbool.h>
#include <stdint.h>
typedef enum clr_field_status {
    CLR_FIELD_OK = 0,
    CLR_FIELD_INVALID_ARGUMENT = 1,
    CLR_FIELD_EMPTY_DIMENSIONS = 2,
    CLR_FIELD_BOARD64_LIMIT_EXCEEDED = 3,
    CLR_FIELD_MASK_OUTSIDE_FIELD = 4,
    CLR_FIELD_COORDINATE_OUT_OF_BOUNDS = 5,
    CLR_FIELD_ROW_WIDTH_MISMATCH = 6,
    CLR_FIELD_INVALID_CELL = 7
} clr_field_status;typedef struct clr_occupancy_field {
    uint64_t mask;
    uint8_t width;
    uint8_t height;
    uint16_t reserved;
} clr_occupancy_field;clr_field_status clr_occupancy_field_init(
    uint8_t width,
    uint8_t height,
    uint64_t mask,
    clr_occupancy_field *out_field);
clr_field_status clr_field_bit_index(
    clr_occupancy_field field,
    uint8_t x,
    uint8_t y,
    uint8_t *out_index);
clr_field_status clr_field_is_occupied(
    clr_occupancy_field field,
    uint8_t x,
    uint8_t y,
    bool *out_occupied);
clr_field_status clr_field_parse_top_down_rows(
    const char *const *rows,
    uint8_t row_count,
    clr_occupancy_field *out_field);
#endif
