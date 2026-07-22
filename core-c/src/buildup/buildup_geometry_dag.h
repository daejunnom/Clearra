#ifndef CLEARRA_BUILDUP_GEOMETRY_DAG_H
#define CLEARRA_BUILDUP_GEOMETRY_DAG_H

#include "buildup_internal.h"
#include "clr_buildup_geometry_language.h"

typedef struct ClearraBuildUpSearchContext ClearraBuildUpSearchContext;
typedef struct ClearraBuildUpGeometryNode ClearraBuildUpGeometryNode;
typedef struct ClearraBuildUpGeometryNodeChunk ClearraBuildUpGeometryNodeChunk;
typedef struct ClearraBuildUpGeometryEdgeChunk ClearraBuildUpGeometryEdgeChunk;
typedef struct ClearraBuildUpGeometryTraceChunk ClearraBuildUpGeometryTraceChunk;

typedef struct ClearraBuildUpGeometryDag {
    clr_board_descriptor initial_board;
    clr_rule_profile_descriptor rule;
    uint64_t candidate_id;
    uint64_t canonical_operation_set_id;
    ClearraBuildUpGeometryNode **buckets;
    uint32_t *touched_buckets;
    size_t bucket_count;
    size_t touched_bucket_count;
    ClearraBuildUpGeometryNodeChunk *node_chunks;
    ClearraBuildUpGeometryNodeChunk *node_tail;
    ClearraBuildUpGeometryNodeChunk *node_cursor;
    ClearraBuildUpGeometryEdgeChunk *edge_chunks;
    ClearraBuildUpGeometryEdgeChunk *edge_tail;
    ClearraBuildUpGeometryEdgeChunk *edge_cursor;
    ClearraBuildUpGeometryTraceChunk *trace_chunks;
    ClearraBuildUpGeometryTraceChunk *trace_tail;
    ClearraBuildUpGeometryTraceChunk *trace_cursor;
    ClearraBuildUpGeometryNode *root;
    uint32_t node_count;
    uint32_t edge_count;
    uint32_t export_node_count;
    uint32_t export_edge_count;
    size_t retained_bytes;
    uint16_t operation_count;
    uint8_t prepared;
    uint8_t available;
    uint8_t capture_trace;
    uint8_t bucket_growth_disabled;
    uint8_t reachability_trace_mode;
    uint8_t reserved;
} ClearraBuildUpGeometryDag;

clr_buildup_status clearra_buildup_geometry_dag_prepare(
    ClearraBuildUpGeometryDag *dag,
    ClearraBuildUpSearchContext *context);
void clearra_buildup_geometry_dag_release(ClearraBuildUpGeometryDag *dag);
size_t clearra_buildup_geometry_dag_retained_bytes(
    const ClearraBuildUpGeometryDag *dag);
bool clearra_buildup_geometry_dag_is_available(
    const ClearraBuildUpGeometryDag *dag);
clr_buildup_status clearra_buildup_geometry_dag_export(
    const ClearraBuildUpGeometryDag *dag,
    clr_buildup_geometry_language_node *nodes,
    size_t node_capacity,
    clr_buildup_geometry_language_edge *edges,
    size_t edge_capacity,
    clr_buildup_geometry_language_report *out_report);
clr_buildup_status clearra_buildup_search_geometry_dag(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpGeometryDag *dag,
    ClearraBuildUpQueueHold queue_hold);

#endif
