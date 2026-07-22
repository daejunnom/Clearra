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

static bool words_fit_cell_count(
    const uint64_t words[CLR_STANDARD_PC_BOARD_WORD_CAPACITY],
    uint32_t cell_count) {
    for (uint32_t word = 0u; word < CLR_STANDARD_PC_BOARD_WORD_CAPACITY; word++) {
        uint32_t consumed = word * 64u;
        uint32_t remaining = cell_count > consumed ? cell_count - consumed : 0u;
        if ((words[word] & ~low_mask_for(remaining)) != 0u) {
            return false;
        }
    }
    return true;
}

clr_board_status clr_standard_pc_extended_board_descriptor_init(
    uint16_t target_lines,
    const uint64_t initial_words[CLR_STANDARD_PC_BOARD_WORD_CAPACITY],
    clr_standard_pc_extended_board_descriptor *out_descriptor) {
    if (initial_words == 0 || out_descriptor == 0 ||
        target_lines < CLR_STANDARD_PC_EXTENDED_MIN_LINES ||
        target_lines > CLR_STANDARD_PC_MAX_LINES) {
        return CLR_BOARD_INVALID_LAYOUT;
    }

    uint32_t cell_count = CLR_STANDARD_PC_BOARD_WIDTH * (uint32_t)target_lines;
    if (!words_fit_cell_count(initial_words, cell_count)) {
        return CLR_BOARD_MASK_OUTSIDE_LAYOUT;
    }

    clr_standard_pc_extended_board_descriptor descriptor;
    memset(&descriptor, 0, sizeof(descriptor));
    descriptor.width = CLR_STANDARD_PC_BOARD_WIDTH;
    descriptor.target_lines = target_lines;
    descriptor.cell_count = (uint16_t)cell_count;
    descriptor.word_count = cell_count <= 128u ? 2u : 4u;
    descriptor.backend_kind = clr_board_backend_kind_for_cell_count(cell_count);
    memcpy(descriptor.initial_words, initial_words, sizeof(descriptor.initial_words));
    *out_descriptor = descriptor;
    return CLR_BOARD_OK;
}

bool clr_standard_pc_extended_board_descriptor_is_valid(
    const clr_standard_pc_extended_board_descriptor *descriptor) {
    if (descriptor == 0) {
        return false;
    }

    clr_standard_pc_extended_board_descriptor expected;
    if (clr_standard_pc_extended_board_descriptor_init(
            descriptor->target_lines, descriptor->initial_words, &expected) != CLR_BOARD_OK) {
        return false;
    }
    return memcmp(&expected, descriptor, sizeof(expected)) == 0;
}
