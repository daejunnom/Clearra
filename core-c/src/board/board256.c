#include "clr_board.h"

#include <string.h>

static uint64_t low_mask_for(uint32_t bit_count) {
    if (bit_count == 0u) {
        return UINT64_C(0);
    }
    if (bit_count >= 64u) {
        return UINT64_MAX;
    }
    return (UINT64_C(1) << bit_count) - UINT64_C(1);
}

static clr_generic_board_mask zero_mask(void) {
    clr_generic_board_mask mask;
    memset(&mask, 0, sizeof(mask));
    mask.backend_kind = CLR_BOARD_BACKEND_BOARD256;
    mask.word_count = 4u;
    return mask;
}

static bool mask_outside_descriptor(
    const clr_board256_descriptor *descriptor,
    clr_generic_board_mask mask) {
    if (mask.backend_kind != CLR_BOARD_BACKEND_BOARD256 || mask.word_count != 4u) {
        return true;
    }
    for (uint32_t word = 0u; word < 4u; word++) {
        if ((mask.words[word] & ~descriptor->all_cells_mask[word]) != 0u) {
            return true;
        }
    }
    return false;
}

clr_board_status clr_board256_make_descriptor(
    uint16_t width,
    uint16_t height,
    clr_board256_descriptor *out_descriptor) {
    if (out_descriptor == 0 || width == 0u || height == 0u) {
        return CLR_BOARD_INVALID_LAYOUT;
    }
    uint32_t cell_count = (uint32_t)width * (uint32_t)height;
    if (cell_count <= 128u || cell_count > 256u) {
        return CLR_BOARD_INVALID_LAYOUT;
    }

    clr_board256_descriptor descriptor;
    memset(&descriptor, 0, sizeof(descriptor));
    descriptor.width = width;
    descriptor.height = height;
    descriptor.cell_count = (uint16_t)cell_count;
    descriptor.word_count = 4u;
    for (uint32_t word = 0u; word < 4u; word++) {
        uint32_t consumed = word * 64u;
        uint32_t remaining = cell_count > consumed ? cell_count - consumed : 0u;
        descriptor.all_cells_mask[word] = low_mask_for(remaining);
    }
    *out_descriptor = descriptor;
    return CLR_BOARD_OK;
}

bool clr_board256_descriptor_is_valid(const clr_board256_descriptor *descriptor) {
    if (descriptor == 0 || descriptor->word_count != 4u) {
        return false;
    }
    clr_board256_descriptor expected;
    if (clr_board256_make_descriptor(descriptor->width, descriptor->height, &expected) !=
        CLR_BOARD_OK) {
        return false;
    }
    return memcmp(&expected, descriptor, sizeof(expected)) == 0;
}

clr_board_status clr_board256_row_mask(
    clr_board256_descriptor descriptor,
    uint16_t y,
    clr_generic_board_mask *out_mask) {
    if (!clr_board256_descriptor_is_valid(&descriptor) || out_mask == 0) {
        return CLR_BOARD_INVALID_LAYOUT;
    }
    if (y >= descriptor.height) {
        return CLR_BOARD_OUT_OF_BOUNDS;
    }
    clr_generic_board_mask mask = zero_mask();
    uint32_t start = (uint32_t)y * (uint32_t)descriptor.width;
    for (uint16_t x = 0u; x < descriptor.width; x++) {
        uint32_t bit = start + (uint32_t)x;
        mask.words[bit / 64u] |= UINT64_C(1) << (bit % 64u);
    }
    *out_mask = mask;
    return CLR_BOARD_OK;
}

clr_board_status clr_board256_collision(
    clr_board256_descriptor descriptor,
    clr_generic_board_mask board,
    clr_generic_board_mask placement,
    bool *out_collision) {
    if (!clr_board256_descriptor_is_valid(&descriptor) || out_collision == 0) {
        return CLR_BOARD_INVALID_LAYOUT;
    }
    if (mask_outside_descriptor(&descriptor, board) ||
        mask_outside_descriptor(&descriptor, placement)) {
        return CLR_BOARD_MASK_OUTSIDE_LAYOUT;
    }
    *out_collision = false;
    for (uint32_t word = 0u; word < 4u; word++) {
        if ((board.words[word] & placement.words[word]) != 0u) {
            *out_collision = true;
            break;
        }
    }
    return CLR_BOARD_OK;
}

clr_board_status clr_board256_place(
    clr_board256_descriptor descriptor,
    clr_generic_board_mask board,
    clr_generic_board_mask placement,
    clr_generic_board_mask *out_board) {
    if (out_board == 0) {
        return CLR_BOARD_INVALID_LAYOUT;
    }
    bool collision = false;
    clr_board_status status =
        clr_board256_collision(descriptor, board, placement, &collision);
    if (status != CLR_BOARD_OK) {
        return status;
    }
    if (collision) {
        return CLR_BOARD_COLLISION;
    }
    clr_generic_board_mask result = zero_mask();
    for (uint32_t word = 0u; word < 4u; word++) {
        result.words[word] = board.words[word] | placement.words[word];
    }
    *out_board = result;
    return CLR_BOARD_OK;
}
