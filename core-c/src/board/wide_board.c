#include "clr_board.h"

#include <string.h>
clr_board_status clr_wide_board_make_descriptor(
    uint16_t width,
    uint16_t height,
    clr_wide_board_descriptor *out_descriptor) {
    if (out_descriptor == 0 || width == 0 || height == 0) {
        return CLR_BOARD_INVALID_LAYOUT;
    }
    uint32_t cell_count = (uint32_t)width * (uint32_t)height;
    if (cell_count <= 128) {
        return CLR_BOARD_INVALID_LAYOUT;
    }

    clr_wide_board_descriptor descriptor;
    memset(&descriptor, 0, sizeof(descriptor));
    descriptor.width = width;
    descriptor.height = height;
    descriptor.cell_count = cell_count;
    *out_descriptor = descriptor;
    return CLR_BOARD_OK;
}bool clr_wide_board_descriptor_is_valid(const clr_wide_board_descriptor *descriptor) {
    if (descriptor == 0 || descriptor->width == 0 || descriptor->height == 0) {
        return false;
    }
    uint32_t cell_count =
        (uint32_t)descriptor->width * (uint32_t)descriptor->height;
    return cell_count > 128 && descriptor->cell_count == cell_count;
}