#include "board64.h"
uint64_t clearra_board64_empty(void) {
    return 0;
}ClearraBoard64Status clearra_board64_occupied_mask(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t *out_mask) {
    if (!clearra_board64_layout_is_valid(layout) || out_mask == 0) {
        return CLEARRA_BOARD64_INVALID_LAYOUT;
    }
    if (board & ~layout.all_cells_mask) {
        return CLEARRA_BOARD64_MASK_OUTSIDE_LAYOUT;
    }

    *out_mask = board & layout.all_cells_mask;
    return CLEARRA_BOARD64_OK;
}

#include "board64.h"
static uint64_t fnv1a_mix_u64(uint64_t hash, uint64_t value) {
    for (int byte_index = 0; byte_index < 8; byte_index++) {
        uint8_t byte = (uint8_t)((value >> (byte_index * 8)) & UINT64_C(0xff));
        hash ^= byte;
        hash *= UINT64_C(1099511628211);
    }
    return hash;
}uint64_t clearra_board64_hash(ClearraBoard64Layout layout, uint64_t board) {
    uint64_t hash = UINT64_C(1469598103934665603);
    hash = fnv1a_mix_u64(hash, layout.width);
    hash = fnv1a_mix_u64(hash, layout.height);
    hash = fnv1a_mix_u64(hash, layout.cell_count);
    hash = fnv1a_mix_u64(hash, layout.all_cells_mask);
    hash = fnv1a_mix_u64(hash, board);
    return hash;
}bool clearra_board64_equal(ClearraBoard64Layout layout, uint64_t lhs, uint64_t rhs) {
    if (!clearra_board64_layout_is_valid(layout)) {
        return false;
    }
    if ((lhs | rhs) & ~layout.all_cells_mask) {
        return false;
    }
    return (lhs & layout.all_cells_mask) == (rhs & layout.all_cells_mask);
}

#include "board64.h"
static uint64_t all_cells_mask_for(uint16_t cell_count) {
    if (cell_count == 64) {
        return UINT64_MAX;
    }
    return (UINT64_C(1) << cell_count) - UINT64_C(1);
}ClearraBoard64Status clearra_board64_make_layout(
    uint8_t width,
    uint8_t height,
    ClearraBoard64Layout *out_layout) {
    if (out_layout == 0 || width == 0 || height == 0) {
        return CLEARRA_BOARD64_INVALID_LAYOUT;
    }

    uint16_t cell_count = (uint16_t)width * (uint16_t)height;
    if (cell_count == 0 || cell_count > 64) {
        return CLEARRA_BOARD64_INVALID_LAYOUT;
    }

    out_layout->width = width;
    out_layout->height = height;
    out_layout->cell_count = cell_count;
    out_layout->all_cells_mask = all_cells_mask_for(cell_count);
    return CLEARRA_BOARD64_OK;
}bool clearra_board64_layout_is_valid(ClearraBoard64Layout layout) {
    if (layout.width == 0 || layout.height == 0) {
        return false;
    }
    uint16_t expected_cell_count = (uint16_t)layout.width * (uint16_t)layout.height;
    if (expected_cell_count == 0 || expected_cell_count > 64) {
        return false;
    }
    return layout.cell_count == expected_cell_count &&
           layout.all_cells_mask == all_cells_mask_for(expected_cell_count);
}ClearraBoard64Status clearra_board64_cell_index(
    ClearraBoard64Layout layout,
    uint8_t x,
    uint8_t y,
    uint8_t *out_index) {
    if (!clearra_board64_layout_is_valid(layout) || out_index == 0) {
        return CLEARRA_BOARD64_INVALID_LAYOUT;
    }
    if (x >= layout.width || y >= layout.height) {
        return CLEARRA_BOARD64_OUT_OF_BOUNDS;
    }

    /* Board64 uses bottom-left row-major indexing. */
    *out_index = (uint8_t)((uint16_t)y * (uint16_t)layout.width + (uint16_t)x);
    return CLEARRA_BOARD64_OK;
}ClearraBoard64Status clearra_board64_cell_mask(
    ClearraBoard64Layout layout,
    uint8_t x,
    uint8_t y,
    uint64_t *out_mask) {
    uint8_t index = 0;
    ClearraBoard64Status status = clearra_board64_cell_index(layout, x, y, &index);
    if (status != CLEARRA_BOARD64_OK) {
        return status;
    }
    if (out_mask == 0) {
        return CLEARRA_BOARD64_INVALID_LAYOUT;
    }

    *out_mask = UINT64_C(1) << index;
    return CLEARRA_BOARD64_OK;
}

#include "board64.h"

ClearraBoard64Status clearra_board64_collision(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t placement,
    bool *out_collision) {
    if (!clearra_board64_layout_is_valid(layout) || out_collision == 0) {
        return CLEARRA_BOARD64_INVALID_LAYOUT;
    }
    if ((board | placement) & ~layout.all_cells_mask) {
        return CLEARRA_BOARD64_MASK_OUTSIDE_LAYOUT;
    }

    *out_collision = (board & placement) != 0;
    return CLEARRA_BOARD64_OK;
}

#include "board64.h"

ClearraBoard64Status clearra_board64_clear_lines(
    ClearraBoard64Layout layout,
    uint64_t board,
    ClearraBoard64LineClearResult *out_result) {
    if (!clearra_board64_layout_is_valid(layout) || out_result == 0) {
        return CLEARRA_BOARD64_INVALID_LAYOUT;
    }
    if (board & ~layout.all_cells_mask) {
        return CLEARRA_BOARD64_MASK_OUTSIDE_LAYOUT;
    }

    uint64_t full_row = layout.width == 64
        ? UINT64_MAX
        : ((UINT64_C(1) << layout.width) - UINT64_C(1));
    uint64_t compacted = 0;
    uint8_t write_y = 0;
    uint8_t cleared_lines = 0;
    uint16_t deleted_row_mask = 0;

    for (uint8_t read_y = 0; read_y < layout.height; read_y++) {
        uint64_t mask = 0;
        ClearraBoard64Status status = clearra_board64_row_mask(layout, read_y, &mask);
        if (status != CLEARRA_BOARD64_OK) {
            return status;
        }

        uint16_t read_shift = (uint16_t)read_y * (uint16_t)layout.width;
        uint64_t row = (board & mask) >> read_shift;

        if (row == full_row) {
            cleared_lines++;
            if (read_y < 16u) {
                deleted_row_mask |= (uint16_t)(UINT16_C(1) << read_y);
            }
            continue;
        }

        uint16_t write_shift = (uint16_t)write_y * (uint16_t)layout.width;
        compacted |= row << write_shift;
        write_y++;
    }

    out_result->board = compacted;
    out_result->deleted_row_mask = deleted_row_mask;
    out_result->cleared_lines = cleared_lines;
    out_result->reserved = 0;
    return CLEARRA_BOARD64_OK;
}

#include "board64.h"

ClearraBoard64Status clearra_board64_place(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t placement,
    uint64_t *out_board) {
    if (out_board == 0) {
        return CLEARRA_BOARD64_INVALID_LAYOUT;
    }

    bool collision = false;
    ClearraBoard64Status status =
        clearra_board64_collision(layout, board, placement, &collision);
    if (status != CLEARRA_BOARD64_OK) {
        return status;
    }
    if (collision) {
        return CLEARRA_BOARD64_COLLISION;
    }

    *out_board = board | placement;
    return CLEARRA_BOARD64_OK;
}

#include "board64.h"
ClearraBoard64Status clearra_board64_row_mask(
    ClearraBoard64Layout layout,
    uint8_t y,
    uint64_t *out_mask) {
    if (!clearra_board64_layout_is_valid(layout) || out_mask == 0) {
        return CLEARRA_BOARD64_INVALID_LAYOUT;
    }
    if (y >= layout.height) {
        return CLEARRA_BOARD64_OUT_OF_BOUNDS;
    }

    uint64_t row_bits = layout.width == 64
        ? UINT64_MAX
        : ((UINT64_C(1) << layout.width) - UINT64_C(1));
    uint16_t shift = (uint16_t)y * (uint16_t)layout.width;
    *out_mask = row_bits << shift;
    return CLEARRA_BOARD64_OK;
}ClearraBoard64Status clearra_board64_row_is_full(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t y,
    bool *out_full) {
    if (!clearra_board64_layout_is_valid(layout) || out_full == 0) {
        return CLEARRA_BOARD64_INVALID_LAYOUT;
    }
    if (board & ~layout.all_cells_mask) {
        return CLEARRA_BOARD64_MASK_OUTSIDE_LAYOUT;
    }

    uint64_t mask = 0;
    ClearraBoard64Status status = clearra_board64_row_mask(layout, y, &mask);
    if (status != CLEARRA_BOARD64_OK) {
        return status;
    }

    *out_full = (board & mask) == mask;
    return CLEARRA_BOARD64_OK;
}
