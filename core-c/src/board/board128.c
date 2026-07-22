#include "clr_board.h"

#include <string.h>
static uint64_t low_mask_for(uint32_t bit_count) {
    if (bit_count == 0) {
        return UINT64_C(0);
    }
    if (bit_count >= 64) {
        return UINT64_MAX;
    }
    return (UINT64_C(1) << bit_count) - UINT64_C(1);
}static clr_generic_board_mask zero_mask(uint32_t backend_kind) {
    clr_generic_board_mask mask;
    memset(&mask, 0, sizeof(mask));
    mask.backend_kind = backend_kind;
    return mask;
}static bool mask128_outside_descriptor(
    const clr_board128_descriptor *descriptor,
    clr_generic_board_mask mask) {
    if (mask.backend_kind != CLR_BOARD_BACKEND_BOARD128 || mask.word_count != 2) {
        return true;
    }
    if ((mask.words[0] & ~descriptor->all_cells_mask_lo) != 0) {
        return true;
    }
    return (mask.words[1] & ~descriptor->all_cells_mask_hi) != 0;
}clr_board_status clr_board128_make_descriptor(
    uint16_t width,
    uint16_t height,
    clr_board128_descriptor *out_descriptor) {
    if (out_descriptor == 0 || width == 0 || height == 0) {
        return CLR_BOARD_INVALID_LAYOUT;
    }
    uint32_t cell_count = (uint32_t)width * (uint32_t)height;
    if (cell_count <= 64 || cell_count > 128) {
        return CLR_BOARD_INVALID_LAYOUT;
    }

    clr_board128_descriptor descriptor;
    memset(&descriptor, 0, sizeof(descriptor));
    descriptor.width = width;
    descriptor.height = height;
    descriptor.cell_count = (uint16_t)cell_count;
    descriptor.all_cells_mask_lo = low_mask_for(cell_count > 64 ? 64 : cell_count);
    descriptor.all_cells_mask_hi =
        cell_count <= 64 ? UINT64_C(0) : low_mask_for(cell_count - 64);
    *out_descriptor = descriptor;
    return CLR_BOARD_OK;
}bool clr_board128_descriptor_is_valid(const clr_board128_descriptor *descriptor) {
    if (descriptor == 0 || descriptor->width == 0 || descriptor->height == 0) {
        return false;
    }
    uint32_t cell_count = (uint32_t)descriptor->width * (uint32_t)descriptor->height;
    if (cell_count <= 64 || cell_count > 128 ||
        descriptor->cell_count != cell_count) {
        return false;
    }

    clr_board128_descriptor expected;
    if (clr_board128_make_descriptor(
            descriptor->width, descriptor->height, &expected) != CLR_BOARD_OK) {
        return false;
    }
    return descriptor->all_cells_mask_lo == expected.all_cells_mask_lo &&
           descriptor->all_cells_mask_hi == expected.all_cells_mask_hi;
}clr_board_status clr_board128_row_mask(
    clr_board128_descriptor descriptor,
    uint16_t y,
    clr_generic_board_mask *out_mask) {
    if (!clr_board128_descriptor_is_valid(&descriptor) || out_mask == 0) {
        return CLR_BOARD_INVALID_LAYOUT;
    }
    if (y >= descriptor.height) {
        return CLR_BOARD_OUT_OF_BOUNDS;
    }

    clr_generic_board_mask mask = zero_mask(CLR_BOARD_BACKEND_BOARD128);
    mask.word_count = 2;
    uint32_t start = (uint32_t)y * (uint32_t)descriptor.width;
    for (uint16_t x = 0; x < descriptor.width; x++) {
        uint32_t bit = start + x;
        if (bit < 64) {
            mask.words[0] |= UINT64_C(1) << bit;
        } else {
            mask.words[1] |= UINT64_C(1) << (bit - 64);
        }
    }
    *out_mask = mask;
    return CLR_BOARD_OK;
}clr_board_status clr_board128_collision(
    clr_board128_descriptor descriptor,
    clr_generic_board_mask board,
    clr_generic_board_mask placement,
    bool *out_collision) {
    if (!clr_board128_descriptor_is_valid(&descriptor) || out_collision == 0) {
        return CLR_BOARD_INVALID_LAYOUT;
    }
    if (mask128_outside_descriptor(&descriptor, board) ||
        mask128_outside_descriptor(&descriptor, placement)) {
        return CLR_BOARD_MASK_OUTSIDE_LAYOUT;
    }

    *out_collision = ((board.words[0] & placement.words[0]) != 0) ||
                     ((board.words[1] & placement.words[1]) != 0);
    return CLR_BOARD_OK;
}clr_board_status clr_board128_place(
    clr_board128_descriptor descriptor,
    clr_generic_board_mask board,
    clr_generic_board_mask placement,
    clr_generic_board_mask *out_board) {
    if (out_board == 0) {
        return CLR_BOARD_INVALID_LAYOUT;
    }

    bool collision = false;
    clr_board_status status =
        clr_board128_collision(descriptor, board, placement, &collision);
    if (status != CLR_BOARD_OK) {
        return status;
    }
    if (collision) {
        return CLR_BOARD_COLLISION;
    }

    clr_generic_board_mask result = zero_mask(CLR_BOARD_BACKEND_BOARD128);
    result.word_count = 2;
    result.words[0] = board.words[0] | placement.words[0];
    result.words[1] = board.words[1] | placement.words[1];
    *out_board = result;
    return CLR_BOARD_OK;
}