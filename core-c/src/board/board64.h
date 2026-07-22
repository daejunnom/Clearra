#ifndef CLEARRA_CORE_C_BOARD64_H
#define CLEARRA_CORE_C_BOARD64_H

#include <stdbool.h>
#include <stdint.h>
typedef enum ClearraBoard64Status {
    CLEARRA_BOARD64_OK = 0,
    CLEARRA_BOARD64_INVALID_LAYOUT = 1,
    CLEARRA_BOARD64_OUT_OF_BOUNDS = 2,
    CLEARRA_BOARD64_MASK_OUTSIDE_LAYOUT = 3,
    CLEARRA_BOARD64_COLLISION = 4
} ClearraBoard64Status;typedef struct ClearraBoard64Layout {
    uint8_t width;
    uint8_t height;
    uint16_t cell_count;
    uint64_t all_cells_mask;
} ClearraBoard64Layout;typedef struct ClearraBoard64LineClearResult {
    uint64_t board;
    uint16_t deleted_row_mask;
    uint8_t cleared_lines;
    uint8_t reserved;
} ClearraBoard64LineClearResult;ClearraBoard64Status clearra_board64_make_layout(
    uint8_t width,
    uint8_t height,
    ClearraBoard64Layout *out_layout);
bool clearra_board64_layout_is_valid(ClearraBoard64Layout layout);
uint64_t clearra_board64_empty(void);
ClearraBoard64Status clearra_board64_occupied_mask(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t *out_mask);
ClearraBoard64Status clearra_board64_cell_index(
    ClearraBoard64Layout layout,
    uint8_t x,
    uint8_t y,
    uint8_t *out_index);
ClearraBoard64Status clearra_board64_cell_mask(
    ClearraBoard64Layout layout,
    uint8_t x,
    uint8_t y,
    uint64_t *out_mask);
ClearraBoard64Status clearra_board64_row_mask(
    ClearraBoard64Layout layout,
    uint8_t y,
    uint64_t *out_mask);
ClearraBoard64Status clearra_board64_row_is_full(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t y,
    bool *out_full);
ClearraBoard64Status clearra_board64_collision(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t placement,
    bool *out_collision);
ClearraBoard64Status clearra_board64_place(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t placement,
    uint64_t *out_board);
ClearraBoard64Status clearra_board64_clear_lines(
    ClearraBoard64Layout layout,
    uint64_t board,
    ClearraBoard64LineClearResult *out_result);
uint64_t clearra_board64_hash(ClearraBoard64Layout layout, uint64_t board);
bool clearra_board64_equal(ClearraBoard64Layout layout, uint64_t lhs, uint64_t rhs);
#endif
