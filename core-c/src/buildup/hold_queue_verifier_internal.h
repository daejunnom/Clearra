#ifndef CLEARRA_HOLD_QUEUE_VERIFIER_INTERNAL_H
#define CLEARRA_HOLD_QUEUE_VERIFIER_INTERNAL_H

#include "buildup_internal.h"
bool clearra_buildup_hold_is_enabled(const clr_buildup_problem *problem);
clr_piece_source_pattern_reader clearra_buildup_piece_source_reader_for_problem(
    const clr_buildup_problem *problem);
clr_buildup_status clearra_buildup_piece_source_reader_piece_at(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *state,
    uint16_t cursor,
    uint8_t *out_piece,
    bool *out_has_piece);
clr_buildup_status clearra_buildup_status_from_hold_automaton_status(
    uint32_t status);
uint8_t clearra_buildup_branch_kind_from_hold_transition(
    uint32_t transition);
uint8_t clearra_buildup_transition_uses_hold(uint32_t transition);
clr_buildup_status clearra_buildup_refresh_bag_state_from_reader(
    const clr_buildup_problem *problem,
    ClearraBuildUpQueueHold *state);
clr_buildup_status clearra_buildup_append_branch_from_hold_transition(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *state,
    uint32_t transition,
    uint8_t current_piece,
    uint8_t next_piece,
    uint8_t desired_piece,
    ClearraBuildUpHoldBranch *out_branches,
    uint8_t *out_count);
#endif
