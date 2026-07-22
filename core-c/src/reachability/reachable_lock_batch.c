#include "reachable_lock_batch.h"

#include "reachability_field.h"

#include <limits.h>

void clearra_reachable_lock_set_clear(ClearraReachableLockSet *set) {
    if (set != 0) {
        *set = (ClearraReachableLockSet){0};
    }
}

bool clearra_reachable_lock_set_contains(
    const ClearraReachableLockSet *set,
    ClearraBoard64Layout layout,
    uint8_t rotation,
    int8_t x,
    int8_t y) {
    if (set == 0 || set->complete == 0u ||
        !clearra_board64_layout_is_valid(layout) ||
        rotation >= CLEARRA_ROTATION_STATE_COUNT || x < 0 || y < 0 ||
        x >= (int8_t)layout.width || y >= (int8_t)layout.height) {
        return false;
    }
    uint8_t anchor_index =
        (uint8_t)((uint8_t)y * layout.width + (uint8_t)x);
    return (set->anchor_bits[rotation] &
            (UINT64_C(1) << anchor_index)) != 0u;
}

static ClearraCandidateStatus candidate_status(
    ClearraReachabilityStatus status) {
    switch (status) {
        case CLEARRA_REACHABILITY_OK:
            return CLEARRA_CANDIDATE_OK;
        case CLEARRA_REACHABILITY_COLLISION:
            return CLEARRA_CANDIDATE_COLLISION;
        case CLEARRA_REACHABILITY_UNREACHABLE:
            return CLEARRA_CANDIDATE_UNREACHABLE;
        case CLEARRA_REACHABILITY_CAPACITY_EXCEEDED:
            return CLEARRA_CANDIDATE_CAPACITY_EXCEEDED;
        case CLEARRA_REACHABILITY_INVALID_OPERATION:
            return CLEARRA_CANDIDATE_INVALID_ROTATION;
        default:
            return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
}

static ClearraCandidateStatus push_state_if_placeable(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraReachabilityState state,
    int8_t source_ceiling,
    ClearraReachabilityFrontier *frontier) {
    /* The reverse reference treats every placeable state at or above the
     * search sky as a source. Exploring kick chains above that boundary adds
     * no reachability information and would make the finite state graph
     * unbounded. */
    if (state.y > source_ceiling) {
        return CLEARRA_CANDIDATE_OK;
    }
    bool placeable = false;
    ClearraReachabilityStatus status =
        clearra_reachability_field_is_placeable(
            layout,
            board,
            piece,
            state.rotation,
            state.x,
            state.y,
            &placeable);
    if (status != CLEARRA_REACHABILITY_OK || !placeable) {
        return candidate_status(status);
    }
    uint16_t node_index = UINT16_MAX;
    bool inserted = false;
    return candidate_status(clearra_locked_reachability_push_state(
        frontier,
        state,
        -1,
        CLEARRA_ROTATION_TRANSITION_NONE,
        false,
        &node_index,
        &inserted));
}

static ClearraCandidateStatus source_ceiling_for_rule(
    ClearraBoard64Layout layout,
    uint8_t piece,
    const ClearraCompactRuleProfile *rule,
    bool allow_180,
    int8_t *out_ceiling) {
    if (rule == 0 || out_ceiling == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    int16_t maximum_downward_delta = 0;
    const ClearraCompactKickTable *table = &rule->kick_table;
    for (uint16_t transition_index = 0u;
         transition_index < table->transition_count;
         ++transition_index) {
        const ClearraCompactKickTransition *transition =
            &table->transitions[transition_index];
        if (transition->piece != piece ||
            (!allow_180 && clearra_rule_transition_is_180(
                               transition->from_rotation,
                               transition->to_rotation))) {
            continue;
        }
        for (uint8_t offset_index = 0u;
             offset_index < transition->sequence.count;
             ++offset_index) {
            int8_t normalized_dx = 0;
            int8_t normalized_dy = 0;
            ClearraCandidateStatus status =
                clearra_candidate_normalized_kick_delta(
                    piece,
                    transition->from_rotation,
                    transition->to_rotation,
                    transition->sequence.offsets[offset_index].dx,
                    transition->sequence.offsets[offset_index].dy,
                    &normalized_dx,
                    &normalized_dy);
            if (status != CLEARRA_CANDIDATE_OK) {
                return status;
            }
            if (normalized_dy < 0 &&
                -(int16_t)normalized_dy > maximum_downward_delta) {
                maximum_downward_delta = -(int16_t)normalized_dy;
            }
        }
    }
    int16_t ceiling = (int16_t)layout.height + maximum_downward_delta;
    if (ceiling > INT8_MAX) {
        return CLEARRA_CANDIDATE_CAPACITY_EXCEEDED;
    }
    *out_ceiling = (int8_t)ceiling;
    return CLEARRA_CANDIDATE_OK;
}

static ClearraCandidateStatus seed_sky_states(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    int8_t source_ceiling,
    ClearraReachabilityFrontier *frontier) {
    for (uint8_t rotation = 0u;
         rotation < CLEARRA_ROTATION_STATE_COUNT;
         ++rotation) {
        ClearraOperation operation;
        if (clearra_operation_from_shape(piece, rotation, &operation) !=
            CLEARRA_OPERATION_OK) {
            return CLEARRA_CANDIDATE_INVALID_ROTATION;
        }
        if (operation.bounds.width > layout.width) {
            continue;
        }
        uint8_t max_x = (uint8_t)(layout.width - operation.bounds.width);
        for (int16_t y = layout.height; y <= source_ceiling; ++y) {
            for (uint8_t x = 0u; x <= max_x; ++x) {
                ClearraCandidateStatus status = push_state_if_placeable(
                    layout,
                    board,
                    piece,
                    (ClearraReachabilityState){
                        rotation, (int8_t)x, (int8_t)y},
                    source_ceiling,
                    frontier);
                if (status != CLEARRA_CANDIDATE_OK) {
                    return status;
                }
            }
        }
    }
    return CLEARRA_CANDIDATE_OK;
}

static ClearraCandidateStatus push_rotation_result(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraReachabilityState state,
    uint8_t to_rotation,
    const ClearraReachabilityKickTable *kick_table,
    int8_t source_ceiling,
    ClearraReachabilityFrontier *frontier) {
    ClearraCandidateOperation operation;
    ClearraReachabilityStatus status =
        clearra_reachability_field_first_success_kick(
            layout,
            board,
            piece,
            state.rotation,
            to_rotation,
            state.x,
            state.y,
            kick_table,
            &operation);
    if (status == CLEARRA_REACHABILITY_UNREACHABLE) {
        return CLEARRA_CANDIDATE_OK;
    }
    if (status != CLEARRA_REACHABILITY_OK) {
        return candidate_status(status);
    }
    if (operation.y > source_ceiling) {
        return CLEARRA_CANDIDATE_OK;
    }
    uint16_t node_index = UINT16_MAX;
    bool inserted = false;
    return candidate_status(clearra_locked_reachability_push_state(
        frontier,
        (ClearraReachabilityState){
            operation.rotation,
            operation.x,
            operation.y,
        },
        -1,
        operation.transition_kind,
        true,
        &node_index,
        &inserted));
}

static ClearraCandidateStatus append_grounded_lock(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraReachabilityState state,
    ClearraReachableLockSet *out_locks) {
    if (state.y < 0 || state.y >= (int8_t)layout.height) {
        return CLEARRA_CANDIDATE_OK;
    }
    bool grounded = false;
    ClearraReachabilityStatus status =
        clearra_reachability_field_is_grounded(
            layout,
            board,
            piece,
            state.rotation,
            state.x,
            state.y,
            &grounded);
    if (status != CLEARRA_REACHABILITY_OK || !grounded) {
        return candidate_status(status);
    }
    if (state.x < 0 || state.x >= (int8_t)layout.width) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    uint8_t anchor_index =
        (uint8_t)((uint8_t)state.y * layout.width + (uint8_t)state.x);
    out_locks->anchor_bits[state.rotation] |= UINT64_C(1) << anchor_index;
    return CLEARRA_CANDIDATE_OK;
}

ClearraCandidateStatus clearra_reachable_lock_batch_generate(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    const ClearraCompactRuleProfile *rule,
    uint8_t mode,
    ClearraReachabilityFrontier *frontier,
    ClearraReachableLockSet *out_locks) {
    if (!clearra_board64_layout_is_valid(layout) ||
        (board & ~layout.all_cells_mask) != 0u ||
        !clearra_piece_is_standard_tetromino(piece) || rule == 0 ||
        frontier == 0 || out_locks == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    if (mode == CLEARRA_CANDIDATE_MODE_HARDDROP ||
        rule->rule_profile_id == CLR_RULE_NO_KICK) {
        ClearraCandidateList harddrop_locks;
        ClearraCandidateStatus status = clearra_harddrop_candidates_generate(
            layout, board, piece, &harddrop_locks);
        if (status != CLEARRA_CANDIDATE_OK) {
            return status;
        }
        clearra_reachable_lock_set_clear(out_locks);
        for (uint16_t index = 0u; index < harddrop_locks.count; ++index) {
            const ClearraCandidateOperation *operation =
                &harddrop_locks.operations[index];
            uint8_t anchor_index = (uint8_t)(
                (uint8_t)operation->y * layout.width +
                (uint8_t)operation->x);
            out_locks->anchor_bits[operation->rotation] |=
                UINT64_C(1) << anchor_index;
        }
        out_locks->complete = 1u;
        return CLEARRA_CANDIDATE_OK;
    }
    bool allow_180 = mode == CLEARRA_CANDIDATE_MODE_LOCKED_180;
    if (mode != CLEARRA_CANDIDATE_MODE_LOCKED && !allow_180) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    if (allow_180 && !rule->supports_180) {
        return CLEARRA_CANDIDATE_UNREACHABLE;
    }

    ClearraReachabilityKickTable kick_table;
    ClearraReachabilityStatus kick_status =
        clearra_reachability_kick_table_from_compiled_rule(
            rule, piece, &kick_table);
    if (kick_status != CLEARRA_REACHABILITY_OK) {
        return candidate_status(kick_status);
    }

    int8_t source_ceiling = 0;
    ClearraCandidateStatus result = source_ceiling_for_rule(
        layout, piece, rule, allow_180, &source_ceiling);
    if (result != CLEARRA_CANDIDATE_OK) {
        return result;
    }
    clearra_reachable_lock_set_clear(out_locks);
    frontier->capture_trace = 0u;
    clearra_locked_reachability_frontier_reset(frontier);
    result = seed_sky_states(
        layout, board, piece, source_ceiling, frontier);
    if (result != CLEARRA_CANDIDATE_OK) {
        return result;
    }

    uint16_t node_index = UINT16_MAX;
    while (clearra_locked_reachability_pop_state(frontier, &node_index)) {
        ClearraReachabilityState state = frontier->nodes[node_index].state;
        result = append_grounded_lock(
            layout, board, piece, state, out_locks);
        if (result != CLEARRA_CANDIDATE_OK) {
            return result;
        }

        if (state.y > 0) {
            ClearraReachabilityState down = state;
            down.y--;
            result = push_state_if_placeable(
                layout, board, piece, down, source_ceiling, frontier);
            if (result != CLEARRA_CANDIDATE_OK) {
                return result;
            }
        }
        ClearraReachabilityState left = state;
        left.x--;
        result = push_state_if_placeable(
            layout, board, piece, left, source_ceiling, frontier);
        if (result != CLEARRA_CANDIDATE_OK) {
            return result;
        }
        ClearraReachabilityState right = state;
        right.x++;
        result = push_state_if_placeable(
            layout, board, piece, right, source_ceiling, frontier);
        if (result != CLEARRA_CANDIDATE_OK) {
            return result;
        }

        uint8_t rotations[3] = {
            (uint8_t)((state.rotation + 1u) % 4u),
            (uint8_t)((state.rotation + 3u) % 4u),
            (uint8_t)((state.rotation + 2u) % 4u),
        };
        uint8_t rotation_count = allow_180 ? 3u : 2u;
        for (uint8_t index = 0u; index < rotation_count; ++index) {
            result = push_rotation_result(
                layout,
                board,
                piece,
                state,
                rotations[index],
                &kick_table,
                source_ceiling,
                frontier);
            if (result != CLEARRA_CANDIDATE_OK) {
                return result;
            }
        }
    }
    out_locks->visited_state_count = frontier->processed_count;
    out_locks->complete = 1u;
    return CLEARRA_CANDIDATE_OK;
}
