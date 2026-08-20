#include "hold_queue_verifier_internal.h"
#include "../supply/standard_bag_automaton.h"

clr_buildup_status clearra_buildup_queue_hold_init(
    const clr_buildup_problem *problem,
    ClearraBuildUpQueueHold *out_state) {
    if (problem == 0 || out_state == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    *out_state = problem->initial_hold_automaton;
    return CLR_BUILDUP_OK;
}

#include "hold_queue_verifier_internal.h"

bool clearra_buildup_hold_is_enabled(const clr_buildup_problem *problem) {
    return problem != 0 &&
           (problem->buildup_flags & CLR_BUILDUP_FLAG_HOLD_ENABLED) != 0u;
}

static bool clearra_buildup_uses_standard_bag_automaton(
    const clr_buildup_problem *problem) {
    return problem != 0 &&
           problem->source_execution_mode ==
               CLR_BUILDUP_SOURCE_STANDARD_BAG_AUTOMATON;
}

static clr_buildup_status buildup_status_from_standard_bag_status(
    ClearraStandardBagAutomatonStatus status) {
    switch (status) {
    case CLEARRA_STANDARD_BAG_AUTOMATON_OK:
        return CLR_BUILDUP_OK;
    case CLEARRA_STANDARD_BAG_AUTOMATON_PIECE_UNAVAILABLE:
        return CLR_BUILDUP_QUEUE_ORDER_IMPOSSIBLE;
    case CLEARRA_STANDARD_BAG_AUTOMATON_CAPACITY_EXCEEDED:
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    case CLEARRA_STANDARD_BAG_AUTOMATON_INVALID_STATE:
    default:
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
}

static clr_buildup_status append_standard_bag_branch(
    const clr_hold_automaton_state *next_state,
    uint8_t desired_piece,
    uint8_t branch_kind,
    uint8_t used_hold,
    uint8_t incoming_piece,
    const clr_hold_automaton_state *before,
    ClearraBuildUpHoldBranchTable *out_table) {
    if (next_state == 0 || before == 0 || out_table == 0 ||
        desired_piece > CLR_PIECE_L) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    uint8_t *count = &out_table->counts[desired_piece];
    if (*count >= CLEARRA_BUILDUP_HOLD_BRANCH_MAX) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    ClearraBuildUpHoldBranch *branch =
        &out_table->branches[desired_piece][*count];
    *branch = (ClearraBuildUpHoldBranch){0};
    branch->state = *next_state;
    branch->branch_kind = branch_kind;
    branch->used_hold = used_hold;
    branch->incoming_piece = incoming_piece;
    branch->held_piece_before = before->hold_piece;
    branch->hold_empty_before = before->hold_empty;
    *count = (uint8_t)(*count + 1u);
    return CLR_BUILDUP_OK;
}

static clr_buildup_status enumerate_standard_bag_automaton_branch_table(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *state,
    uint8_t desired_piece_mask,
    ClearraBuildUpHoldBranchTable *out_table) {
    ClearraStandardBagDraw current_draws[CLEARRA_STANDARD_BAG_DRAW_CAPACITY];
    uint8_t current_count = 0u;
    ClearraStandardBagAutomatonStatus draw_status = clearra_standard_bag_enumerate_draws(
        state, current_draws, CLEARRA_STANDARD_BAG_DRAW_CAPACITY, &current_count);
    if (draw_status != CLEARRA_STANDARD_BAG_AUTOMATON_OK) {
        return buildup_status_from_standard_bag_status(draw_status);
    }

    for (uint8_t index = 0u; index < current_count; ++index) {
        uint8_t current_piece = current_draws[index].piece;
        if ((desired_piece_mask & (uint8_t)(UINT8_C(1) << current_piece)) != 0u) {
            clr_buildup_status status = append_standard_bag_branch(
                &current_draws[index].state, current_piece,
                CLEARRA_BUILDUP_HOLD_BRANCH_CURRENT, 0u,
                current_piece, state, out_table);
            if (status != CLR_BUILDUP_OK) {
                return status;
            }
        }
    }
    if (!clearra_buildup_hold_is_enabled(problem)) {
        return CLR_BUILDUP_OK;
    }

    if (!state->hold_empty) {
        if (state->hold_piece < CLR_PIECE_I ||
            state->hold_piece > CLR_PIECE_L ||
            (desired_piece_mask &
             (uint8_t)(UINT8_C(1) << state->hold_piece)) == 0u) {
            return state->hold_piece < CLR_PIECE_I ||
                           state->hold_piece > CLR_PIECE_L
                       ? CLR_BUILDUP_INVALID_PROBLEM
                       : CLR_BUILDUP_OK;
        }
        for (uint8_t index = 0u; index < current_count; ++index) {
            clr_hold_automaton_state next = current_draws[index].state;
            next.hold_piece = current_draws[index].piece;
            next.hold_empty = 0u;
            clr_buildup_status status = append_standard_bag_branch(
                &next, state->hold_piece,
                CLEARRA_BUILDUP_HOLD_BRANCH_SWAP_HELD, 1u,
                current_draws[index].piece, state, out_table);
            if (status != CLR_BUILDUP_OK) {
                return status;
            }
        }
    } else {
        for (uint8_t index = 0u; index < current_count; ++index) {
            for (uint8_t next_piece = CLR_PIECE_I;
                 next_piece <= CLR_PIECE_L;
                 ++next_piece) {
                if ((desired_piece_mask &
                     (uint8_t)(UINT8_C(1) << next_piece)) == 0u) {
                    continue;
                }
                ClearraStandardBagDraw next_draw;
                draw_status = clearra_standard_bag_draw_piece(
                    &current_draws[index].state, next_piece, &next_draw);
                if (draw_status ==
                    CLEARRA_STANDARD_BAG_AUTOMATON_PIECE_UNAVAILABLE) {
                    continue;
                }
                if (draw_status != CLEARRA_STANDARD_BAG_AUTOMATON_OK) {
                    return buildup_status_from_standard_bag_status(draw_status);
                }
                clr_hold_automaton_state next = next_draw.state;
                next.hold_piece = current_draws[index].piece;
                next.hold_empty = 0u;
                clr_buildup_status status = append_standard_bag_branch(
                    &next, next_piece,
                    CLEARRA_BUILDUP_HOLD_BRANCH_STORE_CURRENT, 1u,
                    current_draws[index].piece, state, out_table);
                if (status != CLR_BUILDUP_OK) {
                    return status;
                }
            }
        }
    }

    return CLR_BUILDUP_OK;
}

#include "hold_queue_verifier_internal.h"

#include <string.h>
clr_piece_source_pattern_reader clearra_buildup_piece_source_reader_for_problem(
    const clr_buildup_problem *problem) {
    clr_piece_source_pattern_reader reader;
    memset(&reader, 0, sizeof(reader));
    if (problem != 0) {
        reader.source = problem->piece_source;
        reader.pattern_id = problem->piece_source_pattern_id;
        reader.fixed_or_materialized_pieces = problem->piece_source_pattern_pieces;
        reader.len = problem->piece_source_pattern_len;
        reader.complete = problem->piece_source_pattern_complete;
        reader.truncation_reason = problem->piece_source_pattern_truncation_reason;
    }
    return reader;
}clr_buildup_status clearra_buildup_piece_source_reader_piece_at(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *state,
    uint16_t cursor,
    uint8_t *out_piece,
    bool *out_has_piece) {
    if (problem == 0 || state == 0 || out_piece == 0 || out_has_piece == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    *out_has_piece = false;
    clr_piece_source_pattern_reader reader =
        clearra_buildup_piece_source_reader_for_problem(problem);
    clr_buildup_status status =
        clr_piece_source_pattern_piece_at(&reader, state, cursor, out_piece);
    if (status == CLR_BUILDUP_PIECE_WINDOW_IMPOSSIBLE) {
        return CLR_BUILDUP_OK;
    }
    if (status == CLR_BUILDUP_OK) {
        *out_has_piece = true;
    }
    return status;
}

#include "hold_queue_verifier_internal.h"

clr_buildup_status clearra_buildup_status_from_hold_automaton_status(
    uint32_t status) {
    switch (status) {
    case CLR_HOLD_AUTOMATON_OK:
        return CLR_BUILDUP_OK;
    case CLR_HOLD_AUTOMATON_INVALID_ARGUMENT:
    case CLR_HOLD_AUTOMATON_UNKNOWN_TRANSITION:
        return CLR_BUILDUP_INVALID_ARGUMENT;
    case CLR_HOLD_AUTOMATON_MISSING_HELD_PIECE:
    case CLR_HOLD_AUTOMATON_MISSING_NEXT_PIECE:
        return CLR_BUILDUP_QUEUE_ORDER_IMPOSSIBLE;
    default:
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
}

#include "hold_queue_verifier_internal.h"
uint8_t clearra_buildup_branch_kind_from_hold_transition(
    uint32_t transition) {
    switch (transition) {
    case CLR_HOLD_TRANSITION_USE_CURRENT:
        return CLEARRA_BUILDUP_HOLD_BRANCH_CURRENT;
    case CLR_HOLD_TRANSITION_SWAP_HELD:
        return CLEARRA_BUILDUP_HOLD_BRANCH_SWAP_HELD;
    case CLR_HOLD_TRANSITION_STORE_CURRENT_THEN_USE_NEXT:
        return CLEARRA_BUILDUP_HOLD_BRANCH_STORE_CURRENT;
    default:
        return 0u;
    }
}uint8_t clearra_buildup_transition_uses_hold(uint32_t transition) {
    return transition == CLR_HOLD_TRANSITION_SWAP_HELD ||
                   transition == CLR_HOLD_TRANSITION_STORE_CURRENT_THEN_USE_NEXT
               ? 1u
               : 0u;
}

#include "hold_queue_verifier_internal.h"
static uint64_t bag_remainder_key_with_piece(uint64_t key, uint8_t piece) {
    return piece > CLR_PIECE_L
               ? key
               : key + (UINT64_C(1) << ((uint64_t)piece * 4u));
}clr_buildup_status clearra_buildup_refresh_bag_state_from_reader(
    const clr_buildup_problem *problem,
    ClearraBuildUpQueueHold *state) {
    if (problem == 0 || state == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    clr_piece_source_pattern_reader reader =
        clearra_buildup_piece_source_reader_for_problem(problem);
    state->bag_epoch = (uint16_t)(state->cursor / 7u);
    state->bag_remainder_key = 0u;
    uint16_t bag_end = (uint16_t)((state->bag_epoch + 1u) * 7u);
    for (uint16_t cursor = state->cursor; cursor < bag_end; ++cursor) {
        uint8_t piece = CLR_PIECE_NONE;
        clr_buildup_status status =
            clr_piece_source_pattern_piece_at(&reader, state, cursor, &piece);
        if (status == CLR_BUILDUP_PIECE_WINDOW_IMPOSSIBLE) {
            return CLR_BUILDUP_OK;
        }
        if (status != CLR_BUILDUP_OK) {
            return status;
        }
        state->bag_remainder_key =
            bag_remainder_key_with_piece(state->bag_remainder_key, piece);
    }
    return CLR_BUILDUP_OK;
}

static bool take_piece_from_bag_remainder(uint64_t *key, uint8_t piece) {
    if (key == 0 || piece < CLR_PIECE_I || piece > CLR_PIECE_L) {
        return false;
    }
    uint8_t shift = (uint8_t)(piece * 4u);
    uint8_t count = (uint8_t)((*key >> shift) & UINT64_C(0x0f));
    if (count == 0u) {
        return false;
    }
    *key -= UINT64_C(1) << shift;
    return true;
}

static clr_buildup_status consume_bag_remainder_incrementally(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *before,
    uint32_t transition,
    uint8_t current_piece,
    uint8_t next_piece,
    ClearraBuildUpQueueHold *after) {
    if (problem == 0 || before == 0 || after == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    uint8_t consumed[2] = {current_piece, next_piece};
    uint8_t consumed_count =
        transition == CLR_HOLD_TRANSITION_STORE_CURRENT_THEN_USE_NEXT ? 2u : 1u;
    ClearraBuildUpQueueHold cursor_state = *before;
    uint16_t cursor = before->cursor;
    if (cursor_state.bag_epoch != (uint16_t)(cursor / 7u)) {
        clr_buildup_status status =
            clearra_buildup_refresh_bag_state_from_reader(problem, &cursor_state);
        if (status != CLR_BUILDUP_OK) {
            return status;
        }
    }

    for (uint8_t index = 0u; index < consumed_count; ++index) {
        if (!take_piece_from_bag_remainder(
                &cursor_state.bag_remainder_key, consumed[index])) {
            clr_buildup_status status =
                clearra_buildup_refresh_bag_state_from_reader(problem, &cursor_state);
            if (status != CLR_BUILDUP_OK ||
                !take_piece_from_bag_remainder(
                    &cursor_state.bag_remainder_key, consumed[index])) {
                return status == CLR_BUILDUP_OK ? CLR_BUILDUP_INVALID_PROBLEM
                                                : status;
            }
        }
        cursor++;
        cursor_state.cursor = cursor;
        if (cursor % 7u == 0u) {
            clr_buildup_status status =
                clearra_buildup_refresh_bag_state_from_reader(problem, &cursor_state);
            if (status != CLR_BUILDUP_OK) {
                return status;
            }
        }
    }
    if (cursor != after->cursor) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    after->bag_epoch = cursor_state.bag_epoch;
    after->bag_remainder_key = cursor_state.bag_remainder_key;
    return CLR_BUILDUP_OK;
}

#include "hold_queue_verifier_internal.h"

clr_buildup_status clearra_buildup_append_branch_from_hold_transition(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *state,
    uint32_t transition,
    uint8_t current_piece,
    uint8_t next_piece,
    uint8_t desired_piece,
    ClearraBuildUpHoldBranch *out_branches,
    uint8_t *out_count) {
    if (problem == 0 || state == 0 || out_branches == 0 || out_count == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (*out_count >= CLEARRA_BUILDUP_HOLD_BRANCH_MAX) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    clr_hold_automaton_step step;
    clr_buildup_status status = clearra_buildup_status_from_hold_automaton_status(
        clearra_hold_automaton_apply(
            state, transition, current_piece, next_piece, &step));
    if (status != CLR_BUILDUP_OK || step.used_piece != desired_piece) {
        return status;
    }
    ClearraBuildUpHoldBranch *branch = &out_branches[*out_count];
    branch->state = step.next_state;
    status = consume_bag_remainder_incrementally(
        problem,
        state,
        transition,
        current_piece,
        next_piece,
        &branch->state);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    branch->branch_kind =
        clearra_buildup_branch_kind_from_hold_transition(transition);
    branch->used_hold = clearra_buildup_transition_uses_hold(transition);
    branch->incoming_piece = current_piece;
    branch->held_piece_before = state->hold_piece;
    branch->hold_empty_before = state->hold_empty;
    *out_count = (uint8_t)(*out_count + 1u);
    return CLR_BUILDUP_OK;
}

#include "hold_queue_verifier_internal.h"

clr_buildup_status clearra_buildup_queue_hold_consume(
    const clr_buildup_problem *problem,
    ClearraBuildUpQueueHold *state,
    uint8_t desired_piece) {
    ClearraBuildUpHoldBranch branches[CLEARRA_BUILDUP_HOLD_BRANCH_MAX];
    uint8_t branch_count = 0u;
    clr_buildup_status status = clearra_buildup_queue_hold_enumerate_branches(
        problem, state, desired_piece, branches, &branch_count);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    if (branch_count == 0u) {
        return clearra_buildup_hold_is_enabled(problem)
                   ? CLR_BUILDUP_QUEUE_ORDER_IMPOSSIBLE
                   : CLR_BUILDUP_HOLD_DISABLED_IMPOSSIBLE;
    }
    *state = branches[0].state;
    return CLR_BUILDUP_OK;
}

#include "hold_queue_verifier_internal.h"

static clr_buildup_status append_terminal_projection_branch(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *state,
    uint8_t desired_piece_mask,
    bool terminal_step,
    ClearraBuildUpHoldBranchTable *out_table) {
    if (problem == 0 || state == 0 || out_table == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (!terminal_step ||
        problem->terminal_projection_policy !=
            CLR_BUILDUP_TERMINAL_PROJECTION_RELEASE_FINITE_HELD ||
        problem->source_execution_mode !=
            CLR_BUILDUP_SOURCE_CONCRETE_PATTERN ||
        !clearra_buildup_hold_is_enabled(problem) ||
        state->terminal_projection_consumed != 0u ||
        state->hold_empty != 0u) {
        return CLR_BUILDUP_OK;
    }
    if (state->hold_piece < CLR_PIECE_I || state->hold_piece > CLR_PIECE_L) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    clr_piece_source_pattern_reader reader =
        clearra_buildup_piece_source_reader_for_problem(problem);
    if (reader.complete == 0u) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    if (state->cursor != reader.len ||
        (desired_piece_mask &
         (uint8_t)(UINT8_C(1) << state->hold_piece)) == 0u) {
        return CLR_BUILDUP_OK;
    }

    ClearraBuildUpQueueHold next = *state;
    next.hold_piece = CLR_PIECE_NONE;
    next.hold_empty = 1u;
    next.terminal_projection_consumed = 1u;
    next.terminal_projection_provenance =
        CLEARRA_BUILDUP_TERMINAL_PROVENANCE_FINITE_SOURCE_END;
    return append_standard_bag_branch(
        &next,
        state->hold_piece,
        CLEARRA_BUILDUP_HOLD_BRANCH_RELEASE_HELD_AT_TERMINAL,
        1u,
        CLR_PIECE_NONE,
        state,
        out_table);
}

clr_buildup_status clearra_buildup_queue_hold_enumerate_branch_mask_for_step(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *state,
    uint8_t desired_piece_mask,
    bool terminal_step,
    ClearraBuildUpHoldBranchTable *out_table) {
    if (problem == 0 || state == 0 || out_table == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    memset(out_table->counts, 0, sizeof(out_table->counts));
    if (clearra_buildup_uses_standard_bag_automaton(problem)) {
        return enumerate_standard_bag_automaton_branch_table(
            problem, state, desired_piece_mask, out_table);
    }

    uint8_t current = CLR_PIECE_NONE;
    bool has_current = false;
    clr_buildup_status status = clearra_buildup_piece_source_reader_piece_at(
        problem, state, state->cursor, &current, &has_current);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    if (has_current && current > CLR_PIECE_L) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    if (has_current &&
        (desired_piece_mask & (uint8_t)(UINT8_C(1) << current)) != 0u) {
        status = clearra_buildup_append_branch_from_hold_transition(
            problem, state, CLR_HOLD_TRANSITION_USE_CURRENT, current,
            CLR_PIECE_NONE, current, out_table->branches[current],
            &out_table->counts[current]);
        if (status != CLR_BUILDUP_OK) {
            return status;
        }
    }
    if (!clearra_buildup_hold_is_enabled(problem)) {
        return CLR_BUILDUP_OK;
    }

    if (!state->hold_empty) {
        if (state->hold_piece < CLR_PIECE_I ||
            state->hold_piece > CLR_PIECE_L) {
            return CLR_BUILDUP_INVALID_PROBLEM;
        }
        if (has_current &&
            (desired_piece_mask &
             (uint8_t)(UINT8_C(1) << state->hold_piece)) != 0u) {
            status = clearra_buildup_append_branch_from_hold_transition(
                problem, state, CLR_HOLD_TRANSITION_SWAP_HELD, current,
                CLR_PIECE_NONE, state->hold_piece,
                out_table->branches[state->hold_piece],
                &out_table->counts[state->hold_piece]);
            if (status != CLR_BUILDUP_OK) {
                return status;
            }
        }
        return has_current
                   ? CLR_BUILDUP_OK
                   : append_terminal_projection_branch(
                         problem,
                         state,
                         desired_piece_mask,
                         terminal_step,
                         out_table);
    }

    if (has_current) {
        uint8_t next = CLR_PIECE_NONE;
        bool has_next = false;
        status = clearra_buildup_piece_source_reader_piece_at(
            problem, state, (uint16_t)(state->cursor + 1u), &next, &has_next);
        if (status != CLR_BUILDUP_OK) {
            return status;
        }
        if (has_next && (next < CLR_PIECE_I || next > CLR_PIECE_L)) {
            return CLR_BUILDUP_INVALID_PROBLEM;
        }
        if (has_next &&
            (desired_piece_mask & (uint8_t)(UINT8_C(1) << next)) != 0u) {
            status = clearra_buildup_append_branch_from_hold_transition(
                problem, state,
                CLR_HOLD_TRANSITION_STORE_CURRENT_THEN_USE_NEXT, current,
                next, next, out_table->branches[next],
                &out_table->counts[next]);
            if (status != CLR_BUILDUP_OK) {
                return status;
            }
        }
    }
    return CLR_BUILDUP_OK;
}

clr_buildup_status clearra_buildup_queue_hold_enumerate_branch_mask(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *state,
    uint8_t desired_piece_mask,
    ClearraBuildUpHoldBranchTable *out_table) {
    return clearra_buildup_queue_hold_enumerate_branch_mask_for_step(
        problem, state, desired_piece_mask, false, out_table);
}

clr_buildup_status clearra_buildup_queue_hold_enumerate_branches(
    const clr_buildup_problem *problem,
    const ClearraBuildUpQueueHold *state,
    uint8_t desired_piece,
    ClearraBuildUpHoldBranch *out_branches,
    uint8_t *out_count) {
    if (problem == 0 || state == 0 || desired_piece == CLR_PIECE_NONE ||
        out_branches == 0 || out_count == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    ClearraBuildUpHoldBranchTable table;
    clr_buildup_status status =
        clearra_buildup_queue_hold_enumerate_branch_mask(
            problem,
            state,
            (uint8_t)(UINT8_C(1) << desired_piece),
            &table);
    if (status != CLR_BUILDUP_OK) {
        *out_count = 0u;
        return status;
    }
    *out_count = table.counts[desired_piece];
    memcpy(
        out_branches, table.branches[desired_piece],
        (size_t)(*out_count) * sizeof(*out_branches));
    if (*out_count != 0u) {
        return CLR_BUILDUP_OK;
    }
    return clearra_buildup_hold_is_enabled(problem)
               ? CLR_BUILDUP_QUEUE_ORDER_IMPOSSIBLE
               : CLR_BUILDUP_HOLD_DISABLED_IMPOSSIBLE;
}

#include "hold_queue_verifier_internal.h"

#include <string.h>

clr_buildup_status clearra_buildup_verify_bag_pattern(
    const clr_buildup_problem *problem) {
    if (problem == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (clearra_buildup_uses_standard_bag_automaton(problem)) {
        if (problem->piece_source.source_kind != CLR_PIECE_SOURCE_BAG_UNIVERSE ||
            problem->piece_source.exact_bag_automaton_supported != 1u ||
            problem->rule.bag_profile_id != CLR_BAG_STANDARD_7_BAG ||
            !clearra_standard_bag_remainder_key_is_exact(
                problem->initial_hold_automaton.bag_remainder_key)) {
            return CLR_BUILDUP_INVALID_PROBLEM;
        }
        return CLR_BUILDUP_OK;
    }
    if (problem->piece_source.source_kind != CLR_PIECE_SOURCE_BAG_UNIVERSE) {
        return CLR_BUILDUP_OK;
    }
    clr_piece_source_pattern_reader reader =
        clearra_buildup_piece_source_reader_for_problem(problem);
    if (reader.complete == 0u) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    if (reader.len == 0u) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    uint8_t seen[CLR_PIECE_L + 1u];
    memset(seen, 0, sizeof(seen));
    uint16_t current_epoch = UINT16_MAX;
    for (uint16_t index = 0u; index < reader.len; ++index) {
        uint16_t epoch = (uint16_t)(index / 7u);
        if (epoch != current_epoch) {
            memset(seen, 0, sizeof(seen));
            current_epoch = epoch;
        }
        uint8_t piece = CLR_PIECE_NONE;
        clr_buildup_status status = clr_piece_source_pattern_piece_at(
            &reader, &problem->initial_hold_automaton, index, &piece);
        if (status == CLR_BUILDUP_PIECE_WINDOW_IMPOSSIBLE) {
            break;
        }
        if (status != CLR_BUILDUP_OK) {
            return status;
        }
        if (piece < CLR_PIECE_I || piece > CLR_PIECE_L || seen[piece] != 0) {
            return CLR_BUILDUP_BAG_PATTERN_IMPOSSIBLE;
        }
        seen[piece] = 1;
    }
    return CLR_BUILDUP_OK;
}
