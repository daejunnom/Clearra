#include "clr_board.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

#define EXPECT_STATUS(EXPR, EXPECTED)                                                   \
    do {                                                                                \
        clr_board_status actual_status = (EXPR);                                        \
        if (actual_status != (EXPECTED)) {                                              \
            fprintf(stderr, "%s:%d expected board status %d but got %d\n", __FILE__,   \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                     \
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

#define EXPECT_U64(EXPR, EXPECTED)                                                      \
    do {                                                                                \
        uint64_t actual_value = (uint64_t)(EXPR);                                       \
        uint64_t expected_value = (uint64_t)(EXPECTED);                                 \
        if (actual_value != expected_value) {                                           \
            fprintf(stderr, "%s:%d expected 0x%llx but got 0x%llx\n", __FILE__,        \
                    __LINE__, (unsigned long long)expected_value,                       \
                    (unsigned long long)actual_value);                                  \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)
static void board64_fast_path_row_mask_is_unchanged(void) {
    clr_board_descriptor descriptor;
    clr_generic_board_mask row;
    clr_board_backend_capability capability =
        clr_board_backend_capability_for_cell_count(60);

    EXPECT_STATUS(clr_board_descriptor_init(10, 6, 6, 0, 0, &descriptor),
                  CLR_BOARD_OK);
    EXPECT_U64(descriptor.backend_kind, CLR_BOARD_BACKEND_BOARD64);
    EXPECT_U64(descriptor.cell_count, 60);
    EXPECT_U64(capability.backend_kind, CLR_BOARD_BACKEND_BOARD64);
    EXPECT_U64(capability.runtime_connected, 1);
    EXPECT_U64(capability.packing_supported, 1);
    EXPECT_U64(capability.unsupported_reason, CLR_BOARD_UNSUPPORTED_REASON_NONE);
    EXPECT_STATUS(clr_board_dispatch_row_mask(&descriptor, 0, &row), CLR_BOARD_OK);
    EXPECT_U64(row.backend_kind, CLR_BOARD_BACKEND_BOARD64);
    EXPECT_U64(row.word_count, 1);
    EXPECT_U64(row.words[0], UINT64_C(0x03ff));
}static void board128_descriptor_validates(void) {
    clr_board128_descriptor descriptor;

    EXPECT_STATUS(clr_board128_make_descriptor(10, 12, &descriptor), CLR_BOARD_OK);
    EXPECT_TRUE(clr_board128_descriptor_is_valid(&descriptor));
    EXPECT_U64(descriptor.cell_count, 120);
    EXPECT_STATUS(clr_board128_make_descriptor(8, 8, &descriptor),
                  CLR_BOARD_INVALID_LAYOUT);
}static void board128_basic_row_mask_collision_place_tests_pass(void) {
    clr_board128_descriptor descriptor;
    clr_generic_board_mask high_row;
    clr_generic_board_mask board = {0};
    clr_generic_board_mask placement = {0};
    clr_generic_board_mask placed = {0};
    bool collision = false;
    clr_board_backend_capability capability =
        clr_board_backend_capability_for_cell_count(120);

    EXPECT_STATUS(clr_board128_make_descriptor(10, 12, &descriptor), CLR_BOARD_OK);
    EXPECT_TRUE(clr_board128_descriptor_is_valid(&descriptor));
    EXPECT_U64(capability.backend_kind, CLR_BOARD_BACKEND_BOARD128);
    EXPECT_U64(capability.descriptor_supported, 1);
    EXPECT_U64(capability.basic_ops_supported, 1);
    EXPECT_U64(capability.operation_mask_supported, 1);
    EXPECT_U64(capability.packing_supported, 0);
    EXPECT_U64(capability.unsupported_reason,
               CLR_BOARD_UNSUPPORTED_REASON_BOARD_BACKEND_NOT_CONNECTED);
    EXPECT_STATUS(clr_board128_row_mask(descriptor, 11, &high_row), CLR_BOARD_OK);
    EXPECT_U64(high_row.backend_kind, CLR_BOARD_BACKEND_BOARD128);
    EXPECT_U64(high_row.word_count, 2);
    EXPECT_U64(high_row.words[0], 0);
    EXPECT_U64(high_row.words[1], UINT64_C(0x03ff) << 46);

    board.backend_kind = CLR_BOARD_BACKEND_BOARD128;
    board.word_count = 2;
    placement.backend_kind = CLR_BOARD_BACKEND_BOARD128;
    placement.word_count = 2;
    placement.words[1] = UINT64_C(1) << 6;

    EXPECT_STATUS(clr_board128_collision(descriptor, board, placement, &collision),
                  CLR_BOARD_OK);
    EXPECT_FALSE(collision);
    EXPECT_STATUS(clr_board128_place(descriptor, board, placement, &placed), CLR_BOARD_OK);
    EXPECT_U64(placed.words[1], UINT64_C(1) << 6);

    EXPECT_STATUS(clr_board128_collision(descriptor, placed, placement, &collision),
                  CLR_BOARD_OK);
    EXPECT_TRUE(collision);
}static void wide_board_descriptor_validates(void) {
    clr_board_descriptor descriptor;
    clr_wide_board_descriptor wide_descriptor;
    clr_generic_board_mask row;
    clr_board_backend_capability capability =
        clr_board_backend_capability_for_cell_count(320);

    EXPECT_STATUS(clr_wide_board_make_descriptor(16, 20, &wide_descriptor),
                  CLR_BOARD_OK);
    EXPECT_TRUE(clr_wide_board_descriptor_is_valid(&wide_descriptor));
    EXPECT_U64(wide_descriptor.cell_count, 320);
    EXPECT_U64(capability.backend_kind, CLR_BOARD_BACKEND_WIDE);
    EXPECT_U64(capability.descriptor_supported, 1);
    EXPECT_U64(capability.operation_mask_supported, 0);
    EXPECT_U64(capability.packing_supported, 0);
    EXPECT_U64(capability.unsupported_reason,
               CLR_BOARD_UNSUPPORTED_REASON_WIDE_BOARD_RUNTIME_NOT_CONNECTED);
    EXPECT_STATUS(clr_board_descriptor_init(16, 20, 20, 0, 0, &descriptor),
                  CLR_BOARD_OK);
    EXPECT_U64(descriptor.backend_kind, CLR_BOARD_BACKEND_WIDE);
    EXPECT_U64(descriptor.cell_count, 320);
    EXPECT_TRUE(clr_board_descriptor_is_valid(&descriptor));
    EXPECT_STATUS(clr_board_dispatch_row_mask(&descriptor, 3, &row), CLR_BOARD_OK);
    EXPECT_U64(row.backend_kind, CLR_BOARD_BACKEND_WIDE);
    EXPECT_U64(row.word_count, 0);
    EXPECT_U64(row.wide_start, 48);
    EXPECT_U64(row.wide_len, 16);
}static void wide_board_runtime_not_connected_reports_reason(void) {
    clr_board_backend_capability capability =
        clr_board_backend_capability_for_kind(CLR_BOARD_BACKEND_WIDE);

    EXPECT_U64(capability.runtime_connected, 0);
    EXPECT_U64(capability.unsupported_reason,
               CLR_BOARD_UNSUPPORTED_REASON_WIDE_BOARD_RUNTIME_NOT_CONNECTED);
}static void unsupported_board_width_silent_fallback_forbidden(void) {
    clr_board_descriptor descriptor;
    clr_generic_board_mask operation_mask;
    uint16_t cells[4] = {0, 1, 16, 17};

    EXPECT_STATUS(clr_board_descriptor_init(16, 20, 20, 0, 0, &descriptor),
                  CLR_BOARD_OK);
    EXPECT_STATUS(clr_board_operation_mask_from_cells(
                      &descriptor, cells, 4, &operation_mask),
                  CLR_BOARD_UNSUPPORTED_BACKEND);
}int main(void) {
    board64_fast_path_row_mask_is_unchanged();
    board128_descriptor_validates();
    board128_basic_row_mask_collision_place_tests_pass();
    wide_board_descriptor_validates();
    wide_board_runtime_not_connected_reports_reason();
    unsupported_board_width_silent_fallback_forbidden();
    puts("core-c board backend dispatch tests passed");
    return 0;
}