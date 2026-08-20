#include "buildup_search_internal.h"
#include "buildup_workspace.h"
#include "clr_execution_control.h"
#include "clr_search_profile.h"

static bool completion_memo_can_shortcut(
    const ClearraBuildUpSearchContext *context) {
    return context != 0 && context->stop_after_first_success == 0u &&
           (context->out_variants == 0 ||
            context->out_variants->count >= context->max_retained_variants);
}

static clr_buildup_status add_memoized_completions(
    ClearraBuildUpSearchContext *context,
    uint64_t completion_count) {
    if (UINT64_MAX - context->enumerated_variant_count < completion_count) {
        context->incomplete_branch_seen = 1u;
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    context->enumerated_variant_count += completion_count;
    if (context->enumerated_variant_count > context->max_count_variants) {
        return CLR_BUILDUP_ENUMERATION_TRUNCATED;
    }
    return CLR_BUILDUP_OK;
}

static clr_buildup_status clear_state_eligible_operations(
    const ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations,
    uint16_t *out_eligible,
    uint8_t *out_piece_mask) {
    uint16_t eligible = 0u;
    uint8_t piece_mask = 0u;
    uint16_t all_operations = (uint16_t)(
        ((uint32_t)UINT16_C(1) << context->order.count) - UINT32_C(1));
    uint16_t placed_operations =
        (uint16_t)(all_operations & ~remaining_operations);
    for (uint16_t operation_index = 0u;
         operation_index < context->order.count;
         ++operation_index) {
        uint16_t operation_bit =
            (uint16_t)(UINT16_C(1) << operation_index);
        if ((remaining_operations & operation_bit) == 0u) {
            continue;
        }
        if (context->operation_source.required_predecessors != 0) {
            uint16_t predecessors = context->operation_source
                                        .required_predecessors[operation_index];
            if ((placed_operations & predecessors) != predecessors) {
                clr_search_profile_count(
                    CLR_PROFILE_BUILDUP_CLEAR_STATE_SKIPS, 1u);
                continue;
            }
        }
        clr_buildup_operation operation;
        if (clearra_buildup_operation_source_operation_at(
                &context->operation_source,
                operation_index,
                &operation) != CLR_BUILDUP_OK ||
            operation.piece < CLR_PIECE_I || operation.piece > CLR_PIECE_L) {
            return CLR_BUILDUP_INVALID_PROBLEM;
        }
        if (clearra_buildup_operation_source_may_match_clear_state(
                &context->operation_source, state, operation_index)) {
            eligible = (uint16_t)(eligible | operation_bit);
            piece_mask = (uint8_t)(
                piece_mask | (uint8_t)(UINT8_C(1) << operation.piece));
        } else {
            clr_search_profile_count(
                CLR_PROFILE_BUILDUP_CLEAR_STATE_SKIPS, 1u);
        }
    }
    *out_eligible = eligible;
    *out_piece_mask = piece_mask;
    return CLR_BUILDUP_OK;
}

clr_buildup_status clearra_buildup_search_order(
    ClearraBuildUpSearchContext *context,
    ClearraBuildUpState state,
    ClearraBuildUpQueueHold queue_hold,
    uint16_t remaining_operations,
    uint16_t depth) {
    uint64_t max_nodes = context->problem->packing.budget.max_nodes;
    if (max_nodes != 0u && context->expanded_state_count >= max_nodes) {
        context->incomplete_branch_seen = 1u;
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    context->expanded_state_count++;
    if (clr_execution_control_poll(&context->cancellation_poll_counter)) {
        context->incomplete_branch_seen = 1u;
        return CLR_BUILDUP_CANCELLED;
    }
    if (remaining_operations == 0u) {
        clr_buildup_status status =
            clearra_buildup_search_verify_goal(context->problem, &state);
        if (status == CLR_BUILDUP_OK) {
            return clearra_buildup_search_record_success(context, &state);
        }
        clearra_buildup_search_record_failure(context, status, depth);
        return CLR_BUILDUP_OK;
    }
    uint64_t memoized_completion_count = 0u;
    /* A search enters its root once, so a root memo entry cannot be reused. */
    if (depth != 0u && clearra_buildup_search_completion_memo_lookup(
            context,
            &state,
            remaining_operations,
            &memoized_completion_count)) {
        if (memoized_completion_count == 0u) {
            return CLR_BUILDUP_OK;
        }
        if (completion_memo_can_shortcut(context)) {
            return add_memoized_completions(
                context, memoized_completion_count);
        }
    }

    uint16_t eligible_operations = 0u;
    uint8_t eligible_piece_mask = 0u;
    clr_buildup_status status = clear_state_eligible_operations(
        context,
        &state,
        remaining_operations,
        &eligible_operations,
        &eligible_piece_mask);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    if (eligible_operations == 0u) {
        clearra_buildup_search_record_failure(
            context, CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE, depth);
        if (depth != 0u) {
            clearra_buildup_search_failed_memo_insert(
                context, &state, remaining_operations);
        }
        return CLR_BUILDUP_OK;
    }

    ClearraBuildUpHoldBranchTable hold_branches;
    clr_search_profile_span hold_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_HOLD_BRANCH_ENUMERATION);
    status = clearra_buildup_queue_hold_enumerate_branch_mask_for_step(
        context->problem,
        &queue_hold,
        eligible_piece_mask,
        (remaining_operations & (uint16_t)(remaining_operations - 1u)) == 0u,
        &hold_branches);
    uint64_t available_branch_count = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        available_branch_count += hold_branches.counts[piece];
    }
    (void)clr_search_profile_end(hold_span, available_branch_count);
    if (status != CLR_BUILDUP_OK) {
        clearra_buildup_search_record_failure(context, status, depth);
        if (clearra_buildup_branch_outcome_for_status(status) ==
            CLEARRA_BUILDUP_BRANCH_LOGICAL_REJECT) {
            if (depth != 0u) {
                clearra_buildup_search_failed_memo_insert(
                    context, &state, remaining_operations);
            }
            return CLR_BUILDUP_OK;
        }
        return status;
    }

    uint64_t variants_before = context->enumerated_variant_count;
    for (uint16_t preference = 0; preference < context->order.count; preference++) {
        uint16_t operation_index = context->order.indices[preference];
        uint16_t operation_bit = (uint16_t)(1u << operation_index);
        if ((eligible_operations & operation_bit) == 0u) {
            continue;
        }
        clr_buildup_operation representative_operation;
        if (clearra_buildup_operation_source_operation_at(
                &context->operation_source,
                operation_index,
                &representative_operation) != CLR_BUILDUP_OK ||
            representative_operation.piece > CLR_PIECE_L) {
            return CLR_BUILDUP_INVALID_PROBLEM;
        }
        uint8_t branch_count =
            hold_branches.counts[representative_operation.piece];
        if (branch_count == 0u) {
            clearra_buildup_search_record_failure(
                context,
                (context->problem->buildup_flags &
                 CLR_BUILDUP_FLAG_HOLD_ENABLED) != 0u
                    ? CLR_BUILDUP_QUEUE_ORDER_IMPOSSIBLE
                    : CLR_BUILDUP_HOLD_DISABLED_IMPOSSIBLE,
                depth);
            continue;
        }
        ClearraBuildUpHoldBranch *branches =
            hold_branches.branches[representative_operation.piece];
        if (!context->preserve_hold_branches && branch_count > 1u) {
            branch_count = 1u;
        }
        ClearraBuildUpRootOperationTransitions *root_transitions =
            depth == 0u && context->root_transition_cache != 0
                ? &context->root_transition_cache->operations[operation_index]
                : 0;
        clr_buildup_operation operation_variants
            [CLR_BUILDUP_MAX_OPERATION_VARIANTS];
        uint8_t operation_variant_count = 0u;
        if (root_transitions != 0) {
            status = clearra_buildup_root_transition_cache_prepare_operation(
                context, &state, operation_index, root_transitions);
            if (status != CLR_BUILDUP_OK) {
                return status;
            }
            status = root_transitions->preparation_status;
            operation_variant_count = root_transitions->count;
        } else {
            status = clearra_buildup_operation_variants_for_state(
                context,
                &state,
                operation_index,
                operation_variants,
                &operation_variant_count);
        }
        if (status != CLR_BUILDUP_OK) {
            clearra_buildup_search_record_failure(context, status, depth);
            if (clearra_buildup_branch_outcome_for_status(status) ==
                CLEARRA_BUILDUP_BRANCH_LOGICAL_REJECT) {
                continue;
            }
            return status;
        }
        if (operation_variant_count == 0u) {
            clearra_buildup_search_record_failure(
                context, CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE, depth);
            continue;
        }
        for (uint8_t branch_index = 0; branch_index < branch_count; branch_index++) {
            for (uint8_t variant_index = 0u;
                 variant_index < operation_variant_count;
                 ++variant_index) {
                ClearraBuildUpState next_state;
                clr_buildup_trace_step trace_step;
                clr_kick_evidence_view kick_evidence;
                if (root_transitions != 0) {
                    const ClearraBuildUpRootTransition *transition =
                        &root_transitions->transitions[variant_index];
                    status = transition->status;
                    next_state = transition->next_state;
                    next_state.hold_automaton_state =
                        branches[branch_index].state;
                    trace_step = transition->trace_step;
                    kick_evidence = transition->kick_evidence;
                } else {
                    status = clearra_buildup_search_try_operation(
                        context,
                        state,
                        branches[branch_index].state,
                        &operation_variants[variant_index],
                        operation_index,
                        &next_state,
                        &trace_step,
                        &kick_evidence);
                }
                if (status != CLR_BUILDUP_OK) {
                    clearra_buildup_search_record_failure(context, status, depth);
                    if (clearra_buildup_branch_outcome_for_status(status) ==
                        CLEARRA_BUILDUP_BRANCH_LOGICAL_REJECT) {
                        continue;
                    }
                    return status;
                }
                next_state.last_hold_branch_kind =
                    branches[branch_index].branch_kind;
                if (context->capture_trace && depth < CLR_BUILDUP_MAX_OPERATIONS) {
                    trace_step.hold_branch_kind =
                        branches[branch_index].branch_kind;
                    trace_step.used_hold = branches[branch_index].used_hold;
                    trace_step.incoming_piece =
                        branches[branch_index].incoming_piece;
                    trace_step.held_piece_before =
                        branches[branch_index].held_piece_before;
                    trace_step.hold_empty_before =
                        branches[branch_index].hold_empty_before;
                    context->current_trace_steps[depth] = trace_step;
                    context->current_kick_evidence[depth] = kick_evidence;
                }
                status = clearra_buildup_search_order(
                    context,
                    next_state,
                    next_state.hold_automaton_state,
                    (uint16_t)(remaining_operations & ~operation_bit),
                    (uint16_t)(depth + 1u));
                if (status != CLR_BUILDUP_OK) {
                    return status;
                }
                if (context->stop_after_first_success &&
                    context->enumerated_variant_count > 0u) {
                    return CLR_BUILDUP_OK;
                }
            }
        }
    }

    uint64_t completion_count =
        context->enumerated_variant_count - variants_before;
    if (completion_count == 0u && depth != 0u) {
        clearra_buildup_search_failed_memo_insert(
            context, &state, remaining_operations);
    } else if (depth != 0u && completion_memo_can_shortcut(context)) {
        clearra_buildup_search_completion_memo_insert(
            context, &state, remaining_operations, completion_count);
    }
    return CLR_BUILDUP_OK;
}

#include "buildup_bfs_state.h"
#include "buildup_state.h"
bool clearra_buildup_bfs_state_has_deleted_line_state(
    const clr_buildup_bfs_state *state) {
    return state != 0;
}bool clearra_buildup_bfs_state_has_hold_automaton_state(
    const clr_buildup_bfs_state *state) {
    return state != 0 && state->hold_automaton_state.piece_source_id != 0u;
}clr_buildup_status clearra_buildup_remaining_ops_bitset_for_count(
    uint16_t operation_count,
    uint16_t *out_bitset) {
    if (out_bitset == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    *out_bitset = 0u;
    if (operation_count == 0u || operation_count > CLR_BUILDUP_MAX_OPERATIONS) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    *out_bitset = (uint16_t)((UINT16_C(1) << operation_count) - UINT16_C(1));
    return CLR_BUILDUP_OK;
}clr_buildup_bfs_state clearra_buildup_bfs_state_from_state(
    const ClearraBuildUpState *state,
    uint16_t remaining_ops_bitset) {
    clr_buildup_bfs_state bfs = {0};
    if (state == 0) {
        return bfs;
    }
    bfs.remaining_ops_bitset = remaining_ops_bitset;
    bfs.current_board_mask = state->board_mask;
    bfs.deleted_line_state.deleted_row_mask =
        state->line_clear_state.deleted_row_mask;
    bfs.deleted_line_state.deleted_count = state->line_clear_state.deleted_count;
    bfs.hold_automaton_state = state->hold_automaton_state;
    bfs.piece_source_cursor = state->hold_automaton_state.cursor;
    bfs.reachability_relevant_state = state->reachability_relevant_state;
    bfs.cleared_lines = state->cleared_lines;
    return bfs;
}

#include "buildup_internal.h"

ClearraBuildUpBranchOutcome clearra_buildup_branch_outcome_for_status(
    clr_buildup_status status) {
    if (status == CLR_BUILDUP_OK) {
        return CLEARRA_BUILDUP_BRANCH_SUCCESS;
    }
    if (status == CLR_BUILDUP_CAPACITY_EXCEEDED ||
        status == CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED ||
        status == CLR_BUILDUP_ENUMERATION_TRUNCATED ||
        status == CLR_BUILDUP_CANCELLED) {
        return CLEARRA_BUILDUP_BRANCH_INCOMPLETE;
    }
    if (status == CLR_BUILDUP_LINE_CLEAR_DEPENDENCY_IMPOSSIBLE ||
        status == CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE ||
        status == CLR_BUILDUP_NOT_GROUNDED ||
        status == CLR_BUILDUP_REACHABILITY_IMPOSSIBLE ||
        status == CLR_BUILDUP_QUEUE_ORDER_IMPOSSIBLE ||
        status == CLR_BUILDUP_HOLD_DISABLED_IMPOSSIBLE ||
        status == CLR_BUILDUP_BAG_PATTERN_IMPOSSIBLE ||
        status == CLR_BUILDUP_PIECE_WINDOW_IMPOSSIBLE ||
        status == CLR_BUILDUP_GOAL_NOT_SATISFIED ||
        status == CLR_BUILDUP_COLLISION) {
        return CLEARRA_BUILDUP_BRANCH_LOGICAL_REJECT;
    }
    return CLEARRA_BUILDUP_BRANCH_FATAL;
}

#include "buildup_search.h"

clr_buildup_status clearra_buildup_verify_piece_window(
    const clr_buildup_problem *problem) {
    return clearra_buildup_verify_piece_window_for_count(
        problem, problem == 0 ? 0u : problem->operation_set.operation_count);
}

clr_buildup_status clearra_buildup_verify_piece_window_for_count(
    const clr_buildup_problem *problem,
    uint16_t count) {
    if (problem == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (count == 0 || count > problem->piece_window.max_pieces) {
        return CLR_BUILDUP_PIECE_WINDOW_IMPOSSIBLE;
    }
    if (problem->piece_window.has_exact_pieces &&
        count != problem->piece_window.exact_pieces) {
        return CLR_BUILDUP_PIECE_WINDOW_IMPOSSIBLE;
    }
    return CLR_BUILDUP_OK;
}

#include "buildup_search_internal.h"

static clr_buildup_status initialize_search_context(
    const clr_buildup_problem *problem,
    const ClearraBuildUpOperationSource *operation_source,
    const ClearraCompactRuleProfile *compiled_rule,
    ClearraBuildUpSearchContext *out_context) {
    if (problem == 0 || operation_source == 0 || out_context == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    *out_context = (ClearraBuildUpSearchContext){0};
    out_context->problem = problem;
    out_context->operation_source = *operation_source;
    out_context->cache_identity_hash = clearra_cache_identity_hash(
        clearra_cache_identity_from_packing_problem(&problem->packing, 1u));
    out_context->first_failure = CLR_BUILDUP_OK;
    out_context->first_failure_step = UINT16_MAX;
    out_context->max_count_variants = UINT64_MAX;
    out_context->max_retained_variants = CLR_BUILDUP_MAX_VARIANTS;
    clr_buildup_status status = clearra_buildup_search_layout_from_problem(
        problem, &out_context->layout);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    status = clearra_buildup_operation_source_order(
        &out_context->operation_source, &out_context->order);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    if (compiled_rule == 0) {
        ClearraReachabilityStatus reachability_status =
            clearra_reachability_compile_rule(
                &problem->rule, &out_context->owned_compiled_rule);
        if (reachability_status != CLEARRA_REACHABILITY_OK) {
            return clearra_buildup_status_from_reachability_status(
                reachability_status);
        }
        out_context->compiled_rule = &out_context->owned_compiled_rule;
    } else {
        out_context->compiled_rule = compiled_rule;
    }
    out_context->reachability_mode =
        clearra_buildup_reachability_mode_for_rule(&problem->rule);
    return out_context->reachability_mode == 0u
               ? CLR_BUILDUP_INVALID_PROBLEM
               : CLR_BUILDUP_OK;
}

clr_buildup_status clearra_buildup_search_context_init_with_reachability(
    const clr_buildup_problem *problem,
    const ClearraCompactRuleProfile *compiled_rule,
    ClearraBuildUpSearchContext *out_context) {
    ClearraBuildUpOperationSource source;
    clr_buildup_status status =
        clearra_buildup_operation_source_from_problem(problem, &source);
    return status == CLR_BUILDUP_OK
               ? initialize_search_context(
                     problem, &source, compiled_rule, out_context)
               : status;
}

clr_buildup_status clearra_buildup_search_context_init_catalog_rows(
    const clr_buildup_problem *problem,
    const ClearraGeometryCatalog *catalog,
    const uint32_t *row_ids,
    uint16_t operation_count,
    const uint8_t *representative_order_hint,
    const uint16_t *required_predecessors,
    const ClearraCompactRuleProfile *compiled_rule,
    ClearraBuildUpSearchContext *out_context) {
    ClearraBuildUpOperationSource source;
    clr_buildup_status status =
        clearra_buildup_operation_source_from_catalog_rows(
            problem,
            catalog,
            row_ids,
            operation_count,
            representative_order_hint,
            required_predecessors,
            &source);
    return status == CLR_BUILDUP_OK
               ? initialize_search_context(
                     problem, &source, compiled_rule, out_context)
               : status;
}

clr_buildup_status clearra_buildup_search_context_init(
    const clr_buildup_problem *problem,
    ClearraBuildUpSearchContext *out_context) {
    return clearra_buildup_search_context_init_with_reachability(
        problem, 0, out_context);
}

#include "buildup_search_internal.h"

void clearra_buildup_search_record_failure(
    ClearraBuildUpSearchContext *context,
    clr_buildup_status status,
    uint16_t step) {
    if (context == 0 || status == CLR_BUILDUP_OK) {
        return;
    }
    ClearraBuildUpBranchOutcome outcome =
        clearra_buildup_branch_outcome_for_status(status);
    if (outcome == CLEARRA_BUILDUP_BRANCH_INCOMPLETE) {
        context->incomplete_branch_seen = 1u;
    } else if (outcome == CLEARRA_BUILDUP_BRANCH_FATAL) {
        context->fatal_branch_seen = 1u;
    }
    if (context->first_failure == CLR_BUILDUP_OK) {
        context->first_failure = status;
        context->first_failure_step = step;
    }
}

#include "buildup_search_internal.h"
static clr_buildup_memo_key search_memo_key(
    const ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations) {
    uint64_t cache_identity_hash = context->cache_identity_hash;
    if (cache_identity_hash == 0u && context->problem != 0) {
        cache_identity_hash = clearra_cache_identity_hash(
            clearra_cache_identity_from_packing_problem(
                &context->problem->packing, 1u));
    }
    clr_buildup_bfs_state bfs_state =
        clearra_buildup_bfs_state_from_state(state, remaining_operations);
    return clearra_buildup_memo_key_from_bfs_state_hash(
        cache_identity_hash, &bfs_state);
}

bool clearra_buildup_search_completion_memo_lookup(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations,
    uint64_t *out_completion_count) {
    if (context == 0 || state == 0 || out_completion_count == 0) {
        return false;
    }
    clr_search_profile_span memo_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_MEMO_LOOKUP);
    clr_buildup_memo_key key = search_memo_key(context, state, remaining_operations);
    uint64_t probes_before = context->completion_memo.probes;
    bool found = clearra_buildup_completion_memo_lookup(
        &context->completion_memo, &key, out_completion_count);
    (void)clr_search_profile_end(
        memo_span, context->completion_memo.probes - probes_before);
    return found;
}

void clearra_buildup_search_completion_memo_insert(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations,
    uint64_t completion_count) {
    if (context == 0 || state == 0) {
        return;
    }
    clr_search_profile_span memo_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_MEMO_INSERT);
    if (context->incomplete_branch_seen != 0u ||
        context->fatal_branch_seen != 0u) {
        (void)clr_search_profile_end(memo_span, 0u);
        return;
    }
    clr_buildup_memo_key key = search_memo_key(context, state, remaining_operations);
    uint64_t probes_before = context->completion_memo.probes;
    clearra_buildup_completion_memo_insert(
        &context->completion_memo, &key, completion_count);
    (void)clr_search_profile_end(
        memo_span, context->completion_memo.probes - probes_before);
}

bool clearra_buildup_search_failed_memo_contains(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations) {
    uint64_t completion_count = 0u;
    return clearra_buildup_search_completion_memo_lookup(
               context, state, remaining_operations, &completion_count) &&
           completion_count == 0u;
}

void clearra_buildup_search_failed_memo_insert(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations) {
    clearra_buildup_search_completion_memo_insert(
        context, state, remaining_operations, 0u);
}

#include "buildup_search_internal.h"

clr_buildup_status clearra_buildup_search_verify_goal(
    const clr_buildup_problem *problem,
    const ClearraBuildUpState *state) {
    if (problem->goal == CLR_GOAL_CLEAR_TO_EMPTY && state->board_mask == 0) {
        return CLR_BUILDUP_OK;
    }
    return CLR_BUILDUP_GOAL_NOT_SATISFIED;
}

#include "buildup_search_internal.h"

clr_buildup_status clearra_buildup_search_layout_from_problem(
    const clr_buildup_problem *problem,
    ClearraBoard64Layout *out_layout) {
    if (problem == 0 || out_layout == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    uint16_t height = problem->initial_board.search_height;
    if (height == 0) {
        height = problem->initial_board.visible_height;
    }
    if (problem->initial_board.width > UINT8_MAX || height > UINT8_MAX) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    return clearra_board64_make_layout(
               (uint8_t)problem->initial_board.width,
               (uint8_t)height,
               out_layout) == CLEARRA_BOARD64_OK
               ? CLR_BUILDUP_OK
               : CLR_BUILDUP_INVALID_PROBLEM;
}

#include "buildup_search_internal.h"
static uint8_t count_bits16(uint16_t value) {
    uint8_t count = 0;
    while (value != 0u) {
        count = (uint8_t)(count + (uint8_t)(value & 1u));
        value = (uint16_t)(value >> 1u);
    }
    return count;
}static bool original_row_for_current_row(
    ClearraLineClearState line_clear_state,
    uint8_t current_row,
    uint8_t *out_original_row) {
    if (out_original_row == 0) {
        return false;
    }
    uint8_t visible_current_row = 0;
    for (uint8_t original_row = 0; original_row < 16u; original_row++) {
        uint16_t bit = (uint16_t)(UINT16_C(1) << original_row);
        if ((line_clear_state.deleted_row_mask & bit) != 0u) {
            continue;
        }
        if (visible_current_row == current_row) {
            *out_original_row = original_row;
            return true;
        }
        visible_current_row++;
    }
    return false;
}clr_buildup_status clearra_buildup_search_update_line_clear_state(
    ClearraBuildUpState *state,
    ClearraBoard64LineClearResult clear_result) {
    if (state == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (clear_result.cleared_lines == 0u) {
        return CLR_BUILDUP_OK;
    }
    if (clear_result.cleared_lines != count_bits16(clear_result.deleted_row_mask)) {
        return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
    }
    uint16_t original_deleted_rows = 0;
    for (uint8_t current_row = 0; current_row < 16u; current_row++) {
        uint16_t bit = (uint16_t)(UINT16_C(1) << current_row);
        if ((clear_result.deleted_row_mask & bit) == 0u) {
            continue;
        }
        uint8_t original_row = 0;
        if (!original_row_for_current_row(
                state->line_clear_state, current_row, &original_row)) {
            return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
        }
        original_deleted_rows |= (uint16_t)(UINT16_C(1) << original_row);
    }
    state->line_clear_state.deleted_row_mask =
        (uint16_t)(state->line_clear_state.deleted_row_mask | original_deleted_rows);
    state->line_clear_state.deleted_count =
        (uint8_t)(state->line_clear_state.deleted_count + clear_result.cleared_lines);
    state->cleared_lines =
        (uint8_t)(state->cleared_lines + clear_result.cleared_lines);
    return state->line_clear_state.deleted_count ==
                   count_bits16(state->line_clear_state.deleted_row_mask)
               ? CLR_BUILDUP_OK
               : CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
}

#include "buildup_search_internal.h"

#include "buildup_search_internal.h"

clr_buildup_status clearra_buildup_search_record_success(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state) {
    if (context == 0 || state == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (context->enumerated_variant_count == UINT64_MAX) {
        context->incomplete_branch_seen = 1u;
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    context->success_state = *state;
    context->enumerated_variant_count++;
    if (context->enumerated_variant_count > context->max_count_variants) {
        return CLR_BUILDUP_ENUMERATION_TRUNCATED;
    }
    clearra_buildup_capture_success_path(context, state->placed_pieces);
    if (context->out_variants != 0 &&
        context->out_variants->count < context->max_retained_variants) {
        clr_buildup_verification verification = {0};
        verification.accepted = 1u;
        verification.rejected_step = UINT16_MAX;
        verification.reject_reason = CLR_BUILDUP_OK;
        clearra_build_variant_from_state(
            context->problem, state, &verification.variant);
        clearra_buildup_attach_success_trace(context, &verification.variant);
        verification.variant.build_variant_id =
            context->enumerated_variant_count;
        return clr_build_variant_buffer_push_verified(
            context->out_variants, &verification);
    }
    return CLR_BUILDUP_OK;
}
