#ifndef CLEARRA_CORE_C_REACHABILITY_H
#define CLEARRA_CORE_C_REACHABILITY_H

#include "../candidate/candidate.h"

#include <stdbool.h>
#include <stdint.h>

#define CLEARRA_REACHABILITY_MAX_GRAPH_STATES 512
#define CLEARRA_REACHABILITY_MAX_DEBUG_STEPS 64
typedef enum ClearraReachabilityStatus {
    CLEARRA_REACHABILITY_OK = 0,
    CLEARRA_REACHABILITY_INVALID_ARGUMENT = 1,
    CLEARRA_REACHABILITY_INVALID_OPERATION = 2,
    CLEARRA_REACHABILITY_COLLISION = 3,
    CLEARRA_REACHABILITY_UNREACHABLE = 4,
    CLEARRA_REACHABILITY_CAPACITY_EXCEEDED = 5
} ClearraReachabilityStatus;typedef enum ClearraReachabilityMode {
    CLEARRA_REACHABILITY_MODE_HARDDROP = 1,
    CLEARRA_REACHABILITY_MODE_LOCKED = 2,
    CLEARRA_REACHABILITY_MODE_LOCKED_180 = 3,
    CLEARRA_REACHABILITY_MODE_KICK_AWARE = 4
} ClearraReachabilityMode;typedef enum ClearraReachabilityPolicy {
    CLEARRA_REACHABILITY_POLICY_INVALID = 0,
    CLEARRA_REACHABILITY_POLICY_HARDDROP_ONLY = 1,
    CLEARRA_REACHABILITY_POLICY_LOCKED_REVERSE_GRAPH = 2,
    CLEARRA_REACHABILITY_POLICY_LOCKED_180_REVERSE_GRAPH = 3
} ClearraReachabilityPolicy;typedef enum ClearraReachabilityTraceMode {
    CLEARRA_REACHABILITY_TRACE_NONE = 0,
    CLEARRA_REACHABILITY_TRACE_FIRST_LEGAL = 1,
    CLEARRA_REACHABILITY_TRACE_HIGHEST_T_SPIN = 2
} ClearraReachabilityTraceMode;
struct ClearraReachabilityKickTable {
    const ClearraCompactKickTable *compact_table;
    ClearraCompactKickTable owned_compact_table;
    uint8_t piece;
    const ClearraKickOffset *clockwise_offsets;
    uint8_t clockwise_count;
    const ClearraKickOffset *counter_clockwise_offsets;
    uint8_t counter_clockwise_count;
    const ClearraKickOffset *half_turn_offsets;
    uint8_t half_turn_count;
};
typedef struct ClearraReachabilityDebugStep {
    uint8_t rotation;
    int8_t x;
    int8_t y;
    uint8_t transition_kind;
} ClearraReachabilityDebugStep;typedef struct ClearraReachabilityReport {
    bool reachable;
    uint16_t visited_states;
    bool used_kick;
    bool used_180;
    bool has_rotation_evidence;
    bool first_success_confirmed;
    bool path_complete;
    uint8_t rotation_from;
    uint8_t rotation_to;
    uint8_t rotation_request;
    uint8_t kick_index;
    int8_t kick_dx;
    int8_t kick_dy;
    int8_t predecessor_x;
    int8_t predecessor_y;
    int8_t result_x;
    int8_t result_y;
    uint8_t debug_step_count;
    ClearraReachabilityDebugStep debug_steps[CLEARRA_REACHABILITY_MAX_DEBUG_STEPS];
} ClearraReachabilityReport;

typedef struct ClearraReachabilityFrontier ClearraReachabilityFrontier;

ClearraReachabilityStatus clearra_reachability_check(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint8_t mode,
    const ClearraReachabilityKickTable *kick_table,
    ClearraReachabilityReport *out_report);
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
    ClearraReachabilityReport *out_report);
void clearra_reachability_report_clear(ClearraReachabilityReport *report);
bool clearra_reachability_mode_supports_180(uint8_t mode);
bool clearra_reachability_mode_uses_kicks(uint8_t mode);
ClearraReachabilityPolicy clearra_reachability_policy_for_mode(uint8_t mode);
ClearraReachabilityStatus clearra_harddrop_reachability_is_reachable(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_reachable);
ClearraReachabilityStatus clearra_locked_reachability_is_reachable(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool allow_180,
    const ClearraReachabilityKickTable *kick_table,
    ClearraReachabilityReport *out_report);
ClearraReachabilityStatus clearra_kick_first_success(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    int8_t anchor_x,
    int8_t anchor_y,
    const ClearraReachabilityKickTable *kick_table,
    ClearraCandidateOperation *out_operation);
ClearraReachabilityStatus clearra_reachability_kick_offsets_for_transition(
    const ClearraReachabilityKickTable *kick_table,
    uint8_t from_rotation,
    uint8_t to_rotation,
    const ClearraKickOffset **out_offsets,
    uint8_t *out_count);
ClearraReachabilityStatus clearra_reachability_kick_table_from_rule(
    const clr_rule_profile_descriptor *rule,
    uint8_t piece,
    ClearraReachabilityKickTable *out_table);
ClearraReachabilityStatus clearra_reachability_compile_rule(
    const clr_rule_profile_descriptor *rule,
    ClearraCompactRuleProfile *out_profile);
ClearraReachabilityStatus clearra_reachability_kick_table_from_compiled_rule(
    const ClearraCompactRuleProfile *profile,
    uint8_t piece,
    ClearraReachabilityKickTable *out_table);
#endif
