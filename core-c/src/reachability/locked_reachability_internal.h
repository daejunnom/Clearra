#ifndef CLEARRA_LOCKED_REACHABILITY_INTERNAL_H
#define CLEARRA_LOCKED_REACHABILITY_INTERNAL_H

#include "reachability.h"
typedef struct ClearraReachabilityState {
    uint8_t rotation;
    int8_t x;
    int8_t y;
} ClearraReachabilityState;typedef struct ClearraReachabilityNode {
    ClearraReachabilityState state;
    int16_t parent_index;
    uint8_t transition_kind;
    bool used_kick;
    bool has_rotation_evidence;
    bool first_success_confirmed;
    uint8_t kick_index;
    int8_t kick_dx;
    int8_t kick_dy;
    ClearraReachabilityState rotation_result;
} ClearraReachabilityNode;

#define CLEARRA_REACHABILITY_STATE_TABLE_CAPACITY 1024u

_Static_assert(
    (CLEARRA_REACHABILITY_STATE_TABLE_CAPACITY &
     (CLEARRA_REACHABILITY_STATE_TABLE_CAPACITY - 1u)) == 0u,
    "reachability state table capacity must remain a power of two");

typedef struct ClearraReachabilityFrontier {
    uint16_t node_count;
    uint16_t stack_count;
    uint16_t processed_count;
    uint16_t generation;
    uint8_t capture_trace;
    uint8_t reserved[3];
    uint32_t state_keys[CLEARRA_REACHABILITY_STATE_TABLE_CAPACITY];
    uint16_t state_generations[CLEARRA_REACHABILITY_STATE_TABLE_CAPACITY];
    uint16_t stack_indices[CLEARRA_REACHABILITY_MAX_GRAPH_STATES];
    ClearraReachabilityNode nodes[CLEARRA_REACHABILITY_MAX_GRAPH_STATES];
} ClearraReachabilityFrontier;

void clearra_locked_reachability_frontier_init(
    ClearraReachabilityFrontier *frontier);
void clearra_locked_reachability_frontier_reset(
    ClearraReachabilityFrontier *frontier);
ClearraReachabilityStatus clearra_locked_reachability_push_state(
    ClearraReachabilityFrontier *frontier,
    ClearraReachabilityState state,
    int16_t parent_index,
    uint8_t transition_kind,
    bool used_kick,
    uint16_t *out_node_index,
    bool *out_inserted);
bool clearra_locked_reachability_pop_state(
    ClearraReachabilityFrontier *frontier,
    uint16_t *out_node_index);
ClearraReachabilityStatus clearra_locked_reachability_is_placeable_state(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraReachabilityState state,
    bool *out_placeable);
ClearraReachabilityStatus clearra_locked_reachability_is_grounded_state(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraReachabilityState state,
    bool *out_grounded);
ClearraReachabilityStatus clearra_locked_reachability_push_if_placeable(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraReachabilityState state,
    ClearraReachabilityFrontier *frontier,
    int16_t parent_index);
void clearra_locked_reachability_record_debug_path(
    const ClearraReachabilityNode *visited,
    int16_t success_index,
    ClearraReachabilityReport *report);
ClearraReachabilityStatus clearra_locked_reachability_push_kick_predecessor(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraReachabilityState after,
    uint8_t before_rotation,
    bool allow_180,
    const ClearraReachabilityKickTable *kick_table,
    ClearraReachabilityFrontier *frontier,
    int16_t parent_index,
    int16_t required_kick_index);
ClearraReachabilityStatus
clearra_locked_reachability_is_reachable_with_frontier(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool allow_180,
    const ClearraReachabilityKickTable *kick_table,
    uint8_t trace_mode,
    ClearraReachabilityFrontier *frontier,
    ClearraReachabilityReport *out_report);
#endif
