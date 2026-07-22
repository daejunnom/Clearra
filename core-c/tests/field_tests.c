#include "clr_field.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_STATUS(EXPR, EXPECTED)                                                   \
    do {                                                                                \
        clr_field_status actual_status = (EXPR);                                        \
        if (actual_status != (EXPECTED)) {                                              \
            fprintf(stderr, "%s:%d expected status %d but got %d\n", __FILE__, __LINE__, \
                    (int)(EXPECTED), (int)actual_status);                               \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_U64(EXPR, EXPECTED)                                                      \
    do {                                                                                \
        uint64_t actual_value = (uint64_t)(EXPR);                                       \
        uint64_t expected_value = (uint64_t)(EXPECTED);                                 \
        if (actual_value != expected_value) {                                           \
            fprintf(stderr, "%s:%d expected 0x%llx but got 0x%llx\n", __FILE__, __LINE__, \
                    (unsigned long long)expected_value,                                 \
                    (unsigned long long)actual_value);                                  \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_U8(EXPR, EXPECTED)                                                       \
    do {                                                                                \
        uint8_t actual_value = (uint8_t)(EXPR);                                         \
        uint8_t expected_value = (uint8_t)(EXPECTED);                                   \
        if (actual_value != expected_value) {                                           \
            fprintf(stderr, "%s:%d expected %u but got %u\n", __FILE__, __LINE__,       \
                    (unsigned)expected_value, (unsigned)actual_value);                  \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_TRUE(EXPR)                                                               \
    do {                                                                                \
        if (!(EXPR)) {                                                                  \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);              \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)
static void text_field_top_down_parses_to_bottom_up_mask(void) {
    const char *rows[] = {"#.", ".#"};
    clr_occupancy_field field = {0};

    EXPECT_STATUS(clr_field_parse_top_down_rows(rows, 2u, &field), CLR_FIELD_OK);
    EXPECT_U8(field.width, 2u);
    EXPECT_U8(field.height, 2u);
    EXPECT_U64(field.mask, 0x6u);
}static void occupancy_field_has_no_color(void) {
    clr_occupancy_field field = {0};

    EXPECT_STATUS(clr_occupancy_field_init(10u, 2u, 0x3ffu, &field), CLR_FIELD_OK);
    EXPECT_U8(field.width, 10u);
    EXPECT_U8(field.height, 2u);
    EXPECT_U64(field.mask, 0x3ffu);
}static void occupancy_field_has_no_owner(void) {
    clr_occupancy_field field = {0};
    bool occupied = false;

    EXPECT_STATUS(clr_occupancy_field_init(4u, 4u, 0x2u, &field), CLR_FIELD_OK);
    EXPECT_STATUS(clr_field_is_occupied(field, 1u, 0u, &occupied), CLR_FIELD_OK);
    EXPECT_TRUE(occupied);
}static void bit_index_uses_bottom_up_row_major_coordinates(void) {
    clr_occupancy_field field = {0};
    uint8_t index = 0;

    EXPECT_STATUS(clr_occupancy_field_init(10u, 4u, 0u, &field), CLR_FIELD_OK);
    EXPECT_STATUS(clr_field_bit_index(field, 3u, 2u, &index), CLR_FIELD_OK);
    EXPECT_U8(index, 23u);
}static void parser_rejects_invalid_cell(void) {
    const char *rows[] = {"?."};
    clr_occupancy_field field = {0};

    EXPECT_STATUS(
        clr_field_parse_top_down_rows(rows, 1u, &field),
        CLR_FIELD_INVALID_CELL);
}int main(void) {
    text_field_top_down_parses_to_bottom_up_mask();
    occupancy_field_has_no_color();
    occupancy_field_has_no_owner();
    bit_index_uses_bottom_up_row_major_coordinates();
    parser_rejects_invalid_cell();

    puts("field_tests passed");
    return 0;
}