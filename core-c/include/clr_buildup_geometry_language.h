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

typedef struct clr_buildup_workspace clr_buildup_workspace;

clr_buildup_status clr_buildup_export_geometry_language_with_workspace(
    const clr_buildup_problem *problem,
    clr_buildup_workspace *workspace,
    clr_buildup_geometry_language_node *nodes,
    size_t node_capacity,
    clr_buildup_geometry_language_edge *edges,
    size_t edge_capacity,
    clr_buildup_geometry_language_report *out_report);

#endif
