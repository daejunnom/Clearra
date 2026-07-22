#include "locked_reachability_internal.h"
#include "reachability_field.h"

static ClearraReachabilityStatus locked_reachability_search(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool allow_180,
    const ClearraReachabilityKickTable *kick_table,
    bool final_rotation_only,
    int16_t required_final_kick_index,
    ClearraReachabilityFrontier *frontier,
    ClearraReachabilityReport *out_report) {
    if (!clearra_board64_layout_is_valid(layout) || frontier == 0 ||
        out_report == 0) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    clearra_reachability_report_clear(out_report);

    ClearraReachabilityState initial = {rotation, x, y};
    bool placeable = false;
    ClearraReachabilityStatus status =
        clearra_locked_reachability_is_placeable_state(
            layout, board, piece, initial, &placeable);
    if (status != CLEARRA_REACHABILITY_OK) {
        return status;
    }

    if (!placeable) {
        return CLEARRA_REACHABILITY_COLLISION;
    }
    bool grounded = false;
    status = clearra_locked_reachability_is_grounded_state(
        layout, board, piece, initial, &grounded);
    if (status != CLEARRA_REACHABILITY_OK || !grounded) {
        return status;
    }

    uint16_t initial_index = UINT16_MAX;
    bool initial_inserted = false;
    status = clearra_locked_reachability_push_state(
        frontier,
        initial,
        -1,
        CLEARRA_ROTATION_TRANSITION_NONE,
        false,
        &initial_index,
        &initial_inserted);
    if (status != CLEARRA_REACHABILITY_OK) {
        return status;
    }

    if (final_rotation_only) {
        frontier->stack_count = 0u;
        uint8_t before_rotations[3] = {
            (uint8_t)((initial.rotation + 3u) % 4u),
            (uint8_t)((initial.rotation + 1u) % 4u),
            (uint8_t)((initial.rotation + 2u) % 4u),
        };
        for (uint8_t index = 0; index < 3; index++) {
            status = clearra_locked_reachability_push_kick_predecessor(
                layout, board, piece, initial, before_rotations[index],
                allow_180, kick_table, frontier, (int16_t)initial_index,
                required_final_kick_index);
            if (status != CLEARRA_REACHABILITY_OK) {
                return status;
            }
        }
    }

    uint16_t current_index = UINT16_MAX;
    while (clearra_locked_reachability_pop_state(frontier, &current_index)) {
        ClearraReachabilityState state = frontier->nodes[current_index].state;

        if (state.y >= (int8_t)layout.height) {
            out_report->reachable = true;
            out_report->visited_states = frontier->processed_count;
            if (frontier->capture_trace != 0u) {
                clearra_locked_reachability_record_debug_path(
                    frontier->nodes, (int16_t)current_index, out_report);
            }
            return CLEARRA_REACHABILITY_OK;
        }

        bool harddrop = false;
        status = clearra_harddrop_reachability_is_reachable(
            layout, board, piece, state.rotation, state.x, state.y, &harddrop);
        if (status != CLEARRA_REACHABILITY_OK) {
            return status;
        }
        if (harddrop) {
            out_report->reachable = true;
            out_report->visited_states = frontier->processed_count;
            if (frontier->capture_trace != 0u) {
                clearra_locked_reachability_record_debug_path(
                    frontier->nodes, (int16_t)current_index, out_report);
            }
            return CLEARRA_REACHABILITY_OK;
        }

        ClearraReachabilityState predecessor = state;
        predecessor.y = (int8_t)(state.y + 1);
        status = clearra_locked_reachability_push_if_placeable(
            layout, board, piece, predecessor, frontier,
            (int16_t)current_index);
        if (status != CLEARRA_REACHABILITY_OK) {
            return status;
        }
        predecessor = state;
        predecessor.x = (int8_t)(state.x - 1);
        status = clearra_locked_reachability_push_if_placeable(
            layout, board, piece, predecessor, frontier,
            (int16_t)current_index);
        if (status != CLEARRA_REACHABILITY_OK) {
            return status;
        }
        predecessor = state;
        predecessor.x = (int8_t)(state.x + 1);
        status = clearra_locked_reachability_push_if_placeable(
            layout, board, piece, predecessor, frontier,
            (int16_t)current_index);
        if (status != CLEARRA_REACHABILITY_OK) {
            return status;
        }

        uint8_t before_rotations[3] = {
            (uint8_t)((state.rotation + 3u) % 4u),
            (uint8_t)((state.rotation + 1u) % 4u),
            (uint8_t)((state.rotation + 2u) % 4u),
        };
        for (uint8_t index = 0; index < 3; index++) {
            status = clearra_locked_reachability_push_kick_predecessor(
                layout, board, piece, state, before_rotations[index], allow_180,
                kick_table, frontier, (int16_t)current_index, -1);
            if (status != CLEARRA_REACHABILITY_OK) {
                return status;
            }
        }
    }

    out_report->visited_states = frontier->processed_count;
    return CLEARRA_REACHABILITY_OK;
}

ClearraReachabilityStatus clearra_locked_reachability_is_reachable(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool allow_180,
    const ClearraReachabilityKickTable *kick_table,
    ClearraReachabilityReport *out_report) {
    ClearraReachabilityFrontier frontier;
    clearra_locked_reachability_frontier_init(&frontier);
    frontier.capture_trace = 1u;
    return clearra_locked_reachability_is_reachable_with_frontier(
        layout, board, piece, rotation, x, y, allow_180, kick_table,
        CLEARRA_REACHABILITY_TRACE_FIRST_LEGAL, &frontier, out_report);
}

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
    ClearraReachabilityReport *out_report) {
    if (frontier == 0) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    if (trace_mode > CLEARRA_REACHABILITY_TRACE_HIGHEST_T_SPIN) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    frontier->capture_trace =
        trace_mode == CLEARRA_REACHABILITY_TRACE_NONE ? 0u : 1u;
    clearra_locked_reachability_frontier_reset(frontier);
    if (trace_mode == CLEARRA_REACHABILITY_TRACE_HIGHEST_T_SPIN &&
        piece == CLEARRA_CANDIDATE_PIECE_T) {
        ClearraReachabilityStatus status = locked_reachability_search(
            layout, board, piece, rotation, x, y, allow_180, kick_table,
            true, 4, frontier, out_report);
        if (status != CLEARRA_REACHABILITY_OK || out_report->reachable) {
            return status;
        }
        clearra_locked_reachability_frontier_reset(frontier);
        status = locked_reachability_search(
            layout, board, piece, rotation, x, y, allow_180, kick_table,
            true, -1, frontier, out_report);
        if (status != CLEARRA_REACHABILITY_OK || out_report->reachable) {
            return status;
        }
        clearra_locked_reachability_frontier_reset(frontier);
    }
    return locked_reachability_search(
        layout, board, piece, rotation, x, y, allow_180, kick_table,
        false, -1, frontier, out_report);
}

#include "locked_reachability_internal.h"

void clearra_locked_reachability_record_debug_path(
    const ClearraReachabilityNode *visited,
    int16_t success_index,
    ClearraReachabilityReport *report) {
    ClearraReachabilityDebugStep reverse[CLEARRA_REACHABILITY_MAX_DEBUG_STEPS];
    uint8_t reverse_count = 0;
    report->debug_step_count = 0;
    report->used_kick = false;
    report->used_180 = false;
    report->has_rotation_evidence = false;
    report->first_success_confirmed = false;
    int16_t index = success_index;
    while (index >= 0 && reverse_count < CLEARRA_REACHABILITY_MAX_DEBUG_STEPS) {
        ClearraReachabilityNode node = visited[index];
        reverse[reverse_count++] = (ClearraReachabilityDebugStep){
            node.state.rotation,
            node.state.x,
            node.state.y,
            node.transition_kind,
        };
        report->used_kick = report->used_kick || node.used_kick;
        report->used_180 =
            report->used_180 ||
            node.transition_kind == CLEARRA_ROTATION_TRANSITION_HALF_TURN;
        bool rotation_lands_at_lock =
            node.has_rotation_evidence && node.parent_index >= 0 &&
            visited[node.parent_index].parent_index < 0;
        if (rotation_lands_at_lock) {
            report->has_rotation_evidence = true;
            report->first_success_confirmed =
                node.first_success_confirmed;
            report->rotation_from = node.state.rotation;
            report->rotation_to = node.rotation_result.rotation;
            report->rotation_request = node.transition_kind;
            report->kick_index = node.kick_index;
            report->kick_dx = node.kick_dx;
            report->kick_dy = node.kick_dy;
            report->predecessor_x = node.state.x;
            report->predecessor_y = node.state.y;
            report->result_x = node.rotation_result.x;
            report->result_y = node.rotation_result.y;
        }
        index = node.parent_index;
    }
    report->path_complete = index < 0;
    for (uint8_t offset = 0; offset < reverse_count; offset++) {
        report->debug_steps[offset] = reverse[reverse_count - offset - 1u];
    }
    report->debug_step_count = reverse_count;
}

#include "locked_reachability_internal.h"

ClearraReachabilityStatus clearra_locked_reachability_push_if_placeable(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraReachabilityState state,
    ClearraReachabilityFrontier *frontier,
    int16_t parent_index) {
    bool placeable = false;
    ClearraReachabilityStatus status =
        clearra_locked_reachability_is_placeable_state(
            layout, board, piece, state, &placeable);
    if (status != CLEARRA_REACHABILITY_OK || !placeable) {
        return status;
    }
    uint16_t node_index = UINT16_MAX;
    bool inserted = false;
    return clearra_locked_reachability_push_state(
        frontier, state, parent_index, CLEARRA_ROTATION_TRANSITION_NONE,
        false, &node_index, &inserted);
}

#include "locked_reachability_internal.h"

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
    int16_t required_kick_index) {
    ClearraRotationTransitionKind transition = CLEARRA_ROTATION_TRANSITION_NONE;
    if (clearra_candidate_transition_kind(
            before_rotation, after.rotation, &transition) != CLEARRA_CANDIDATE_OK) {
        return CLEARRA_REACHABILITY_INVALID_OPERATION;
    }
    if (transition == CLEARRA_ROTATION_TRANSITION_NONE ||
        (transition == CLEARRA_ROTATION_TRANSITION_HALF_TURN && !allow_180)) {
        return CLEARRA_REACHABILITY_OK;
    }

    const ClearraKickOffset *offsets = 0;
    uint8_t offset_count = 0;
    ClearraReachabilityStatus status =
        clearra_reachability_kick_offsets_for_transition(
            kick_table, before_rotation, after.rotation, &offsets, &offset_count);
    if (status == CLEARRA_REACHABILITY_UNREACHABLE) {
        return CLEARRA_REACHABILITY_OK;
    }
    if (status != CLEARRA_REACHABILITY_OK) {
        return status;
    }

    for (uint8_t index = 0; index < offset_count; index++) {
        if (required_kick_index >= 0 &&
            index != (uint8_t)required_kick_index) {
            continue;
        }
        int8_t normalized_dx = 0;
        int8_t normalized_dy = 0;
        ClearraCandidateStatus delta_status =
            clearra_candidate_normalized_kick_delta(
                piece, before_rotation, after.rotation, offsets[index].dx,
                offsets[index].dy, &normalized_dx, &normalized_dy);
        if (delta_status != CLEARRA_CANDIDATE_OK) {
            return CLEARRA_REACHABILITY_INVALID_OPERATION;
        }
        ClearraReachabilityState before = {
            before_rotation,
            (int8_t)(after.x - normalized_dx),
            (int8_t)(after.y - normalized_dy),
        };
        bool placeable = false;
        status = clearra_locked_reachability_is_placeable_state(
            layout, board, piece, before, &placeable);
        if (status != CLEARRA_REACHABILITY_OK) {
            return status;
        }
        if (!placeable) {
            continue;
        }
        ClearraCandidateOperation operation;
        status = clearra_reachability_field_first_success_kick(
            layout, board, piece, before_rotation, after.rotation, before.x, before.y,
            kick_table, &operation);
        if (status == CLEARRA_REACHABILITY_UNREACHABLE) {
            continue;
        }
        if (status != CLEARRA_REACHABILITY_OK) {
            return status;
        }
        if (operation.x != after.x || operation.y != after.y ||
            operation.rotation != after.rotation ||
            operation.kick_dx != offsets[index].dx ||
            operation.kick_dy != offsets[index].dy) {
            continue;
        }
        uint16_t pushed_index = UINT16_MAX;
        bool inserted = false;
        status = clearra_locked_reachability_push_state(
            frontier, before, parent_index, (uint8_t)transition,
            operation.kick_dx != 0 || operation.kick_dy != 0,
            &pushed_index, &inserted);
        if (status != CLEARRA_REACHABILITY_OK) {
            return status;
        }
        if (inserted && frontier->capture_trace != 0u) {
            ClearraReachabilityNode *pushed = &frontier->nodes[pushed_index];
            pushed->has_rotation_evidence = true;
            pushed->first_success_confirmed = true;
            pushed->kick_index = operation.kick_index;
            pushed->kick_dx = operation.kick_dx;
            pushed->kick_dy = operation.kick_dy;
            pushed->rotation_result = after;
        }
    }
    return CLEARRA_REACHABILITY_OK;
}

#include "locked_reachability_internal.h"
ClearraReachabilityStatus clearra_locked_reachability_is_placeable_state(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraReachabilityState state,
    bool *out_placeable) {
    return clearra_reachability_field_is_placeable(
        layout, board, piece, state.rotation, state.x, state.y, out_placeable);
}ClearraReachabilityStatus clearra_locked_reachability_is_grounded_state(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraReachabilityState state,
    bool *out_grounded) {
    return clearra_reachability_field_is_grounded(
        layout, board, piece, state.rotation, state.x, state.y, out_grounded);
}

#include "reachability.h"
static ClearraReachabilityStatus reachability_status_from_rule_status(
    ClearraRuleStatus status) {
    if (status == CLEARRA_RULE_OK) {
        return CLEARRA_REACHABILITY_OK;
    }
    if (status == CLEARRA_RULE_INVALID_ARGUMENT) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    return CLEARRA_REACHABILITY_INVALID_OPERATION;
}

ClearraReachabilityStatus clearra_reachability_compile_rule(
    const clr_rule_profile_descriptor *rule,
    ClearraCompactRuleProfile *out_profile) {
    if (rule == 0 || out_profile == 0) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    return reachability_status_from_rule_status(
        clearra_rule_profile_from_descriptor(rule, out_profile));
}

ClearraReachabilityStatus clearra_reachability_kick_table_from_compiled_rule(
    const ClearraCompactRuleProfile *profile,
    uint8_t piece,
    ClearraReachabilityKickTable *out_table) {
    if (profile == 0 || out_table == 0 || piece > CLR_PIECE_L) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    *out_table = (ClearraReachabilityKickTable){0};
    out_table->compact_table = &profile->kick_table;
    out_table->piece = piece;
    return CLEARRA_REACHABILITY_OK;
}

ClearraReachabilityStatus clearra_reachability_kick_table_from_rule(
    const clr_rule_profile_descriptor *rule,
    uint8_t piece,
    ClearraReachabilityKickTable *out_table) {
    if (rule == 0 || out_table == 0) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }

    *out_table = (ClearraReachabilityKickTable){0};

    ClearraCompactRuleProfile profile = {0};
    ClearraReachabilityStatus reachability_status =
        clearra_reachability_compile_rule(rule, &profile);
    if (reachability_status != CLEARRA_REACHABILITY_OK) {
        return reachability_status;
    }

    out_table->owned_compact_table = profile.kick_table;
    out_table->compact_table = &out_table->owned_compact_table;
    out_table->piece = piece;
    return CLEARRA_REACHABILITY_OK;
}
