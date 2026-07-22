#include "reachability.h"
#include "locked_reachability_internal.h"

#include <stddef.h>
#include <string.h>
bool clearra_reachability_mode_supports_180(uint8_t mode) {
    return mode == CLEARRA_REACHABILITY_MODE_LOCKED_180 ||
           mode == CLEARRA_REACHABILITY_MODE_KICK_AWARE;
}bool clearra_reachability_mode_uses_kicks(uint8_t mode) {
    return mode == CLEARRA_REACHABILITY_MODE_LOCKED ||
           mode == CLEARRA_REACHABILITY_MODE_LOCKED_180 ||
           mode == CLEARRA_REACHABILITY_MODE_KICK_AWARE;
}ClearraReachabilityPolicy clearra_reachability_policy_for_mode(uint8_t mode) {
    if (mode == CLEARRA_REACHABILITY_MODE_HARDDROP) {
        return CLEARRA_REACHABILITY_POLICY_HARDDROP_ONLY;
    }
    if (mode == CLEARRA_REACHABILITY_MODE_LOCKED) {
        return CLEARRA_REACHABILITY_POLICY_LOCKED_REVERSE_GRAPH;
    }
    if (mode == CLEARRA_REACHABILITY_MODE_LOCKED_180 ||
        mode == CLEARRA_REACHABILITY_MODE_KICK_AWARE) {
        return CLEARRA_REACHABILITY_POLICY_LOCKED_180_REVERSE_GRAPH;
    }
    return CLEARRA_REACHABILITY_POLICY_INVALID;
}void clearra_reachability_report_clear(ClearraReachabilityReport *report) {
    if (report != 0) {
        memset(report, 0, offsetof(ClearraReachabilityReport, debug_steps));
    }
}static ClearraReachabilityStatus reachability_check_internal(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint8_t mode,
    const ClearraReachabilityKickTable *kick_table,
    uint8_t trace_mode,
    ClearraReachabilityFrontier *frontier,
    ClearraReachabilityReport *out_report) {
    if (out_report == 0) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    clearra_reachability_report_clear(out_report);

    ClearraReachabilityPolicy policy = clearra_reachability_policy_for_mode(mode);
    if (policy == CLEARRA_REACHABILITY_POLICY_HARDDROP_ONLY) {
        bool reachable = false;
        ClearraReachabilityStatus status = clearra_harddrop_reachability_is_reachable(
            layout, board, piece, rotation, x, y, &reachable);
        if (status != CLEARRA_REACHABILITY_OK) {
            return status;
        }
        out_report->reachable = reachable;
        out_report->visited_states = 1;
        if (reachable && trace_mode != CLEARRA_REACHABILITY_TRACE_NONE) {
            out_report->path_complete = true;
            out_report->debug_step_count = 1;
            out_report->debug_steps[0].rotation = rotation;
            out_report->debug_steps[0].x = x;
            out_report->debug_steps[0].y = y;
            out_report->debug_steps[0].transition_kind =
                CLEARRA_ROTATION_TRANSITION_NONE;
        }
        return CLEARRA_REACHABILITY_OK;
    }

    if (policy == CLEARRA_REACHABILITY_POLICY_LOCKED_REVERSE_GRAPH ||
        policy == CLEARRA_REACHABILITY_POLICY_LOCKED_180_REVERSE_GRAPH) {
        const ClearraReachabilityKickTable *effective_kick_table =
            clearra_reachability_mode_uses_kicks(mode) ? kick_table : 0;
        bool allow_180 = clearra_reachability_mode_supports_180(mode);
        if (frontier != 0) {
            return clearra_locked_reachability_is_reachable_with_frontier(
                layout, board, piece, rotation, x, y, allow_180,
                effective_kick_table, trace_mode, frontier, out_report);
        }
        if (trace_mode == CLEARRA_REACHABILITY_TRACE_HIGHEST_T_SPIN) {
            ClearraReachabilityFrontier local_frontier;
            clearra_locked_reachability_frontier_init(&local_frontier);
            return clearra_locked_reachability_is_reachable_with_frontier(
                layout, board, piece, rotation, x, y, allow_180,
                effective_kick_table, trace_mode, &local_frontier, out_report);
        }
        return clearra_locked_reachability_is_reachable(
            layout, board, piece, rotation, x, y, allow_180, effective_kick_table,
            out_report);
    }

    return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
}

ClearraReachabilityStatus clearra_reachability_check(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint8_t mode,
    const ClearraReachabilityKickTable *kick_table,
    ClearraReachabilityReport *out_report) {
    return reachability_check_internal(
        layout, board, piece, rotation, x, y, mode, kick_table,
        CLEARRA_REACHABILITY_TRACE_FIRST_LEGAL, 0, out_report);
}

ClearraReachabilityStatus clearra_reachability_check_with_frontier(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint8_t mode,
    const ClearraReachabilityKickTable *kick_table,
    uint8_t trace_mode,
    ClearraReachabilityFrontier *frontier,
    ClearraReachabilityReport *out_report) {
    if (frontier == 0) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    return reachability_check_internal(
        layout, board, piece, rotation, x, y, mode, kick_table, trace_mode,
        frontier, out_report);
}
