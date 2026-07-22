#ifndef CLR_BUILDUP_OPERATION_H
#define CLR_BUILDUP_OPERATION_H

#include "clr_buildup_status.h"
#include "clr_supply.h"

#include <stdbool.h>
#include <stdint.h>
#define CLR_BUILDUP_MAX_OPERATION_VARIANTS 4u
typedef struct clr_buildup_operation {
    uint8_t piece;
    uint8_t rotation;
    int8_t x;
    int8_t y;
    uint16_t operation_id;
    uint16_t required_deleted_row_mask;
    uint64_t mask;
} clr_buildup_operation;typedef struct clr_buildup_operation_set {
    uint16_t operation_count;
    uint16_t geometry_variant_domains;
    uint16_t representative_order_hint[CLR_BUILDUP_MAX_OPERATIONS];
    uint16_t reserved_tail[3];
    clr_buildup_operation operations[CLR_BUILDUP_MAX_OPERATIONS];
} clr_buildup_operation_set;typedef struct clr_bag_window {
    uint16_t start;
    uint16_t len;
    uint8_t boundary_known;
    uint8_t reserved[3];
} clr_bag_window;clr_bag_window clearra_bag_window_from_queue_and_piece_window(
    const clr_queue_view *queue,
    const clr_piece_window_descriptor *piece_window);
bool clearra_bag_window_boundary_known(const clr_bag_window *window);
uint16_t clearra_buildup_mvp1_max_operations(void);
clr_buildup_status clearra_buildup_operation_set_runtime_status(
    uint32_t operation_count);
#endif
