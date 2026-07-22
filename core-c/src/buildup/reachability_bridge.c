#include "buildup_internal.h"

uint8_t clearra_buildup_reachability_mode_for_rule(
    const clr_rule_profile_descriptor *rule) {
    if (rule == 0) {
        return 0u;
    }
    if (rule->kick_profile_id == CLR_KICK_NO_KICK ||
        rule->rule_profile_id == CLR_RULE_NO_KICK) {
        return CLEARRA_REACHABILITY_MODE_HARDDROP;
    }
    if (rule->kick_profile_id == CLR_KICK_SRS_PLUS_180 ||
        rule->rule_profile_id == CLR_RULE_SRS_PLUS) {
        return CLEARRA_REACHABILITY_MODE_LOCKED_180;
    }
    return CLEARRA_REACHABILITY_MODE_LOCKED;
}

clr_buildup_status clearra_buildup_status_from_reachability_status(
    ClearraReachabilityStatus status) {
    if (status == CLEARRA_REACHABILITY_OK) {
        return CLR_BUILDUP_OK;
    }
    if (status == CLEARRA_REACHABILITY_COLLISION) {
        return CLR_BUILDUP_COLLISION;
    }
    if (status == CLEARRA_REACHABILITY_UNREACHABLE) {
        return CLR_BUILDUP_REACHABILITY_IMPOSSIBLE;
    }
    if (status == CLEARRA_REACHABILITY_CAPACITY_EXCEEDED) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    if (status == CLEARRA_REACHABILITY_INVALID_ARGUMENT) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    return CLR_BUILDUP_INVALID_PROBLEM;
}

static ClearraReachabilityStatus
buildup_reachability_bridge_check_compiled_internal(
    const ClearraCompactRuleProfile *compiled_rule,
    ClearraBoard64Layout layout,
    uint64_t board,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    uint8_t mode,
    uint8_t trace_mode,
    ClearraReachabilityFrontier *frontier,
    ClearraReachabilityReport *out_report) {
    if (compiled_rule == 0 || operation == 0 || out_report == 0 || mode == 0u) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    ClearraReachabilityKickTable kick_table;
    ClearraReachabilityStatus status =
        clearra_reachability_kick_table_from_compiled_rule(
            compiled_rule, operation->piece, &kick_table);
    if (status != CLEARRA_REACHABILITY_OK) {
        return status;
    }
    if (frontier != 0) {
        return clearra_reachability_check_with_frontier(
            layout, board, operation->piece, operation->rotation,
            operation->x, adjusted_y, mode, &kick_table, trace_mode, frontier,
            out_report);
    }
    return clearra_reachability_check(
        layout, board, operation->piece, operation->rotation, operation->x,
        adjusted_y, mode, &kick_table, out_report);
}

ClearraReachabilityStatus clearra_buildup_reachability_bridge_check_compiled(
    const ClearraCompactRuleProfile *compiled_rule,
    ClearraBoard64Layout layout,
    uint64_t board,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    uint8_t mode,
    ClearraReachabilityReport *out_report) {
    return buildup_reachability_bridge_check_compiled_internal(
        compiled_rule, layout, board, operation, adjusted_y, mode,
        CLEARRA_REACHABILITY_TRACE_FIRST_LEGAL, 0, out_report);
}

ClearraReachabilityStatus
clearra_buildup_reachability_bridge_check_compiled_with_frontier(
    const ClearraCompactRuleProfile *compiled_rule,
    ClearraBoard64Layout layout,
    uint64_t board,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    uint8_t mode,
    uint8_t trace_mode,
    ClearraReachabilityFrontier *frontier,
    ClearraReachabilityReport *out_report) {
    return buildup_reachability_bridge_check_compiled_internal(
        compiled_rule, layout, board, operation, adjusted_y, mode,
        trace_mode, frontier, out_report);
}

clr_buildup_status clearra_buildup_reachability_bridge_accepts(
    const clr_buildup_problem *problem,
    ClearraBoard64Layout layout,
    uint64_t board,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    ClearraReachabilityReport *out_report) {
    if (problem == 0 || operation == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }

    ClearraCompactRuleProfile compiled_rule;
    ClearraReachabilityStatus compile_status =
        clearra_reachability_compile_rule(&problem->rule, &compiled_rule);
    if (compile_status != CLEARRA_REACHABILITY_OK) {
        return clearra_buildup_status_from_reachability_status(compile_status);
    }
    ClearraReachabilityReport local_report;
    ClearraReachabilityReport *report = out_report == 0 ? &local_report : out_report;
    ClearraReachabilityStatus status =
        clearra_buildup_reachability_bridge_check_compiled(
            &compiled_rule, layout, board, operation, adjusted_y,
            clearra_buildup_reachability_mode_for_rule(&problem->rule), report);
    if (status != CLEARRA_REACHABILITY_OK) {
        return clearra_buildup_status_from_reachability_status(status);
    }
    return report->reachable ? CLR_BUILDUP_OK : CLR_BUILDUP_REACHABILITY_IMPOSSIBLE;
}
