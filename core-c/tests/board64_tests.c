#include "../src/board/board64.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

#define EXPECT_STATUS(EXPR, EXPECTED)                                                   \
    do {                                                                                \
        ClearraBoard64Status actual_status = (EXPR);                                    \
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

#define EXPECT_TRUE(EXPR)                                                               \
    do {                                                                                \
        if (!(EXPR)) {                                                                  \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);              \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                              \
    do {                                                                                \
        if ((EXPR)) {                                                                   \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__);             \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)
static ClearraBoard64Layout standard_10x4(void) {
    ClearraBoard64Layout layout;
    EXPECT_STATUS(clearra_board64_make_layout(10, 4, &layout), CLEARRA_BOARD64_OK);
    return layout;
}static void empty_board_is_zero(void) {
    EXPECT_U64(clearra_board64_empty(), 0);
}static void cell_index_mapping_is_bottom_left_row_major(void) {
    ClearraBoard64Layout layout = standard_10x4();
    uint8_t index = 0;
    EXPECT_STATUS(clearra_board64_cell_index(layout, 3, 2, &index), CLEARRA_BOARD64_OK);
    EXPECT_U64(index, 23);
}static void single_cell_mask_uses_cell_index_mapping(void) {
    ClearraBoard64Layout layout = standard_10x4();
    uint64_t mask = 0;
    EXPECT_STATUS(clearra_board64_cell_mask(layout, 2, 1, &mask), CLEARRA_BOARD64_OK);
    EXPECT_U64(mask, UINT64_C(1) << 12);
}static void occupied_mask_rejects_bits_outside_layout(void) {
    ClearraBoard64Layout layout = standard_10x4();
    uint64_t occupied = 0;

    EXPECT_STATUS(clearra_board64_occupied_mask(layout, 0x000f, &occupied),
                  CLEARRA_BOARD64_OK);
    EXPECT_U64(occupied, 0x000f);
    EXPECT_STATUS(clearra_board64_occupied_mask(layout, UINT64_C(1) << 63, &occupied),
                  CLEARRA_BOARD64_MASK_OUTSIDE_LAYOUT);
}static void row_mask_generation_uses_layout_width(void) {
    ClearraBoard64Layout layout = standard_10x4();
    uint64_t mask = 0;
    EXPECT_STATUS(clearra_board64_row_mask(layout, 1, &mask), CLEARRA_BOARD64_OK);
    EXPECT_U64(mask, UINT64_C(0x03ff) << 10);
}static void row_full_detection_uses_exact_row_mask(void) {
    ClearraBoard64Layout layout = standard_10x4();
    bool full = false;

    EXPECT_STATUS(clearra_board64_row_is_full(layout, UINT64_C(0x03ff), 0, &full),
                  CLEARRA_BOARD64_OK);
    EXPECT_TRUE(full);

    EXPECT_STATUS(clearra_board64_row_is_full(layout, UINT64_C(0x03fe), 0, &full),
                  CLEARRA_BOARD64_OK);
    EXPECT_FALSE(full);
}static void collision_reports_true_and_false(void) {
    ClearraBoard64Layout layout = standard_10x4();
    bool collision = false;

    EXPECT_STATUS(clearra_board64_collision(layout, 0x0003, 0x0004, &collision),
                  CLEARRA_BOARD64_OK);
    EXPECT_FALSE(collision);

    EXPECT_STATUS(clearra_board64_collision(layout, 0x0003, 0x0002, &collision),
                  CLEARRA_BOARD64_OK);
    EXPECT_TRUE(collision);
}static void place_result_or_collision_is_explicit(void) {
    ClearraBoard64Layout layout = standard_10x4();
    uint64_t placed = 0;

    EXPECT_STATUS(clearra_board64_place(layout, 0x0003, 0x000c, &placed),
                  CLEARRA_BOARD64_OK);
    EXPECT_U64(placed, 0x000f);

    EXPECT_STATUS(clearra_board64_place(layout, 0x0003, 0x0002, &placed),
                  CLEARRA_BOARD64_COLLISION);
}static void line_clear_compacts_rows_above(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraBoard64LineClearResult result;
    uint64_t full_bottom = UINT64_C(0x03ff);
    uint64_t single_cell_on_second_row = UINT64_C(1) << 10;

    EXPECT_STATUS(clearra_board64_clear_lines(layout, full_bottom | single_cell_on_second_row,
                                              &result),
                  CLEARRA_BOARD64_OK);
    EXPECT_U64(result.deleted_row_mask, 0x0001);
    EXPECT_U64(result.cleared_lines, 1);
    EXPECT_U64(result.board, 1);
}static void multi_line_clear_compacts_remaining_rows(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraBoard64LineClearResult result;
    uint64_t full_bottom = UINT64_C(0x03ff);
    uint64_t full_second = UINT64_C(0x03ff) << 10;
    uint64_t third_row_cells = UINT64_C(0x0003) << 20;

    EXPECT_STATUS(clearra_board64_clear_lines(layout, full_bottom | full_second | third_row_cells,
                                              &result),
                  CLEARRA_BOARD64_OK);
    EXPECT_U64(result.deleted_row_mask, 0x0003);
    EXPECT_U64(result.cleared_lines, 2);
    EXPECT_U64(result.board, 0x0003);
}static void line_clear_reports_non_bottom_deleted_row_mask(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraBoard64LineClearResult result;
    uint64_t full_second = UINT64_C(0x03ff) << 10;
    uint64_t third_row_cell = UINT64_C(1) << 20;

    EXPECT_STATUS(clearra_board64_clear_lines(layout, full_second | third_row_cell,
                                              &result),
                  CLEARRA_BOARD64_OK);
    EXPECT_U64(result.deleted_row_mask, 0x0002);
    EXPECT_U64(result.cleared_lines, 1);
    EXPECT_U64(result.board, UINT64_C(1) << 10);
}static void line_clear_after_placement_clears_completed_row(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraBoard64LineClearResult result;
    uint64_t placed = 0;
    uint64_t almost_full_bottom = UINT64_C(0x03ff) & ~UINT64_C(0x000c);
    uint64_t placement = UINT64_C(0x000c);

    EXPECT_STATUS(clearra_board64_place(layout, almost_full_bottom, placement, &placed),
                  CLEARRA_BOARD64_OK);
    EXPECT_U64(placed, UINT64_C(0x03ff));
    EXPECT_STATUS(clearra_board64_clear_lines(layout, placed, &result), CLEARRA_BOARD64_OK);
    EXPECT_U64(result.deleted_row_mask, 0x0001);
    EXPECT_U64(result.cleared_lines, 1);
    EXPECT_U64(result.board, 0);
}static void board_hash_is_stable_and_layout_scoped(void) {
    ClearraBoard64Layout layout = standard_10x4();
    uint64_t first = clearra_board64_hash(layout, 0x000f);
    uint64_t second = clearra_board64_hash(layout, 0x000f);
    uint64_t different_board = clearra_board64_hash(layout, 0x000e);

    EXPECT_U64(first, second);
    EXPECT_TRUE(first != different_board);
}static void board_equality_is_layout_scoped(void) {
    ClearraBoard64Layout layout = standard_10x4();

    EXPECT_TRUE(clearra_board64_equal(layout, UINT64_C(0x000f), UINT64_C(0x000f)));
    EXPECT_FALSE(clearra_board64_equal(layout, UINT64_C(0x000f), UINT64_C(0x000e)));
    EXPECT_FALSE(clearra_board64_equal(layout, UINT64_C(1) << 63, UINT64_C(1) << 63));
}int main(void) {
    empty_board_is_zero();
    cell_index_mapping_is_bottom_left_row_major();
    single_cell_mask_uses_cell_index_mapping();
    occupied_mask_rejects_bits_outside_layout();
    row_mask_generation_uses_layout_width();
    row_full_detection_uses_exact_row_mask();
    collision_reports_true_and_false();
    place_result_or_collision_is_explicit();
    line_clear_compacts_rows_above();
    multi_line_clear_compacts_remaining_rows();
    line_clear_reports_non_bottom_deleted_row_mask();
    line_clear_after_placement_clears_completed_row();
    board_hash_is_stable_and_layout_scoped();
    board_equality_is_layout_scoped();
    puts("core-c board64 tests passed");
    return 0;
}