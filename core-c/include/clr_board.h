#ifndef CLR_BOARD_H
#define CLR_BOARD_H

#include <stdbool.h>
#include <stdint.h>

#define CLR_BOARD_BACKEND_BOARD64 1u
#define CLR_BOARD_BACKEND_BOARD128 2u
#define CLR_BOARD_BACKEND_WIDE 3u
#define CLR_BOARD_BACKEND_BOARD256 4u

#define CLR_STANDARD_PC_BOARD_WIDTH 10u
#define CLR_STANDARD_PC_COMPACT_MAX_LINES 6u
#define CLR_STANDARD_PC_EXTENDED_MIN_LINES 7u
#define CLR_STANDARD_PC_MAX_LINES 24u
#define CLR_STANDARD_PC_BOARD_WORD_CAPACITY 4u

#define CLR_BOARD_UNSUPPORTED_REASON_NONE 0u
#define CLR_BOARD_UNSUPPORTED_REASON_BOARD_WIDTH_OUT_OF_SCOPE 1u
#define CLR_BOARD_UNSUPPORTED_REASON_BOARD_BACKEND_NOT_CONNECTED 2u
#define CLR_BOARD_UNSUPPORTED_REASON_WIDE_BOARD_RUNTIME_NOT_CONNECTED 3u
typedef enum clr_board_status {
    CLR_BOARD_OK = 0,
    CLR_BOARD_INVALID_LAYOUT = 1,
    CLR_BOARD_OUT_OF_BOUNDS = 2,
    CLR_BOARD_MASK_OUTSIDE_LAYOUT = 3,
    CLR_BOARD_COLLISION = 4,
    CLR_BOARD_UNSUPPORTED_BACKEND = 5
} clr_board_status;

typedef struct clr_board_descriptor {
    uint16_t width;
    uint16_t visible_height;
    uint16_t search_height;
    uint16_t reserved;
    uint64_t initial_mask;
    uint64_t initial_mask_hi;
    uint32_t backend_kind;
    uint32_t cell_count;
} clr_board_descriptor;

typedef struct clr_board_backend_capability {
    uint32_t backend_kind;
    uint8_t descriptor_supported;
    uint8_t basic_ops_supported;
    uint8_t operation_mask_supported;
    uint8_t runtime_connected;
    uint8_t packing_supported;
    uint8_t reserved[3];
    uint32_t unsupported_reason;
} clr_board_backend_capability;

typedef struct clr_board128_descriptor {
    uint16_t width;
    uint16_t height;
    uint16_t cell_count;
    uint16_t reserved;
    uint64_t all_cells_mask_lo;
    uint64_t all_cells_mask_hi;
} clr_board128_descriptor;

typedef struct clr_wide_board_descriptor {
    uint16_t width;
    uint16_t height;
    uint32_t cell_count;
} clr_wide_board_descriptor;

typedef struct clr_board256_descriptor {
    uint16_t width;
    uint16_t height;
    uint16_t cell_count;
    uint16_t word_count;
    uint64_t all_cells_mask[4];
} clr_board256_descriptor;

typedef struct clr_standard_pc_extended_board_descriptor {
    uint16_t width;
    uint16_t target_lines;
    uint16_t cell_count;
    uint16_t word_count;
    uint32_t backend_kind;
    uint32_t reserved;
    uint64_t initial_words[CLR_STANDARD_PC_BOARD_WORD_CAPACITY];
} clr_standard_pc_extended_board_descriptor;

typedef struct clr_generic_board_mask {
    uint32_t backend_kind;
    uint32_t word_count;
    uint64_t words[4];
    uint32_t wide_start;
    uint32_t wide_len;
} clr_generic_board_mask;

uint32_t clr_board_backend_kind_for_cell_count(uint32_t cell_count);
clr_board_backend_capability clr_board_backend_capability_for_kind(uint32_t backend_kind);
clr_board_backend_capability clr_board_backend_capability_for_cell_count(uint32_t cell_count);
clr_board_status clr_board_descriptor_init(
    uint16_t width,
    uint16_t visible_height,
    uint16_t search_height,
    uint64_t initial_mask_lo,
    uint64_t initial_mask_hi,
    clr_board_descriptor *out_descriptor);
bool clr_board_descriptor_is_valid(const clr_board_descriptor *descriptor);
clr_board_status clr_board128_make_descriptor(
    uint16_t width,
    uint16_t height,
    clr_board128_descriptor *out_descriptor);
bool clr_board128_descriptor_is_valid(const clr_board128_descriptor *descriptor);
clr_board_status clr_board256_make_descriptor(
    uint16_t width,
    uint16_t height,
    clr_board256_descriptor *out_descriptor);
bool clr_board256_descriptor_is_valid(const clr_board256_descriptor *descriptor);
clr_board_status clr_standard_pc_extended_board_descriptor_init(
    uint16_t target_lines,
    const uint64_t initial_words[CLR_STANDARD_PC_BOARD_WORD_CAPACITY],
    clr_standard_pc_extended_board_descriptor *out_descriptor);
bool clr_standard_pc_extended_board_descriptor_is_valid(
    const clr_standard_pc_extended_board_descriptor *descriptor);
clr_board_status clr_wide_board_make_descriptor(
    uint16_t width,
    uint16_t height,
    clr_wide_board_descriptor *out_descriptor);
bool clr_wide_board_descriptor_is_valid(const clr_wide_board_descriptor *descriptor);
clr_board_status clr_board128_row_mask(
    clr_board128_descriptor descriptor,
    uint16_t y,
    clr_generic_board_mask *out_mask);
clr_board_status clr_board128_collision(
    clr_board128_descriptor descriptor,
    clr_generic_board_mask board,
    clr_generic_board_mask placement,
    bool *out_collision);
clr_board_status clr_board128_place(
    clr_board128_descriptor descriptor,
    clr_generic_board_mask board,
    clr_generic_board_mask placement,
    clr_generic_board_mask *out_board);
clr_board_status clr_board256_row_mask(
    clr_board256_descriptor descriptor,
    uint16_t y,
    clr_generic_board_mask *out_mask);
clr_board_status clr_board256_collision(
    clr_board256_descriptor descriptor,
    clr_generic_board_mask board,
    clr_generic_board_mask placement,
    bool *out_collision);
clr_board_status clr_board256_place(
    clr_board256_descriptor descriptor,
    clr_generic_board_mask board,
    clr_generic_board_mask placement,
    clr_generic_board_mask *out_board);
clr_board_status clr_board_dispatch_row_mask(
    const clr_board_descriptor *descriptor,
    uint16_t y,
    clr_generic_board_mask *out_mask);
clr_board_status clr_board_operation_mask_from_cells(
    const clr_board_descriptor *descriptor,
    const uint16_t *cell_indexes,
    uint16_t cell_count,
    clr_generic_board_mask *out_mask);
#endif
