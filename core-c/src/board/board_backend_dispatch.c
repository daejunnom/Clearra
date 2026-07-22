#include "clr_board.h"
#include "board64.h"

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
}uint32_t clr_board_backend_kind_for_cell_count(uint32_t cell_count) {
    if (cell_count == 0) {
        return 0;
    }
    if (cell_count <= 64) {
        return CLR_BOARD_BACKEND_BOARD64;
    }
    if (cell_count <= 128) {
        return CLR_BOARD_BACKEND_BOARD128;
    }
    if (cell_count <= 256) {
        return CLR_BOARD_BACKEND_BOARD256;
    }
    return CLR_BOARD_BACKEND_WIDE;
}clr_board_backend_capability clr_board_backend_capability_for_kind(uint32_t backend_kind) {
    clr_board_backend_capability capability;
    memset(&capability, 0, sizeof(capability));
    capability.backend_kind = backend_kind;

    if (backend_kind == CLR_BOARD_BACKEND_BOARD64) {
        capability.descriptor_supported = 1u;
        capability.basic_ops_supported = 1u;
        capability.operation_mask_supported = 1u;
        capability.runtime_connected = 1u;
        capability.packing_supported = 1u;
        capability.unsupported_reason = CLR_BOARD_UNSUPPORTED_REASON_NONE;
        return capability;
    }

    if (backend_kind == CLR_BOARD_BACKEND_BOARD128) {
        capability.descriptor_supported = 1u;
        capability.basic_ops_supported = 1u;
        capability.operation_mask_supported = 1u;
        capability.runtime_connected = 0u;
        capability.packing_supported = 0u;
        capability.unsupported_reason =
            CLR_BOARD_UNSUPPORTED_REASON_BOARD_BACKEND_NOT_CONNECTED;
        return capability;
    }

    if (backend_kind == CLR_BOARD_BACKEND_BOARD256) {
        capability.descriptor_supported = 1u;
        capability.basic_ops_supported = 1u;
        capability.operation_mask_supported = 1u;
        capability.runtime_connected = 0u;
        capability.packing_supported = 0u;
        capability.unsupported_reason =
            CLR_BOARD_UNSUPPORTED_REASON_BOARD_BACKEND_NOT_CONNECTED;
        return capability;
    }

    if (backend_kind == CLR_BOARD_BACKEND_WIDE) {
        capability.descriptor_supported = 1u;
        capability.basic_ops_supported = 0u;
        capability.operation_mask_supported = 0u;
        capability.runtime_connected = 0u;
        capability.packing_supported = 0u;
        capability.unsupported_reason =
            CLR_BOARD_UNSUPPORTED_REASON_WIDE_BOARD_RUNTIME_NOT_CONNECTED;
        return capability;
    }

    capability.unsupported_reason =
        CLR_BOARD_UNSUPPORTED_REASON_BOARD_WIDTH_OUT_OF_SCOPE;
    return capability;
}clr_board_backend_capability clr_board_backend_capability_for_cell_count(uint32_t cell_count) {
    return clr_board_backend_capability_for_kind(
        clr_board_backend_kind_for_cell_count(cell_count));
}clr_board_status clr_board_descriptor_init(
    uint16_t width,
    uint16_t visible_height,
    uint16_t search_height,
    uint64_t initial_mask_lo,
    uint64_t initial_mask_hi,
    clr_board_descriptor *out_descriptor) {
    if (out_descriptor == 0 || width == 0 || visible_height == 0 ||
        search_height < visible_height) {
        return CLR_BOARD_INVALID_LAYOUT;
    }
    uint32_t cell_count = (uint32_t)width * (uint32_t)search_height;
    uint32_t backend_kind = clr_board_backend_kind_for_cell_count(cell_count);
    if (backend_kind == 0) {
        return CLR_BOARD_INVALID_LAYOUT;
    }

    clr_board_descriptor descriptor;
    memset(&descriptor, 0, sizeof(descriptor));
    descriptor.width = width;
    descriptor.visible_height = visible_height;
    descriptor.search_height = search_height;
    descriptor.initial_mask = initial_mask_lo;
    descriptor.initial_mask_hi = initial_mask_hi;
    descriptor.backend_kind = backend_kind;
    descriptor.cell_count = cell_count;

    if (!clr_board_descriptor_is_valid(&descriptor)) {
        return CLR_BOARD_MASK_OUTSIDE_LAYOUT;
    }

    *out_descriptor = descriptor;
    return CLR_BOARD_OK;
}bool clr_board_descriptor_is_valid(const clr_board_descriptor *descriptor) {
    if (descriptor == 0 || descriptor->width == 0 ||
        descriptor->visible_height == 0 ||
        descriptor->search_height < descriptor->visible_height) {
        return false;
    }
    uint32_t cell_count =
        (uint32_t)descriptor->width * (uint32_t)descriptor->search_height;
    if (cell_count == 0 || descriptor->cell_count != cell_count) {
        return false;
    }
    if (descriptor->backend_kind != clr_board_backend_kind_for_cell_count(cell_count)) {
        return false;
    }
    if (descriptor->backend_kind == CLR_BOARD_BACKEND_BOARD64) {
        if (descriptor->initial_mask_hi != 0) {
            return false;
        }
        uint64_t all_cells = low_mask_for(cell_count);
        return (descriptor->initial_mask & ~all_cells) == 0;
    }
    if (descriptor->backend_kind == CLR_BOARD_BACKEND_BOARD128) {
        clr_board128_descriptor board128;
        if (clr_board128_make_descriptor(
                descriptor->width, descriptor->search_height, &board128) !=
            CLR_BOARD_OK) {
            return false;
        }
        return (descriptor->initial_mask & ~board128.all_cells_mask_lo) == 0 &&
               (descriptor->initial_mask_hi & ~board128.all_cells_mask_hi) == 0;
    }
    if (descriptor->backend_kind == CLR_BOARD_BACKEND_BOARD256) {
        clr_board256_descriptor board256;
        if (clr_board256_make_descriptor(
                descriptor->width, descriptor->search_height, &board256) !=
            CLR_BOARD_OK) {
            return false;
        }
        return (descriptor->initial_mask & ~board256.all_cells_mask[0]) == 0 &&
               (descriptor->initial_mask_hi & ~board256.all_cells_mask[1]) == 0;
    }
    return descriptor->backend_kind == CLR_BOARD_BACKEND_WIDE &&
           descriptor->initial_mask == 0 && descriptor->initial_mask_hi == 0;
}clr_board_status clr_board_dispatch_row_mask(
    const clr_board_descriptor *descriptor,
    uint16_t y,
    clr_generic_board_mask *out_mask) {
    if (!clr_board_descriptor_is_valid(descriptor) || out_mask == 0) {
        return CLR_BOARD_INVALID_LAYOUT;
    }

    if (descriptor->backend_kind == CLR_BOARD_BACKEND_BOARD64) {
        ClearraBoard64Layout layout;
        ClearraBoard64Status layout_status = clearra_board64_make_layout(
            (uint8_t)descriptor->width, (uint8_t)descriptor->search_height, &layout);
        if (layout_status != CLEARRA_BOARD64_OK) {
            return CLR_BOARD_INVALID_LAYOUT;
        }
        uint64_t row = 0;
        ClearraBoard64Status row_status =
            clearra_board64_row_mask(layout, (uint8_t)y, &row);
        if (row_status == CLEARRA_BOARD64_OUT_OF_BOUNDS) {
            return CLR_BOARD_OUT_OF_BOUNDS;
        }
        if (row_status != CLEARRA_BOARD64_OK) {
            return CLR_BOARD_INVALID_LAYOUT;
        }
        clr_generic_board_mask mask = zero_mask(CLR_BOARD_BACKEND_BOARD64);
        mask.word_count = 1;
        mask.words[0] = row;
        *out_mask = mask;
        return CLR_BOARD_OK;
    }

    if (descriptor->backend_kind == CLR_BOARD_BACKEND_BOARD128) {
        clr_board128_descriptor board128;
        clr_board_status status = clr_board128_make_descriptor(
            descriptor->width, descriptor->search_height, &board128);
        if (status != CLR_BOARD_OK) {
            return status;
        }
        return clr_board128_row_mask(board128, y, out_mask);
    }

    if (descriptor->backend_kind == CLR_BOARD_BACKEND_BOARD256) {
        clr_board256_descriptor board256;
        clr_board_status status = clr_board256_make_descriptor(
            descriptor->width, descriptor->search_height, &board256);
        if (status != CLR_BOARD_OK) {
            return status;
        }
        return clr_board256_row_mask(board256, y, out_mask);
    }

    if (descriptor->backend_kind == CLR_BOARD_BACKEND_WIDE) {
        if (y >= descriptor->search_height) {
            return CLR_BOARD_OUT_OF_BOUNDS;
        }
        clr_generic_board_mask mask = zero_mask(CLR_BOARD_BACKEND_WIDE);
        mask.word_count = 0;
        mask.wide_start = (uint32_t)y * (uint32_t)descriptor->width;
        mask.wide_len = descriptor->width;
        *out_mask = mask;
        return CLR_BOARD_OK;
    }

    return CLR_BOARD_UNSUPPORTED_BACKEND;
}clr_board_status clr_board_operation_mask_from_cells(
    const clr_board_descriptor *descriptor,
    const uint16_t *cell_indexes,
    uint16_t cell_count,
    clr_generic_board_mask *out_mask) {
    if (!clr_board_descriptor_is_valid(descriptor) || cell_indexes == 0 ||
        out_mask == 0) {
        return CLR_BOARD_INVALID_LAYOUT;
    }
    if (descriptor->backend_kind == CLR_BOARD_BACKEND_WIDE) {
        return CLR_BOARD_UNSUPPORTED_BACKEND;
    }

    clr_generic_board_mask mask = zero_mask(descriptor->backend_kind);
    mask.word_count = descriptor->backend_kind == CLR_BOARD_BACKEND_BOARD64
                          ? 1u
                          : descriptor->backend_kind == CLR_BOARD_BACKEND_BOARD128 ? 2u : 4u;
    for (uint16_t index = 0; index < cell_count; index++) {
        uint16_t cell = cell_indexes[index];
        if ((uint32_t)cell >= descriptor->cell_count) {
            return CLR_BOARD_OUT_OF_BOUNDS;
        }
        uint32_t word = (uint32_t)cell / 64u;
        uint32_t bit = (uint32_t)cell % 64u;
        mask.words[word] |= UINT64_C(1) << bit;
    }
    *out_mask = mask;
    return CLR_BOARD_OK;
}
