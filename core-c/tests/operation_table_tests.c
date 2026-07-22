#include "../src/piece/operation.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_STATUS(EXPR, EXPECTED)                                                       \
    do {                                                                                    \
        ClearraOperationStatus actual_status = (EXPR);                                      \
        if (actual_status != (EXPECTED)) {                                                  \
            fprintf(stderr, "%s:%d expected status %d but got %d\n", __FILE__, __LINE__,     \
                    (int)(EXPECTED), (int)actual_status);                                   \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)

#define EXPECT_BOARD_STATUS(EXPR, EXPECTED)                                                 \
    do {                                                                                    \
        ClearraBoard64Status actual_status = (EXPR);                                        \
        if (actual_status != (EXPECTED)) {                                                  \
            fprintf(stderr, "%s:%d expected board status %d but got %d\n", __FILE__,         \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                         \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)

#define EXPECT_U64(EXPR, EXPECTED)                                                          \
    do {                                                                                    \
        uint64_t actual_value = (uint64_t)(EXPR);                                           \
        uint64_t expected_value = (uint64_t)(EXPECTED);                                     \
        if (actual_value != expected_value) {                                               \
            fprintf(stderr, "%s:%d expected 0x%llx but got 0x%llx\n", __FILE__, __LINE__,     \
                    (unsigned long long)expected_value,                                     \
                    (unsigned long long)actual_value);                                      \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)

#define EXPECT_U16(EXPR, EXPECTED)                                                          \
    do {                                                                                    \
        uint16_t actual_value = (uint16_t)(EXPR);                                           \
        uint16_t expected_value = (uint16_t)(EXPECTED);                                     \
        if (actual_value != expected_value) {                                               \
            fprintf(stderr, "%s:%d expected %u but got %u\n", __FILE__, __LINE__,            \
                    (unsigned)expected_value, (unsigned)actual_value);                      \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)

#define EXPECT_U8(EXPR, EXPECTED)                                                           \
    do {                                                                                    \
        uint8_t actual_value = (uint8_t)(EXPR);                                             \
        uint8_t expected_value = (uint8_t)(EXPECTED);                                       \
        if (actual_value != expected_value) {                                               \
            fprintf(stderr, "%s:%d expected %u but got %u\n", __FILE__, __LINE__,            \
                    (unsigned)expected_value, (unsigned)actual_value);                      \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)

#define EXPECT_TRUE(EXPR)                                                                   \
    do {                                                                                    \
        if (!(EXPR)) {                                                                      \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);                  \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)
static ClearraBoard64Layout standard_10x4(void) {
    ClearraBoard64Layout layout;
    EXPECT_BOARD_STATUS(clearra_board64_make_layout(10, 4, &layout),
                        CLEARRA_BOARD64_OK);
    return layout;
}static ClearraOperation operation_for(uint8_t piece, uint8_t rotation) {
    ClearraOperation operation;
    EXPECT_STATUS(clearra_operation_from_shape(piece, rotation, &operation),
                  CLEARRA_OPERATION_OK);
    return operation;
}static void standard_seven_tetrominoes_exist(void) {
    EXPECT_TRUE(clearra_piece_is_standard_tetromino(CLR_PIECE_I));
    EXPECT_TRUE(clearra_piece_is_standard_tetromino(CLR_PIECE_O));
    EXPECT_TRUE(clearra_piece_is_standard_tetromino(CLR_PIECE_T));
    EXPECT_TRUE(clearra_piece_is_standard_tetromino(CLR_PIECE_S));
    EXPECT_TRUE(clearra_piece_is_standard_tetromino(CLR_PIECE_Z));
    EXPECT_TRUE(clearra_piece_is_standard_tetromino(CLR_PIECE_J));
    EXPECT_TRUE(clearra_piece_is_standard_tetromino(CLR_PIECE_L));

    uint8_t piece = CLR_PIECE_NONE;
    for (uint8_t index = 0; index < CLEARRA_STANDARD_TETROMINO_COUNT; index++) {
        EXPECT_TRUE(clearra_standard_tetromino_piece_at(index, &piece));
        EXPECT_TRUE(clearra_piece_is_standard_tetromino(piece));
    }
}static void each_piece_has_four_rotation_operations(void) {
    const uint8_t pieces[] = {
        CLR_PIECE_I,
        CLR_PIECE_O,
        CLR_PIECE_T,
        CLR_PIECE_S,
        CLR_PIECE_Z,
        CLR_PIECE_J,
        CLR_PIECE_L,
    };

    for (uint8_t index = 0; index < CLEARRA_STANDARD_TETROMINO_COUNT; index++) {
        EXPECT_U8(clearra_rotation_count_for_piece(pieces[index]),
                  CLEARRA_ROTATION_STATE_COUNT);
    }
}static void piece_area_is_four(void) {
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; piece++) {
        EXPECT_U8(clearra_piece_area(piece), CLEARRA_TETROMINO_AREA);
    }
}static void operation_id_is_deterministic(void) {
    uint16_t id = 0;
    EXPECT_STATUS(clearra_operation_id(CLR_PIECE_I, CLEARRA_ROTATION_SPAWN, &id),
                  CLEARRA_OPERATION_OK);
    EXPECT_U16(id, 0);
    EXPECT_STATUS(clearra_operation_id(CLR_PIECE_O, CLEARRA_ROTATION_SPAWN, &id),
                  CLEARRA_OPERATION_OK);
    EXPECT_U16(id, 4);
    EXPECT_STATUS(clearra_operation_id(CLR_PIECE_L, CLEARRA_ROTATION_LEFT, &id),
                  CLEARRA_OPERATION_OK);
    EXPECT_U16(id, 27);
}static void bounds_are_correct(void) {
    ClearraOperation i_spawn = operation_for(CLR_PIECE_I, CLEARRA_ROTATION_SPAWN);
    EXPECT_U8(i_spawn.bounds.width, 4);
    EXPECT_U8(i_spawn.bounds.height, 1);
    EXPECT_U8(i_spawn.bounds.max_x, 3);
    EXPECT_U8(i_spawn.bounds.max_y, 0);

    ClearraOperation i_right = operation_for(CLR_PIECE_I, CLEARRA_ROTATION_RIGHT);
    EXPECT_U8(i_right.bounds.width, 1);
    EXPECT_U8(i_right.bounds.height, 4);
    EXPECT_U8(i_right.bounds.max_x, 0);
    EXPECT_U8(i_right.bounds.max_y, 3);

    ClearraOperation o_spawn = operation_for(CLR_PIECE_O, CLEARRA_ROTATION_SPAWN);
    EXPECT_U8(o_spawn.bounds.width, 2);
    EXPECT_U8(o_spawn.bounds.height, 2);

    ClearraOperation t_spawn = operation_for(CLR_PIECE_T, CLEARRA_ROTATION_SPAWN);
    EXPECT_U8(t_spawn.bounds.width, 3);
    EXPECT_U8(t_spawn.bounds.height, 2);
}static void operation_mask_stays_stable(void) {
    ClearraBoard64Layout layout = standard_10x4();
    uint64_t mask = 0;

    ClearraOperation i_spawn = operation_for(CLR_PIECE_I, CLEARRA_ROTATION_SPAWN);
    EXPECT_STATUS(clearra_operation_mask(layout, &i_spawn, 0, 0, &mask),
                  CLEARRA_OPERATION_OK);
    EXPECT_U64(mask, UINT64_C(0x000f));

    ClearraOperation i_right = operation_for(CLR_PIECE_I, CLEARRA_ROTATION_RIGHT);
    EXPECT_STATUS(clearra_operation_mask(layout, &i_right, 0, 0, &mask),
                  CLEARRA_OPERATION_OK);
    EXPECT_U64(mask, UINT64_C(0x40100401));

    ClearraOperation o_spawn = operation_for(CLR_PIECE_O, CLEARRA_ROTATION_SPAWN);
    EXPECT_STATUS(clearra_operation_mask(layout, &o_spawn, 0, 0, &mask),
                  CLEARRA_OPERATION_OK);
    EXPECT_U64(mask, UINT64_C(0x0c03));

    ClearraOperation t_spawn = operation_for(CLR_PIECE_T, CLEARRA_ROTATION_SPAWN);
    EXPECT_STATUS(clearra_operation_mask(layout, &t_spawn, 0, 0, &mask),
                  CLEARRA_OPERATION_OK);
    EXPECT_U64(mask, UINT64_C(0x0807));
    EXPECT_STATUS(clearra_operation_mask(layout, &t_spawn, 8, 0, &mask),
                  CLEARRA_OPERATION_OUT_OF_BOUNDS);
}static void operation_table_generates_standard_28_operations(void) {
    ClearraOperationTable table;
    EXPECT_STATUS(clearra_operation_table_generate(&table), CLEARRA_OPERATION_OK);
    EXPECT_U16(table.count, CLEARRA_STANDARD_OPERATION_COUNT);
    for (uint16_t index = 0; index < table.count; index++) {
        EXPECT_U16(table.operations[index].operation_id, index);
        EXPECT_U8(table.operations[index].area, CLEARRA_TETROMINO_AREA);
    }
}static void standard_operation_table_unchanged(void) {
    ClearraOperationTable table;
    EXPECT_STATUS(clearra_operation_table_generate(&table), CLEARRA_OPERATION_OK);
    EXPECT_U16(table.count, CLEARRA_STANDARD_OPERATION_COUNT);
}static void operation_set_counts_piece_rotations(void) {
    ClearraOperationTable table;
    ClearraOperationSet set;
    EXPECT_STATUS(clearra_operation_table_generate(&table), CLEARRA_OPERATION_OK);
    EXPECT_STATUS(clearra_operation_set_from_table_for_piece(&table, CLR_PIECE_T, &set),
                  CLEARRA_OPERATION_OK);
    EXPECT_U16(clearra_operation_set_count_piece(&set, CLR_PIECE_T),
               CLEARRA_ROTATION_STATE_COUNT);
    EXPECT_U16(clearra_operation_set_count_piece(&set, CLR_PIECE_I), 0);
}int main(void) {
    standard_seven_tetrominoes_exist();
    each_piece_has_four_rotation_operations();
    piece_area_is_four();
    operation_id_is_deterministic();
    bounds_are_correct();
    operation_mask_stays_stable();
    operation_table_generates_standard_28_operations();
    standard_operation_table_unchanged();
    operation_set_counts_piece_rotations();
    return 0;
}
