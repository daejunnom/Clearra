#ifndef CLEARRA_BUILDUP_INTERNAL_H
#define CLEARRA_BUILDUP_INTERNAL_H

#include "../board/board64.h"
#include "../reachability/reachability.h"

#include "buildup_state.h"
#include "clr_problem.h"

#include <stdbool.h>
#include <stdint.h>
typedef struct ClearraBuildUpOrder {
    uint16_t indices[CLR_BUILDUP_MAX_OPERATIONS];
    uint16_t count;
} ClearraBuildUpOrder;typedef clr_hold_automaton_state ClearraBuildUpQueueHold;typedef enum ClearraBuildUpBranchOutcome {
    CLEARRA_BUILDUP_BRANCH_SUCCESS = 0,
    CLEARRA_BUILDUP_BRANCH_LOGICAL_REJECT = 1,
    CLEARRA_BUILDUP_BRANCH_INCOMPLETE = 2,
    CLEARRA_BUILDUP_BRANCH_FATAL = 3
} ClearraBuildUpBranchOutcome;
#define CLEARRA_BUILDUP_HOLD_BRANCH_MAX 8u
#define CLEARRA_BUILDUP_HOLD_BRANCH_CURRENT CLR_BUILDUP_HOLD_BRANCH_CURRENT
#define CLEARRA_BUILDUP_HOLD_BRANCH_SWAP_HELD CLR_BUILDUP_HOLD_BRANCH_SWAP_HELD
#define CLEARRA_BUILDUP_HOLD_BRANCH_STORE_CURRENT CLR_BUILDUP_HOLD_BRANCH_STORE_CURRENT
typedef struct ClearraBuildUpHoldBranch {
    ClearraBuildUpQueueHold state;
    uint8_t branch_kind;
    uint8_t used_hold;
    uint8_t incoming_piece;
    uint8_t held_piece_before;
    uint8_t hold_empty_before;
    uint8_t reserved[3];
} ClearraBuildUpHoldBranch;
typedef struct ClearraBuildUpHoldBranchTable {
    ClearraBuildUpHoldBranch
        branches[CLR_PIECE_L + 1u][CLEARRA_BUILDUP_HOLD_BRANCH_MAX];
    uint8_t counts[CLR_PIECE_L + 1u];
} ClearraBuildUpHoldBranchTable;
clr_buildup_status clearra_buildup_order_from_problem(
    const clr_buildup_problem *problem,
    ClearraBuildUpOrder *out_order);
clr_buildup_status clearra_buildup_check_line_clear_dependency(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t placement_mask);
clr_buildup_status clearra_buildup_adjust_operation_for_line_clears(
    ClearraBoard64Layout layout,
    ClearraBuildUpState state,
    const clr_buildup_operation *operation,
    uint64_t *out_mask,
    int8_t *out_y);
bool clearra_buildup_operation_matches_clear_state(
    const ClearraBuildUpState *state,
    const clr_buildup_operation *operation);
bool clearra_buildup_operation_domain_may_match_clear_state(
    const clr_buildup_problem *problem,
    const ClearraBuildUpState *state,
    uint16_t operation_index);
clr_buildup_status clearra_buildup_grounded_filter_accepts(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t placement_mask);
clr_buildup_status clearra_buildup_reachability_bridge_accepts(
    const clr_buildup_problem *problem,
    ClearraBoard64Layout layout,
    uint64_t board,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    ClearraReachabilityReport *out_report);
uint8_t clearra_buildup_reachability_mode_for_rule(
    const clr_rule_profile_descriptor *rule);
ClearraReachabilityStatus clearra_buildup_reachability_bridge_check_compiled(
    const ClearraCompactRuleProfile *compiled_rule,
    ClearraBoard64Layout layout,
    uint64_t board,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    uint8_t mode,
    ClearraReachabilityReport *out_report);
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
    ClearraReachabilityReport *out_report);
clr_buildup_status clearra_buildup_status_from_reachability_status(
    ClearraReachabilityStatus status);
ClearraBuildUpBranchOutcome clearra_buildup_branch_outcome_for_status(
    clr_buildup_status status);
clr_buildup_status clearra_buildup_queue_hold_init(
    const clr_buildup_problem *problem,
    ClearraBuildUpQueueHold *out_state);
clr_buildup_status clearra_buildup_queue_hold_consume(
    const clr_buildup_problem *problem,
    ClearraBuildUpQueueHold *state,
    uint8_t desired_piece);
clr_buildup_status clearra_buildup_queue_hold_enumerate_branches(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *state,
    uint8_t desired_piece,
    ClearraBuildUpHoldBranch *out_branches,
    uint8_t *out_count);
clr_buildup_status clearra_buildup_queue_hold_enumerate_branch_mask(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *state,
    uint8_t desired_piece_mask,
    ClearraBuildUpHoldBranchTable *out_table);
clr_buildup_status clearra_buildup_verify_bag_pattern(
    const clr_buildup_problem *problem);
void clearra_build_variant_from_state(
    const clr_buildup_problem *problem,
    const ClearraBuildUpState *state,
    clr_build_variant_view *out_variant);
#endif
