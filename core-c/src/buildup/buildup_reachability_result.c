#include "buildup_reachability_result.h"

bool clearra_buildup_reachability_result_has_flag(
    const ClearraBuildUpReachabilityResult *result,
    uint8_t flag) {
    return result != 0 && (result->flags & flag) != 0u;
}

void clearra_buildup_reachability_result_from_report(
    const ClearraReachabilityReport *report,
    ClearraBuildUpReachabilityResult *out_result) {
    if (out_result == 0) {
        return;
    }
    *out_result = (ClearraBuildUpReachabilityResult){0};
    if (report == 0) {
        return;
    }
    out_result->path_digest = clearra_buildup_reachability_path_digest(report);
    out_result->visited_states = report->visited_states;
    out_result->flags =
        (uint8_t)((report->reachable ? CLEARRA_BUILDUP_REACHABLE_FLAG : 0u) |
                  (report->used_kick ? CLEARRA_BUILDUP_USED_KICK_FLAG : 0u) |
                  (report->used_180 ? CLEARRA_BUILDUP_USED_180_FLAG : 0u) |
                  (report->has_rotation_evidence
                       ? CLEARRA_BUILDUP_ROTATION_EVIDENCE_FLAG
                       : 0u) |
                  (report->first_success_confirmed
                       ? CLEARRA_BUILDUP_FIRST_SUCCESS_FLAG
                       : 0u) |
                  (report->path_complete
                       ? CLEARRA_BUILDUP_PATH_COMPLETE_FLAG
                       : 0u) |
                  (report->has_rotation_evidence
                       ? CLEARRA_BUILDUP_LAST_ACTION_ROTATION_FLAG
                       : 0u));
    out_result->rotation_from = report->rotation_from;
    out_result->rotation_to = report->rotation_to;
    out_result->rotation_request = report->rotation_request;
    out_result->kick_index = report->kick_index;
    out_result->kick_dx = report->kick_dx;
    out_result->kick_dy = report->kick_dy;
    out_result->predecessor_x = report->predecessor_x;
    out_result->predecessor_y = report->predecessor_y;
    out_result->result_x = report->result_x;
    out_result->result_y = report->result_y;
}

clr_buildup_status clearra_buildup_reachability_check_compiled(
    const ClearraCompactRuleProfile *compiled_rule,
    ClearraBoard64Layout layout,
    uint64_t board,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    uint8_t mode,
    uint8_t trace_mode,
    ClearraReachabilityFrontier *frontier,
    ClearraBuildUpReachabilityResult *out_result) {
    if (compiled_rule == 0 || operation == 0 || out_result == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    ClearraReachabilityReport report;
    ClearraReachabilityStatus status =
        clearra_buildup_reachability_bridge_check_compiled_with_frontier(
            compiled_rule, layout, board, operation, adjusted_y, mode,
            trace_mode, frontier, &report);
    if (status != CLEARRA_REACHABILITY_OK) {
        return clearra_buildup_status_from_reachability_status(status);
    }
    clearra_buildup_reachability_result_from_report(&report, out_result);
    return clearra_buildup_reachability_result_has_flag(
               out_result, CLEARRA_BUILDUP_REACHABLE_FLAG)
               ? CLR_BUILDUP_OK
               : CLR_BUILDUP_REACHABILITY_IMPOSSIBLE;
}
