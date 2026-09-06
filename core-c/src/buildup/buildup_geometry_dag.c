/* SRP rationale: this module has one behavior-level change reason: constructing and exporting the exact BuildUp geometry DAG. */

#include "buildup_geometry_dag.h"

#include "buildup_search_internal.h"
#include "clr_execution_control.h"
#include "clr_search_profile.h"

#include <stdlib.h>
#include <string.h>

#define CLEARRA_BUILDUP_GEOMETRY_NODE_CHUNK_CAPACITY 256u
#define CLEARRA_BUILDUP_GEOMETRY_EDGE_CHUNK_CAPACITY 512u
#define CLEARRA_BUILDUP_GEOMETRY_TRACE_CHUNK_CAPACITY 512u
#define CLEARRA_BUILDUP_GEOMETRY_EDGE_DATA_CHUNK_CAPACITY 512u
#define CLEARRA_BUILDUP_GEOMETRY_BUCKET_COUNT 4096u
#define CLEARRA_BUILDUP_GEOMETRY_INDEX_LOAD_NUMERATOR 3u
#define CLEARRA_BUILDUP_GEOMETRY_INDEX_LOAD_DENOMINATOR 4u

_Static_assert(
    sizeof(clr_buildup_geometry_language_node_v2) == 32u,
    "BuildUp geometry language v2 node ABI changed");
_Static_assert(
    sizeof(clr_buildup_geometry_language_edge_v2) == 24u,
    "BuildUp geometry language v2 edge ABI changed");
_Static_assert(
    sizeof(clr_buildup_geometry_language_report_v2) == 40u,
    "BuildUp geometry language v2 report ABI changed");
_Static_assert(
    sizeof(clr_buildup_geometry_transition_mode) == sizeof(int),
    "BuildUp geometry transition mode ABI changed");

typedef struct ClearraBuildUpGeometryTracePayload {
    clr_buildup_trace_step trace_step;
    clr_kick_evidence_view kick_evidence;
} ClearraBuildUpGeometryTracePayload;

typedef struct ClearraBuildUpGeometryEdgeData {
    uint64_t target_mask;
    uint16_t cleared_row_mask;
    int8_t adjusted_y;
    int8_t x;
    uint8_t rotation;
    uint8_t cleared_lines;
    uint8_t reserved[2];
} ClearraBuildUpGeometryEdgeData;

typedef union ClearraBuildUpGeometryEdgePayload {
    ClearraBuildUpGeometryTracePayload *trace;
    ClearraBuildUpGeometryEdgeData *geometry;
} ClearraBuildUpGeometryEdgePayload;

typedef struct ClearraBuildUpGeometryEdge {
    struct ClearraBuildUpGeometryEdge *next;
    ClearraBuildUpGeometryNode *child;
    ClearraBuildUpGeometryEdgePayload payload;
    uint16_t operation_index;
    uint8_t piece;
    uint8_t reserved[5];
} ClearraBuildUpGeometryEdge;

_Static_assert(
    sizeof(ClearraBuildUpGeometryEdge) == 32u,
    "BuildUp geometry hot edges must fit two per 64-byte cache line");

struct ClearraBuildUpGeometryNode {
    struct ClearraBuildUpGeometryNode *hash_next;
    ClearraBuildUpGeometryEdge *first_edge;
    ClearraBuildUpGeometryEdge *last_edge;
    uint64_t key_hash;
    uint64_t board_mask;
    uint64_t reachability_relevant_state;
    uint16_t remaining_operations;
    uint16_t deleted_row_mask;
    uint32_t export_index;
    uint8_t deleted_count;
    uint8_t cleared_lines;
    uint8_t expanded;
    uint8_t accepting;
    uint8_t live;
};

_Static_assert(
    sizeof(ClearraBuildUpGeometryNode) == 64u,
    "BuildUp geometry hot nodes must fit one 64-byte cache line");

struct ClearraBuildUpGeometryNodeChunk {
    struct ClearraBuildUpGeometryNodeChunk *next;
    size_t used;
    ClearraBuildUpGeometryNode
        nodes[CLEARRA_BUILDUP_GEOMETRY_NODE_CHUNK_CAPACITY];
};

struct ClearraBuildUpGeometryEdgeChunk {
    struct ClearraBuildUpGeometryEdgeChunk *next;
    size_t used;
    ClearraBuildUpGeometryEdge
        edges[CLEARRA_BUILDUP_GEOMETRY_EDGE_CHUNK_CAPACITY];
};

struct ClearraBuildUpGeometryTraceChunk {
    struct ClearraBuildUpGeometryTraceChunk *next;
    size_t used;
    ClearraBuildUpGeometryTracePayload
        traces[CLEARRA_BUILDUP_GEOMETRY_TRACE_CHUNK_CAPACITY];
};

struct ClearraBuildUpGeometryEdgeDataChunk {
    struct ClearraBuildUpGeometryEdgeDataChunk *next;
    size_t used;
    ClearraBuildUpGeometryEdgeData
        values[CLEARRA_BUILDUP_GEOMETRY_EDGE_DATA_CHUNK_CAPACITY];
};

typedef struct ClearraBuildUpGeometryPathStep {
    const ClearraBuildUpGeometryEdge *edge;
    uint8_t branch_kind;
    uint8_t used_hold;
    uint8_t incoming_piece;
    uint8_t held_piece_before;
    uint8_t hold_empty_before;
    uint8_t reserved[3];
} ClearraBuildUpGeometryPathStep;

typedef struct ClearraBuildUpGeometryPath {
    ClearraBuildUpGeometryPathStep steps[CLR_BUILDUP_MAX_OPERATIONS];
} ClearraBuildUpGeometryPath;

static void reset_chunks(ClearraBuildUpGeometryDag *dag) {
    for (ClearraBuildUpGeometryNodeChunk *chunk = dag->node_chunks;
         chunk != 0;
         chunk = chunk->next) {
        chunk->used = 0u;
    }
    for (ClearraBuildUpGeometryEdgeChunk *chunk = dag->edge_chunks;
         chunk != 0;
         chunk = chunk->next) {
        chunk->used = 0u;
    }
    for (ClearraBuildUpGeometryTraceChunk *chunk = dag->trace_chunks;
         chunk != 0;
         chunk = chunk->next) {
        chunk->used = 0u;
    }
    for (ClearraBuildUpGeometryEdgeDataChunk *chunk = dag->edge_data_chunks;
         chunk != 0;
         chunk = chunk->next) {
        chunk->used = 0u;
    }
    if (dag->buckets != 0 && dag->touched_buckets != 0) {
        for (size_t index = 0u;
             index < dag->touched_bucket_count;
             ++index) {
            dag->buckets[dag->touched_buckets[index]] = 0;
        }
    }
    dag->touched_bucket_count = 0u;
    dag->node_cursor = dag->node_chunks;
    dag->edge_cursor = dag->edge_chunks;
    dag->trace_cursor = dag->trace_chunks;
    dag->edge_data_cursor = dag->edge_data_chunks;
    dag->root = 0;
    dag->node_count = 0u;
    dag->edge_count = 0u;
    dag->export_node_count = 0u;
    dag->export_edge_count = 0u;
    dag->available = 0u;
}

static bool ensure_buckets(ClearraBuildUpGeometryDag *dag) {
    if (dag->buckets != 0) {
        return true;
    }
    size_t bucket_count = CLEARRA_BUILDUP_GEOMETRY_BUCKET_COUNT;
    ClearraBuildUpGeometryNode **buckets =
        (ClearraBuildUpGeometryNode **)malloc(
            bucket_count * sizeof(*buckets));
    uint32_t *touched_buckets =
        (uint32_t *)malloc(bucket_count * sizeof(*touched_buckets));
    if (buckets == 0 || touched_buckets == 0) {
        free(buckets);
        free(touched_buckets);
        return false;
    }
    memset(buckets, 0, bucket_count * sizeof(*buckets));
    dag->buckets = buckets;
    dag->touched_buckets = touched_buckets;
    dag->bucket_count = bucket_count;
    dag->touched_bucket_count = 0u;
    dag->retained_bytes +=
        bucket_count * (sizeof(*buckets) + sizeof(*touched_buckets));
    return true;
}

static ClearraBuildUpGeometryNode *allocate_node(
    ClearraBuildUpGeometryDag *dag) {
    ClearraBuildUpGeometryNodeChunk *chunk = dag->node_cursor;
    if (chunk != 0 &&
        chunk->used == CLEARRA_BUILDUP_GEOMETRY_NODE_CHUNK_CAPACITY) {
        chunk = chunk->next;
        dag->node_cursor = chunk;
    }
    if (chunk == 0) {
        chunk = (ClearraBuildUpGeometryNodeChunk *)malloc(sizeof(*chunk));
        if (chunk == 0) {
            return 0;
        }
        chunk->used = 0u;
        chunk->next = 0;
        if (dag->node_tail == 0) {
            dag->node_chunks = chunk;
        } else {
            dag->node_tail->next = chunk;
        }
        dag->node_tail = chunk;
        dag->node_cursor = chunk;
        dag->retained_bytes += sizeof(*chunk);
    }
    ClearraBuildUpGeometryNode *node = &chunk->nodes[chunk->used++];
    *node = (ClearraBuildUpGeometryNode){0};
    if (dag->node_count == UINT32_MAX) {
        chunk->used--;
        return 0;
    }
    node->export_index = dag->node_count++;
    return node;
}

static ClearraBuildUpGeometryEdge *allocate_edge(
    ClearraBuildUpGeometryDag *dag) {
    ClearraBuildUpGeometryEdgeChunk *chunk = dag->edge_cursor;
    if (chunk != 0 &&
        chunk->used == CLEARRA_BUILDUP_GEOMETRY_EDGE_CHUNK_CAPACITY) {
        chunk = chunk->next;
        dag->edge_cursor = chunk;
    }
    if (chunk == 0) {
        chunk = (ClearraBuildUpGeometryEdgeChunk *)malloc(sizeof(*chunk));
        if (chunk == 0) {
            return 0;
        }
        chunk->used = 0u;
        chunk->next = 0;
        if (dag->edge_tail == 0) {
            dag->edge_chunks = chunk;
        } else {
            dag->edge_tail->next = chunk;
        }
        dag->edge_tail = chunk;
        dag->edge_cursor = chunk;
        dag->retained_bytes += sizeof(*chunk);
    }
    ClearraBuildUpGeometryEdge *edge = &chunk->edges[chunk->used++];
    *edge = (ClearraBuildUpGeometryEdge){0};
    if (dag->edge_count == UINT32_MAX) {
        chunk->used--;
        return 0;
    }
    dag->edge_count++;
    return edge;
}

static ClearraBuildUpGeometryTracePayload *allocate_trace(
    ClearraBuildUpGeometryDag *dag) {
    ClearraBuildUpGeometryTraceChunk *chunk = dag->trace_cursor;
    if (chunk != 0 &&
        chunk->used == CLEARRA_BUILDUP_GEOMETRY_TRACE_CHUNK_CAPACITY) {
        chunk = chunk->next;
        dag->trace_cursor = chunk;
    }
    if (chunk == 0) {
        chunk = (ClearraBuildUpGeometryTraceChunk *)malloc(sizeof(*chunk));
        if (chunk == 0) {
            return 0;
        }
        chunk->used = 0u;
        chunk->next = 0;
        if (dag->trace_tail == 0) {
            dag->trace_chunks = chunk;
        } else {
            dag->trace_tail->next = chunk;
        }
        dag->trace_tail = chunk;
        dag->trace_cursor = chunk;
        dag->retained_bytes += sizeof(*chunk);
    }
    ClearraBuildUpGeometryTracePayload *trace =
        &chunk->traces[chunk->used++];
    *trace = (ClearraBuildUpGeometryTracePayload){0};
    return trace;
}

static ClearraBuildUpGeometryEdgeData *allocate_edge_data(
    ClearraBuildUpGeometryDag *dag) {
    ClearraBuildUpGeometryEdgeDataChunk *chunk = dag->edge_data_cursor;
    if (chunk != 0 &&
        chunk->used == CLEARRA_BUILDUP_GEOMETRY_EDGE_DATA_CHUNK_CAPACITY) {
        chunk = chunk->next;
        dag->edge_data_cursor = chunk;
    }
    if (chunk == 0) {
        chunk = (ClearraBuildUpGeometryEdgeDataChunk *)malloc(sizeof(*chunk));
        if (chunk == 0) {
            return 0;
        }
        chunk->used = 0u;
        chunk->next = 0;
        if (dag->edge_data_tail == 0) {
            dag->edge_data_chunks = chunk;
        } else {
            dag->edge_data_tail->next = chunk;
        }
        dag->edge_data_tail = chunk;
        dag->edge_data_cursor = chunk;
        dag->retained_bytes += sizeof(*chunk);
    }
    ClearraBuildUpGeometryEdgeData *value = &chunk->values[chunk->used++];
    *value = (ClearraBuildUpGeometryEdgeData){0};
    return value;
}

static uint64_t geometry_hash(
    const ClearraBuildUpState *state,
    uint16_t remaining_operations) {
    uint64_t hash = state->board_mask ^ UINT64_C(0x9e3779b97f4a7c15);
    hash ^= state->reachability_relevant_state + UINT64_C(0x517cc1b727220a95);
    hash ^= (uint64_t)remaining_operations << 32u;
    hash ^= (uint64_t)state->line_clear_state.deleted_row_mask << 16u;
    hash ^= (uint64_t)state->line_clear_state.deleted_count << 8u;
    hash ^= state->cleared_lines;
    hash = (hash ^ (hash >> 30u)) * UINT64_C(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27u)) * UINT64_C(0x94d049bb133111eb);
    return hash ^ (hash >> 31u);
}

static bool bucket_index_should_grow(
    const ClearraBuildUpGeometryDag *dag) {
    if (dag->bucket_count == 0u || dag->node_count == UINT32_MAX) {
        return false;
    }
    size_t threshold =
        dag->bucket_count / CLEARRA_BUILDUP_GEOMETRY_INDEX_LOAD_DENOMINATOR *
        CLEARRA_BUILDUP_GEOMETRY_INDEX_LOAD_NUMERATOR;
    return (size_t)dag->node_count >= threshold;
}

static void grow_bucket_index_best_effort(
    ClearraBuildUpGeometryDag *dag) {
    if (dag->bucket_growth_disabled != 0u ||
        !bucket_index_should_grow(dag)) {
        return;
    }
    if (dag->bucket_count > UINT32_MAX / 2u ||
        dag->bucket_count > SIZE_MAX / 2u) {
        dag->bucket_growth_disabled = 1u;
        return;
    }
    size_t new_bucket_count = dag->bucket_count * 2u;
    if (new_bucket_count > SIZE_MAX / sizeof(*dag->buckets) ||
        new_bucket_count > SIZE_MAX / sizeof(*dag->touched_buckets)) {
        dag->bucket_growth_disabled = 1u;
        return;
    }

    ClearraBuildUpGeometryNode **new_buckets =
        (ClearraBuildUpGeometryNode **)malloc(
            new_bucket_count * sizeof(*new_buckets));
    uint32_t *new_touched_buckets =
        (uint32_t *)malloc(
            new_bucket_count * sizeof(*new_touched_buckets));
    if (new_buckets == 0 || new_touched_buckets == 0) {
        free(new_buckets);
        free(new_touched_buckets);
        dag->bucket_growth_disabled = 1u;
        return;
    }
    memset(new_buckets, 0, new_bucket_count * sizeof(*new_buckets));

    size_t new_touched_count = 0u;
    for (ClearraBuildUpGeometryNodeChunk *chunk = dag->node_chunks;
         chunk != 0;
         chunk = chunk->next) {
        for (size_t index = 0u; index < chunk->used; ++index) {
            ClearraBuildUpGeometryNode *node = &chunk->nodes[index];
            size_t bucket = (size_t)node->key_hash & (new_bucket_count - 1u);
            if (new_buckets[bucket] == 0) {
                new_touched_buckets[new_touched_count++] = (uint32_t)bucket;
            }
            node->hash_next = new_buckets[bucket];
            new_buckets[bucket] = node;
        }
    }

    size_t old_index_bytes = dag->bucket_count *
        (sizeof(*dag->buckets) + sizeof(*dag->touched_buckets));
    size_t new_index_bytes = new_bucket_count *
        (sizeof(*new_buckets) + sizeof(*new_touched_buckets));
    free(dag->buckets);
    free(dag->touched_buckets);
    dag->buckets = new_buckets;
    dag->touched_buckets = new_touched_buckets;
    dag->bucket_count = new_bucket_count;
    dag->touched_bucket_count = new_touched_count;
    dag->retained_bytes = dag->retained_bytes - old_index_bytes +
                          new_index_bytes;
}

static bool node_matches(
    const ClearraBuildUpGeometryNode *node,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations) {
    return node->board_mask == state->board_mask &&
           node->reachability_relevant_state ==
               state->reachability_relevant_state &&
           node->remaining_operations == remaining_operations &&
           node->deleted_row_mask ==
               state->line_clear_state.deleted_row_mask &&
           node->deleted_count == state->line_clear_state.deleted_count &&
           node->cleared_lines == state->cleared_lines;
}

static uint8_t count_operation_bits(uint16_t value) {
    uint8_t count = 0u;
    while (value != 0u) {
        value = (uint16_t)(value & (uint16_t)(value - 1u));
        count++;
    }
    return count;
}

static ClearraBuildUpGeometryNode *find_or_insert_node(
    ClearraBuildUpGeometryDag *dag,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations) {
    uint64_t key_hash = geometry_hash(state, remaining_operations);
    size_t bucket = (size_t)key_hash & (dag->bucket_count - 1u);
    for (ClearraBuildUpGeometryNode *node = dag->buckets[bucket];
         node != 0;
         node = node->hash_next) {
        if (node->key_hash == key_hash &&
            node_matches(node, state, remaining_operations)) {
            return node;
        }
    }
    grow_bucket_index_best_effort(dag);
    bucket = (size_t)key_hash & (dag->bucket_count - 1u);
    ClearraBuildUpGeometryNode *node = allocate_node(dag);
    if (node == 0) {
        return 0;
    }
    if (dag->buckets[bucket] == 0) {
        if (dag->touched_bucket_count >= dag->bucket_count) {
            return 0;
        }
        dag->touched_buckets[dag->touched_bucket_count++] =
            (uint32_t)bucket;
    }
    node->key_hash = key_hash;
    node->board_mask = state->board_mask;
    node->reachability_relevant_state = state->reachability_relevant_state;
    node->remaining_operations = remaining_operations;
    node->deleted_row_mask = state->line_clear_state.deleted_row_mask;
    node->deleted_count = state->line_clear_state.deleted_count;
    node->cleared_lines = state->cleared_lines;
    node->hash_next = dag->buckets[bucket];
    dag->buckets[bucket] = node;
    return node;
}

static ClearraBuildUpState state_from_node(
    const ClearraBuildUpGeometryNode *node,
    ClearraBuildUpQueueHold queue_hold,
    uint16_t depth) {
    ClearraBuildUpState state = {0};
    state.board_mask = node->board_mask;
    state.line_clear_state.deleted_row_mask = node->deleted_row_mask;
    state.line_clear_state.deleted_count = node->deleted_count;
    state.hold_automaton_state = queue_hold;
    state.reachability_relevant_state = node->reachability_relevant_state;
    state.placed_pieces = depth;
    state.cleared_lines = node->cleared_lines;
    return state;
}

static clr_buildup_status expand_node(
    ClearraBuildUpGeometryDag *dag,
    ClearraBuildUpSearchContext *context,
    ClearraBuildUpGeometryNode *node,
    uint16_t depth) {
    if (node->expanded != 0u) {
        return CLR_BUILDUP_OK;
    }
    node->expanded = 1u;
    if (node->remaining_operations == 0u) {
        node->accepting = (uint8_t)(node->board_mask == 0u);
        node->live = node->accepting;
        return CLR_BUILDUP_OK;
    }
    if (clr_execution_cancel_requested()) {
        return CLR_BUILDUP_CANCELLED;
    }

    ClearraBuildUpState state = state_from_node(
        node, (ClearraBuildUpQueueHold){0}, depth);
    for (uint16_t preference = 0u;
         preference < context->order.count;
         ++preference) {
        uint16_t operation_index = context->order.indices[preference];
        uint16_t operation_bit = (uint16_t)(UINT16_C(1) << operation_index);
        if ((node->remaining_operations & operation_bit) == 0u) {
            continue;
        }
        if (!clearra_buildup_operation_source_may_match_clear_state(
                &context->operation_source, &state, operation_index)) {
            clr_search_profile_count(
                CLR_PROFILE_BUILDUP_CLEAR_STATE_SKIPS, 1u);
            continue;
        }
        clr_buildup_operation variants[CLR_BUILDUP_MAX_OPERATION_VARIANTS];
        uint8_t variant_count = 0u;
        clr_buildup_status status = clearra_buildup_operation_variants_for_state(
            context, &state, operation_index, variants, &variant_count);
        if (status != CLR_BUILDUP_OK) {
            if (clearra_buildup_branch_outcome_for_status(status) ==
                CLEARRA_BUILDUP_BRANCH_LOGICAL_REJECT) {
                continue;
            }
            return status;
        }
        for (uint8_t variant_index = 0u;
             variant_index < variant_count;
             ++variant_index) {
            ClearraBuildUpState next_state;
            clr_buildup_trace_step trace_step;
            clr_kick_evidence_view kick_evidence;
            ClearraBuildUpGeometryTransitionView geometry = {0};
            status = clearra_buildup_search_try_operation_with_geometry(
                context,
                state,
                (ClearraBuildUpQueueHold){0},
                &variants[variant_index],
                operation_index,
                &next_state,
                &trace_step,
                &kick_evidence,
                dag->capture_geometry != 0u ? &geometry : 0);
            if (status != CLR_BUILDUP_OK) {
                if (clearra_buildup_branch_outcome_for_status(status) ==
                    CLEARRA_BUILDUP_BRANCH_LOGICAL_REJECT) {
                    continue;
                }
                return status;
            }
            next_state.hold_automaton_state = (ClearraBuildUpQueueHold){0};
            next_state.last_hold_branch_kind = 0u;
            uint16_t next_remaining =
                (uint16_t)(node->remaining_operations & ~operation_bit);
            ClearraBuildUpGeometryNode *child = find_or_insert_node(
                dag, &next_state, next_remaining);
            ClearraBuildUpGeometryEdge *edge = allocate_edge(dag);
            ClearraBuildUpGeometryTracePayload *trace =
                dag->capture_trace != 0u ? allocate_trace(dag) : 0;
            ClearraBuildUpGeometryEdgeData *edge_data =
                dag->capture_geometry != 0u ? allocate_edge_data(dag) : 0;
            if (child == 0 || edge == 0 ||
                (dag->capture_trace != 0u && trace == 0) ||
                (dag->capture_geometry != 0u && edge_data == 0)) {
                return CLR_BUILDUP_CAPACITY_EXCEEDED;
            }
            edge->child = child;
            edge->operation_index = operation_index;
            edge->piece = variants[variant_index].piece;
            edge->payload.trace = trace;
            if (trace != 0) {
                trace->trace_step = trace_step;
                trace->kick_evidence = kick_evidence;
            }
            if (edge_data != 0) {
                edge->payload.geometry = edge_data;
                *edge_data = (ClearraBuildUpGeometryEdgeData){
                    .target_mask = geometry.target_mask,
                    .cleared_row_mask = geometry.cleared_row_mask,
                    .adjusted_y = geometry.adjusted_y,
                    .x = variants[variant_index].x,
                    .rotation = variants[variant_index].rotation,
                    .cleared_lines = geometry.cleared_lines,
                };
            }
            if (node->last_edge == 0) {
                node->first_edge = edge;
            } else {
                node->last_edge->next = edge;
            }
            node->last_edge = edge;
            status = expand_node(dag, context, child, (uint16_t)(depth + 1u));
            if (status != CLR_BUILDUP_OK) {
                return status;
            }
            if (child->live != 0u) {
                node->live = 1u;
            }
        }
    }
    return CLR_BUILDUP_OK;
}

static bool node_is_exported(
    const ClearraBuildUpGeometryDag *dag,
    const ClearraBuildUpGeometryNode *node) {
    return node == dag->root || node->live != 0u;
}

static clr_buildup_status finalize_export_layout(
    ClearraBuildUpGeometryDag *dag) {
    uint32_t export_node_count = 0u;
    uint32_t export_edge_count = 0u;
    for (ClearraBuildUpGeometryNodeChunk *chunk = dag->node_chunks;
         chunk != 0;
         chunk = chunk->next) {
        for (size_t index = 0u; index < chunk->used; ++index) {
            ClearraBuildUpGeometryNode *node = &chunk->nodes[index];
            if (!node_is_exported(dag, node)) {
                node->export_index = UINT32_MAX;
                continue;
            }
            if (export_node_count == UINT32_MAX) {
                return CLR_BUILDUP_CAPACITY_EXCEEDED;
            }
            node->export_index = export_node_count++;
            for (const ClearraBuildUpGeometryEdge *edge = node->first_edge;
                 edge != 0;
                 edge = edge->next) {
                if (edge->child->live == 0u) {
                    continue;
                }
                if (export_edge_count == UINT32_MAX) {
                    return CLR_BUILDUP_CAPACITY_EXCEEDED;
                }
                export_edge_count++;
            }
        }
    }
    if (dag->root == 0 || dag->root->export_index == UINT32_MAX ||
        export_node_count == 0u) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    dag->export_node_count = export_node_count;
    dag->export_edge_count = export_edge_count;
    return CLR_BUILDUP_OK;
}

clr_buildup_status clearra_buildup_geometry_dag_prepare_with_options(
    ClearraBuildUpGeometryDag *dag,
    ClearraBuildUpSearchContext *context,
    uint8_t capture_geometry) {
    if (dag == 0 || context == 0 || context->problem == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    capture_geometry = capture_geometry != 0u ? 1u : 0u;
    if (capture_geometry != 0u && context->capture_trace != 0u) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    const clr_buildup_problem *problem = context->problem;
    const uint64_t previous_snapshot_id = dag->snapshot_id;
    if (dag->prepared != 0u &&
        (dag->capture_trace != context->capture_trace ||
         dag->capture_geometry != capture_geometry ||
         dag->reachability_trace_mode !=
             context->reachability_trace_mode ||
         dag->transition_mode != context->geometry_transition_mode)) {
        clearra_buildup_geometry_dag_release(dag);
        dag->snapshot_id = previous_snapshot_id;
    } else {
        reset_chunks(dag);
    }
    dag->initial_board = problem->initial_board;
    dag->rule = problem->rule;
    dag->candidate_id = problem->candidate_id;
    dag->canonical_operation_set_id = problem->canonical_operation_set_id;
    dag->operation_count = context->operation_source.operation_count;
    dag->capture_trace = context->capture_trace;
    dag->capture_geometry = capture_geometry;
    dag->reachability_trace_mode = context->reachability_trace_mode;
    dag->transition_mode = context->geometry_transition_mode;
    dag->prepared = 1u;
    if (!ensure_buckets(dag)) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    ClearraBuildUpState initial_state = clearra_buildup_state_initial(problem);
    initial_state.hold_automaton_state = (ClearraBuildUpQueueHold){0};
    uint16_t remaining_operations = 0u;
    clr_buildup_status status = clearra_buildup_remaining_ops_bitset_for_count(
        context->order.count, &remaining_operations);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    dag->root = find_or_insert_node(dag, &initial_state, remaining_operations);
    if (dag->root == 0) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    status = expand_node(dag, context, dag->root, 0u);
    if (status == CLR_BUILDUP_CAPACITY_EXCEEDED) {
        reset_chunks(dag);
        return status;
    }
    if (status != CLR_BUILDUP_OK) {
        reset_chunks(dag);
        return status;
    }
    status = finalize_export_layout(dag);
    if (status != CLR_BUILDUP_OK) {
        reset_chunks(dag);
        return status;
    }
    dag->available = 1u;
    dag->snapshot_id++;
    if (dag->snapshot_id == 0u) {
        dag->snapshot_id = 1u;
    }
    return CLR_BUILDUP_OK;
}

clr_buildup_status clearra_buildup_geometry_dag_prepare(
    ClearraBuildUpGeometryDag *dag,
    ClearraBuildUpSearchContext *context) {
    return clearra_buildup_geometry_dag_prepare_with_options(
        dag, context, 0u);
}

void clearra_buildup_geometry_dag_release(ClearraBuildUpGeometryDag *dag) {
    if (dag == 0) {
        return;
    }
    ClearraBuildUpGeometryNodeChunk *node_chunk = dag->node_chunks;
    while (node_chunk != 0) {
        ClearraBuildUpGeometryNodeChunk *next = node_chunk->next;
        free(node_chunk);
        node_chunk = next;
    }
    ClearraBuildUpGeometryEdgeChunk *edge_chunk = dag->edge_chunks;
    while (edge_chunk != 0) {
        ClearraBuildUpGeometryEdgeChunk *next = edge_chunk->next;
        free(edge_chunk);
        edge_chunk = next;
    }
    ClearraBuildUpGeometryTraceChunk *trace_chunk = dag->trace_chunks;
    while (trace_chunk != 0) {
        ClearraBuildUpGeometryTraceChunk *next = trace_chunk->next;
        free(trace_chunk);
        trace_chunk = next;
    }
    ClearraBuildUpGeometryEdgeDataChunk *edge_data_chunk =
        dag->edge_data_chunks;
    while (edge_data_chunk != 0) {
        ClearraBuildUpGeometryEdgeDataChunk *next = edge_data_chunk->next;
        free(edge_data_chunk);
        edge_data_chunk = next;
    }
    free(dag->buckets);
    free(dag->touched_buckets);
    *dag = (ClearraBuildUpGeometryDag){0};
}

size_t clearra_buildup_geometry_dag_retained_bytes(
    const ClearraBuildUpGeometryDag *dag) {
    return dag == 0 ? 0u : dag->retained_bytes;
}

bool clearra_buildup_geometry_dag_is_available(
    const ClearraBuildUpGeometryDag *dag) {
    return dag != 0 && dag->prepared != 0u && dag->available != 0u &&
           dag->root != 0;
}

clr_buildup_status clearra_buildup_geometry_dag_export(
    const ClearraBuildUpGeometryDag *dag,
    clr_buildup_geometry_language_node *nodes,
    size_t node_capacity,
    clr_buildup_geometry_language_edge *edges,
    size_t edge_capacity,
    clr_buildup_geometry_language_report *out_report) {
    if (dag == 0 || out_report == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    *out_report = (clr_buildup_geometry_language_report){
        .candidate_id = dag->candidate_id,
        .canonical_operation_set_id = dag->canonical_operation_set_id,
        .root_node_index = dag->root == 0 ? 0u : dag->root->export_index,
        .node_count = dag->export_node_count,
        .edge_count = dag->export_edge_count,
        .complete = (uint8_t)clearra_buildup_geometry_dag_is_available(dag),
    };
    if (out_report->complete == 0u || (nodes == 0 && edges == 0)) {
        return CLR_BUILDUP_OK;
    }
    if (nodes == 0 || node_capacity < dag->export_node_count ||
        (dag->export_edge_count != 0u &&
         (edges == 0 || edge_capacity < dag->export_edge_count))) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }

    uint32_t edge_cursor = 0u;
    for (const ClearraBuildUpGeometryNodeChunk *chunk = dag->node_chunks;
         chunk != 0;
         chunk = chunk->next) {
        for (size_t index = 0u; index < chunk->used; ++index) {
            const ClearraBuildUpGeometryNode *node = &chunk->nodes[index];
            if (!node_is_exported(dag, node)) {
                continue;
            }
            clr_buildup_geometry_language_node *exported =
                &nodes[node->export_index];
            exported->first_edge = edge_cursor;
            exported->edge_count = 0u;
            exported->accepting = node->accepting;
            exported->depth = (uint8_t)(
                dag->operation_count -
                count_operation_bits(node->remaining_operations));
            for (const ClearraBuildUpGeometryEdge *edge = node->first_edge;
                 edge != 0;
                edge = edge->next) {
                if (edge->child->live == 0u) {
                    continue;
                }
                if (edge_cursor >= dag->export_edge_count ||
                    edge->operation_index >= dag->operation_count) {
                    return CLR_BUILDUP_INVALID_PROBLEM;
                }
                edges[edge_cursor++] = (clr_buildup_geometry_language_edge){
                    .child_node_index = edge->child->export_index,
                    .operation_index = edge->operation_index,
                    .piece = edge->piece,
                };
                if (exported->edge_count == UINT16_MAX) {
                    return CLR_BUILDUP_CAPACITY_EXCEEDED;
                }
                exported->edge_count++;
            }
        }
    }
    return edge_cursor == dag->export_edge_count ? CLR_BUILDUP_OK
                                                 : CLR_BUILDUP_INVALID_PROBLEM;
}

clr_buildup_status clearra_buildup_geometry_dag_export_v2(
    const ClearraBuildUpGeometryDag *dag,
    clr_buildup_geometry_language_node_v2 *nodes,
    size_t node_capacity,
    clr_buildup_geometry_language_edge_v2 *edges,
    size_t edge_capacity,
    clr_buildup_geometry_language_report_v2 *out_report) {
    if (dag == 0 || out_report == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    *out_report = (clr_buildup_geometry_language_report_v2){
        .candidate_id = dag->candidate_id,
        .canonical_operation_set_id = dag->canonical_operation_set_id,
        .snapshot_id = dag->snapshot_id,
        .root_node_index = dag->root == 0 ? 0u : dag->root->export_index,
        .node_count = dag->export_node_count,
        .edge_count = dag->export_edge_count,
        .complete = (uint8_t)(
            clearra_buildup_geometry_dag_is_available(dag) &&
            dag->capture_geometry != 0u),
        .transition_mode = dag->transition_mode,
        .format_version = 2u,
    };
    if (out_report->complete == 0u || (nodes == 0 && edges == 0)) {
        return CLR_BUILDUP_OK;
    }
    if (nodes == 0 || node_capacity < dag->export_node_count ||
        (dag->export_edge_count != 0u &&
         (edges == 0 || edge_capacity < dag->export_edge_count))) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }

    uint32_t edge_cursor = 0u;
    for (const ClearraBuildUpGeometryNodeChunk *chunk = dag->node_chunks;
         chunk != 0;
         chunk = chunk->next) {
        for (size_t index = 0u; index < chunk->used; ++index) {
            const ClearraBuildUpGeometryNode *node = &chunk->nodes[index];
            if (!node_is_exported(dag, node)) {
                continue;
            }
            clr_buildup_geometry_language_node_v2 *exported =
                &nodes[node->export_index];
            *exported = (clr_buildup_geometry_language_node_v2){
                .board_mask = node->board_mask,
                .reachability_relevant_state =
                    node->reachability_relevant_state,
                .first_edge = edge_cursor,
                .remaining_operations = node->remaining_operations,
                .deleted_row_mask = node->deleted_row_mask,
                .deleted_count = node->deleted_count,
                .cleared_lines = node->cleared_lines,
                .accepting = node->accepting,
                .depth = (uint8_t)(
                    dag->operation_count -
                    count_operation_bits(node->remaining_operations)),
            };
            for (const ClearraBuildUpGeometryEdge *edge = node->first_edge;
                 edge != 0;
                 edge = edge->next) {
                if (edge->child->live == 0u) {
                    continue;
                }
                const ClearraBuildUpGeometryEdgeData *geometry =
                    edge->payload.geometry;
                if (edge_cursor >= dag->export_edge_count || geometry == 0 ||
                    edge->operation_index >= dag->operation_count) {
                    return CLR_BUILDUP_INVALID_PROBLEM;
                }
                edges[edge_cursor++] =
                    (clr_buildup_geometry_language_edge_v2){
                        .target_mask = geometry->target_mask,
                        .child_node_index = edge->child->export_index,
                        .operation_index = edge->operation_index,
                        .cleared_row_mask = geometry->cleared_row_mask,
                        .x = geometry->x,
                        .adjusted_y = geometry->adjusted_y,
                        .piece = edge->piece,
                        .rotation = geometry->rotation,
                        .cleared_lines = geometry->cleared_lines,
                    };
                if (exported->edge_count == UINT16_MAX) {
                    return CLR_BUILDUP_CAPACITY_EXCEEDED;
                }
                exported->edge_count++;
            }
        }
    }
    return edge_cursor == dag->export_edge_count ? CLR_BUILDUP_OK
                                                 : CLR_BUILDUP_INVALID_PROBLEM;
}

static bool completion_memo_can_shortcut(
    const ClearraBuildUpSearchContext *context) {
    return context->stop_after_first_success == 0u &&
           (context->out_variants == 0 ||
            context->out_variants->count >= context->max_retained_variants);
}

static clr_buildup_status add_memoized_completions(
    ClearraBuildUpSearchContext *context,
    uint64_t completion_count) {
    if (UINT64_MAX - context->enumerated_variant_count < completion_count) {
        context->incomplete_branch_seen = 1u;
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    context->enumerated_variant_count += completion_count;
    if (context->enumerated_variant_count > context->max_count_variants) {
        return CLR_BUILDUP_ENUMERATION_TRUNCATED;
    }
    return CLR_BUILDUP_OK;
}

static void materialize_success_path(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpGeometryPath *path,
    uint16_t depth) {
    if (!context->capture_trace || path == 0) {
        return;
    }
    for (uint16_t index = 0u; index < depth; ++index) {
        const ClearraBuildUpGeometryPathStep *path_step = &path->steps[index];
        const ClearraBuildUpGeometryTracePayload *trace =
            path_step->edge->payload.trace;
        if (trace == 0) {
            context->incomplete_branch_seen = 1u;
            return;
        }
        context->current_trace_steps[index] = trace->trace_step;
        context->current_trace_steps[index].hold_branch_kind =
            path_step->branch_kind;
        context->current_trace_steps[index].used_hold = path_step->used_hold;
        context->current_trace_steps[index].incoming_piece =
            path_step->incoming_piece;
        context->current_trace_steps[index].held_piece_before =
            path_step->held_piece_before;
        context->current_trace_steps[index].hold_empty_before =
            path_step->hold_empty_before;
        context->current_kick_evidence[index] = trace->kick_evidence;
    }
}

static clr_buildup_status search_geometry_node(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpGeometryNode *node,
    ClearraBuildUpQueueHold queue_hold,
    uint16_t depth,
    uint8_t last_hold_branch_kind,
    ClearraBuildUpGeometryPath *path) {
    if (node->live == 0u) {
        return CLR_BUILDUP_OK;
    }
    uint64_t max_nodes = context->problem->packing.budget.max_nodes;
    if (max_nodes != 0u && context->expanded_state_count >= max_nodes) {
        context->incomplete_branch_seen = 1u;
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    context->expanded_state_count++;
    if (clr_execution_control_poll(&context->cancellation_poll_counter)) {
        context->incomplete_branch_seen = 1u;
        return CLR_BUILDUP_CANCELLED;
    }

    ClearraBuildUpState state = state_from_node(node, queue_hold, depth);
    state.last_hold_branch_kind = last_hold_branch_kind;
    if (node->remaining_operations == 0u) {
        if (node->accepting != 0u) {
            materialize_success_path(context, path, depth);
            return clearra_buildup_search_record_success(context, &state);
        }
        clearra_buildup_search_record_failure(
            context, CLR_BUILDUP_GOAL_NOT_SATISFIED, depth);
        return CLR_BUILDUP_OK;
    }

    uint64_t memoized_completion_count = 0u;
    if (depth != 0u && clearra_buildup_search_completion_memo_lookup(
            context,
            &state,
            node->remaining_operations,
            &memoized_completion_count)) {
        if (memoized_completion_count == 0u) {
            return CLR_BUILDUP_OK;
        }
        if (completion_memo_can_shortcut(context)) {
            return add_memoized_completions(context, memoized_completion_count);
        }
    }

    uint8_t eligible_piece_mask = 0u;
    for (const ClearraBuildUpGeometryEdge *edge = node->first_edge;
         edge != 0;
         edge = edge->next) {
        if (edge->child->live == 0u) {
            continue;
        }
        if (edge->piece < CLR_PIECE_I || edge->piece > CLR_PIECE_L) {
            return CLR_BUILDUP_INVALID_PROBLEM;
        }
        eligible_piece_mask = (uint8_t)(
            eligible_piece_mask |
            (uint8_t)(UINT8_C(1) << edge->piece));
    }
    if (eligible_piece_mask == 0u) {
        clearra_buildup_search_record_failure(
            context, CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE, depth);
        if (depth != 0u) {
            clearra_buildup_search_failed_memo_insert(
                context, &state, node->remaining_operations);
        }
        return CLR_BUILDUP_OK;
    }

    ClearraBuildUpHoldBranchTable hold_branches;
    clr_buildup_status status =
        clearra_buildup_queue_hold_enumerate_branch_mask_for_step(
            context->problem,
            &queue_hold,
            eligible_piece_mask,
            (node->remaining_operations &
             (uint16_t)(node->remaining_operations - 1u)) == 0u,
            &hold_branches);
    if (status != CLR_BUILDUP_OK) {
        clearra_buildup_search_record_failure(context, status, depth);
        if (clearra_buildup_branch_outcome_for_status(status) ==
            CLEARRA_BUILDUP_BRANCH_LOGICAL_REJECT) {
            if (depth != 0u) {
                clearra_buildup_search_failed_memo_insert(
                    context, &state, node->remaining_operations);
            }
            return CLR_BUILDUP_OK;
        }
        return status;
    }

    uint64_t variants_before = context->enumerated_variant_count;
    const ClearraBuildUpGeometryEdge *operation_group = node->first_edge;
    while (operation_group != 0) {
        uint16_t operation_index = operation_group->operation_index;
        const ClearraBuildUpGeometryEdge *next_operation_group =
            operation_group->next;
        while (next_operation_group != 0 &&
               next_operation_group->operation_index == operation_index) {
            next_operation_group = next_operation_group->next;
        }
        uint8_t piece = operation_group->piece;
        uint8_t branch_count = hold_branches.counts[piece];
        if (!context->preserve_hold_branches && branch_count > 1u) {
            branch_count = 1u;
        }
        ClearraBuildUpHoldBranch *branches =
            hold_branches.branches[piece];
        for (uint8_t branch_index = 0u;
             branch_index < branch_count;
             ++branch_index) {
            for (const ClearraBuildUpGeometryEdge *edge = operation_group;
                 edge != next_operation_group;
                 edge = edge->next) {
                if (edge->child->live == 0u) {
                    continue;
                }
                if (context->capture_trace && depth < CLR_BUILDUP_MAX_OPERATIONS) {
                    path->steps[depth] = (ClearraBuildUpGeometryPathStep){
                        .edge = edge,
                        .branch_kind = branches[branch_index].branch_kind,
                        .used_hold = branches[branch_index].used_hold,
                        .incoming_piece = branches[branch_index].incoming_piece,
                        .held_piece_before = branches[branch_index].held_piece_before,
                        .hold_empty_before = branches[branch_index].hold_empty_before,
                    };
                }
                status = search_geometry_node(
                    context,
                    edge->child,
                    branches[branch_index].state,
                    (uint16_t)(depth + 1u),
                    branches[branch_index].branch_kind,
                    path);
                if (status != CLR_BUILDUP_OK) {
                    return status;
                }
                if (context->stop_after_first_success &&
                    context->enumerated_variant_count > 0u) {
                    return CLR_BUILDUP_OK;
                }
            }
        }
        operation_group = next_operation_group;
    }

    uint64_t completion_count =
        context->enumerated_variant_count - variants_before;
    if (completion_count == 0u && depth != 0u) {
        clearra_buildup_search_failed_memo_insert(
            context, &state, node->remaining_operations);
    } else if (depth != 0u && completion_memo_can_shortcut(context)) {
        clearra_buildup_search_completion_memo_insert(
            context, &state, node->remaining_operations, completion_count);
    }
    return CLR_BUILDUP_OK;
}

clr_buildup_status clearra_buildup_search_geometry_dag(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpGeometryDag *dag,
    ClearraBuildUpQueueHold queue_hold) {
    if (context == 0 || !clearra_buildup_geometry_dag_is_available(dag) ||
        context->capture_trace != dag->capture_trace ||
        context->reachability_trace_mode != dag->reachability_trace_mode) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    ClearraBuildUpGeometryPath path = {0};
    return search_geometry_node(context, dag->root, queue_hold, 0u, 0u, &path);
}
