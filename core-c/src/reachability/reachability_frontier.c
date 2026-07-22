#include "locked_reachability_internal.h"

#include <string.h>

static uint32_t state_key(ClearraReachabilityState state) {
    return UINT32_C(0x01000000) |
           ((uint32_t)state.rotation << 16u) |
           ((uint32_t)(uint8_t)state.x << 8u) |
           (uint32_t)(uint8_t)state.y;
}

static uint32_t state_bucket(uint32_t key) {
    return (key * UINT32_C(2654435761)) &
           (CLEARRA_REACHABILITY_STATE_TABLE_CAPACITY - 1u);
}

static bool insert_state_key(
    ClearraReachabilityFrontier *frontier,
    uint32_t key) {
    uint32_t mask = CLEARRA_REACHABILITY_STATE_TABLE_CAPACITY - 1u;
    uint32_t bucket = state_bucket(key);
    for (uint32_t probe = 0u;
         probe < CLEARRA_REACHABILITY_STATE_TABLE_CAPACITY;
         ++probe) {
        uint32_t slot_index = (bucket + probe) & mask;
        uint16_t *slot_generation =
            &frontier->state_generations[slot_index];
        uint32_t *slot = &frontier->state_keys[slot_index];
        if (*slot_generation != frontier->generation) {
            *slot_generation = frontier->generation;
            *slot = key;
            return true;
        }
        if (*slot == key) {
            return false;
        }
    }
    return false;
}

void clearra_locked_reachability_frontier_init(
    ClearraReachabilityFrontier *frontier) {
    if (frontier != 0) {
        memset(
            frontier->state_generations,
            0,
            sizeof(frontier->state_generations));
        frontier->generation = 0u;
        frontier->capture_trace = 0u;
        clearra_locked_reachability_frontier_reset(frontier);
    }
}

void clearra_locked_reachability_frontier_reset(
    ClearraReachabilityFrontier *frontier) {
    if (frontier != 0) {
        if (frontier->generation == UINT16_MAX) {
            memset(
                frontier->state_generations,
                0,
                sizeof(frontier->state_generations));
            frontier->generation = 1u;
        } else {
            frontier->generation++;
        }
        frontier->node_count = 0u;
        frontier->stack_count = 0u;
        frontier->processed_count = 0u;
    }
}

ClearraReachabilityStatus clearra_locked_reachability_push_state(
    ClearraReachabilityFrontier *frontier,
    ClearraReachabilityState state,
    int16_t parent_index,
    uint8_t transition_kind,
    bool used_kick,
    uint16_t *out_node_index,
    bool *out_inserted) {
    if (frontier == 0 || out_node_index == 0 || out_inserted == 0) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    *out_node_index = UINT16_MAX;
    *out_inserted = false;

    uint32_t key = state_key(state);
    if (!insert_state_key(frontier, key)) {
        return CLEARRA_REACHABILITY_OK;
    }
    if (frontier->node_count >= CLEARRA_REACHABILITY_MAX_GRAPH_STATES ||
        frontier->stack_count >= CLEARRA_REACHABILITY_MAX_GRAPH_STATES) {
        return CLEARRA_REACHABILITY_CAPACITY_EXCEEDED;
    }

    uint16_t node_index = frontier->node_count++;
    ClearraReachabilityNode *node = &frontier->nodes[node_index];
    node->state = state;
    if (frontier->capture_trace != 0u) {
        node->parent_index = parent_index;
        node->transition_kind = transition_kind;
        node->used_kick = used_kick;
        node->has_rotation_evidence = false;
        node->first_success_confirmed = false;
        node->kick_index = 0u;
        node->kick_dx = 0;
        node->kick_dy = 0;
        node->rotation_result = (ClearraReachabilityState){0};
    }
    frontier->stack_indices[frontier->stack_count++] = node_index;
    *out_node_index = node_index;
    *out_inserted = true;
    return CLEARRA_REACHABILITY_OK;
}

bool clearra_locked_reachability_pop_state(
    ClearraReachabilityFrontier *frontier,
    uint16_t *out_node_index) {
    if (frontier == 0 || out_node_index == 0 || frontier->stack_count == 0u) {
        return false;
    }
    *out_node_index = frontier->stack_indices[--frontier->stack_count];
    frontier->processed_count++;
    return true;
}
