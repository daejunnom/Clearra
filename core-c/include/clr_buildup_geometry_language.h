#ifndef CLR_BUILDUP_GEOMETRY_LANGUAGE_H
#define CLR_BUILDUP_GEOMETRY_LANGUAGE_H

#include "clr_buildup_problem.h"

#include <stddef.h>
#include <stdint.h>

typedef struct clr_buildup_geometry_language_node {
    uint32_t first_edge;
    uint16_t edge_count;
    uint8_t accepting;
    uint8_t depth;
} clr_buildup_geometry_language_node;

typedef struct clr_buildup_geometry_language_edge {
    uint32_t child_node_index;
    uint16_t operation_index;
    uint8_t piece;
    uint8_t reserved;
} clr_buildup_geometry_language_edge;

typedef struct clr_buildup_geometry_language_report {
    uint64_t candidate_id;
    uint64_t canonical_operation_set_id;
    uint32_t root_node_index;
    uint32_t node_count;
    uint32_t edge_count;
    uint8_t complete;
    uint8_t reserved[3];
} clr_buildup_geometry_language_report;

/*
 * Version 2 is a prepared snapshot.  Preparing owns the potentially expensive
 * DAG construction; copying only reads that frozen workspace snapshot.  This
 * split is intentional so callers can size their buffers without causing the
 * search to run a second time.
 */
typedef enum clr_buildup_geometry_transition_mode {
    CLR_BUILDUP_GEOMETRY_TRANSITION_REACHABLE = 0,
    CLR_BUILDUP_GEOMETRY_TRANSITION_GEOMETRY_ONLY = 1
} clr_buildup_geometry_transition_mode;

typedef struct clr_buildup_geometry_language_node_v2 {
    uint64_t board_mask;
    uint64_t reachability_relevant_state;
    uint32_t first_edge;
    uint16_t edge_count;
    uint16_t remaining_operations;
    uint16_t deleted_row_mask;
    uint8_t deleted_count;
    uint8_t cleared_lines;
    uint8_t accepting;
    uint8_t depth;
    uint8_t reserved[2];
} clr_buildup_geometry_language_node_v2;

typedef struct clr_buildup_geometry_language_edge_v2 {
    uint64_t target_mask;
    uint32_t child_node_index;
    uint16_t operation_index;
    uint16_t cleared_row_mask;
    int8_t x;
    int8_t adjusted_y;
    uint8_t piece;
    uint8_t rotation;
    uint8_t cleared_lines;
    uint8_t reserved[3];
} clr_buildup_geometry_language_edge_v2;

typedef struct clr_buildup_geometry_language_report_v2 {
    uint64_t candidate_id;
    uint64_t canonical_operation_set_id;
    uint64_t snapshot_id;
    uint32_t root_node_index;
    uint32_t node_count;
    uint32_t edge_count;
    uint8_t complete;
    uint8_t transition_mode;
    uint8_t format_version;
    uint8_t reserved;
} clr_buildup_geometry_language_report_v2;

typedef struct clr_buildup_workspace clr_buildup_workspace;

clr_buildup_status clr_buildup_export_geometry_language_with_workspace(
    const clr_buildup_problem *problem,
    clr_buildup_workspace *workspace,
    clr_buildup_geometry_language_node *nodes,
    size_t node_capacity,
    clr_buildup_geometry_language_edge *edges,
    size_t edge_capacity,
    clr_buildup_geometry_language_report *out_report);

clr_buildup_status clr_buildup_prepare_geometry_language_v2_with_workspace(
    const clr_buildup_problem *problem,
    clr_buildup_workspace *workspace,
    clr_buildup_geometry_transition_mode transition_mode,
    clr_buildup_geometry_language_report_v2 *out_report);

clr_buildup_status clr_buildup_copy_prepared_geometry_language_v2(
    const clr_buildup_workspace *workspace,
    clr_buildup_geometry_language_node_v2 *nodes,
    size_t node_capacity,
    clr_buildup_geometry_language_edge_v2 *edges,
    size_t edge_capacity,
    clr_buildup_geometry_language_report_v2 *out_report);

#endif
