#include "buildup_geometry_transition_cache.h"
#include "buildup_reachability_cache.h"
#include "buildup_reachable_lock_cache.h"
#include "buildup_search_internal.h"
#include "clr_search_profile.h"

clr_buildup_status clearra_buildup_search_try_operation(
    ClearraBuildUpSearchContext *context,
    ClearraBuildUpState state,
    ClearraBuildUpQueueHold queue_hold,
    const clr_buildup_operation *operation,
    uint16_t operation_index,
    ClearraBuildUpState *out_next_state,
    clr_buildup_trace_step *out_trace_step,
    clr_kick_evidence_view *out_kick_evidence) {
    if (context == 0 || operation == 0 || out_next_state == 0 ||
        out_trace_step == 0 || out_kick_evidence == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    const ClearraBuildUpState input_state = state;
    *out_trace_step = (clr_buildup_trace_step){0};
    *out_kick_evidence = (clr_kick_evidence_view){0};
    ClearraBuildUpGeometryTransitionResult cached;
    clr_search_profile_count(
        CLR_PROFILE_BUILDUP_GEOMETRY_TRANSITION_CACHE_LOOKUPS, 1u);
    if (clearra_buildup_geometry_transition_cache_lookup(
            context->geometry_transition_cache,
            &input_state,
            operation,
            context->reachability_trace_mode,
            &cached)) {
        clr_search_profile_count(
            CLR_PROFILE_BUILDUP_GEOMETRY_TRANSITION_CACHE_HITS, 1u);
        clr_buildup_status cached_status = (clr_buildup_status)cached.status;
        if (cached_status != CLR_BUILDUP_OK) {
            return cached_status;
        }
        *out_trace_step = cached.trace_step;
        out_trace_step->operation_id = operation->operation_id;
        out_trace_step->operation_index = operation_index;
        out_trace_step->piece = operation->piece;
        out_trace_step->rotation = operation->rotation;
        out_trace_step->target_frame_mask = operation->mask;
        *out_kick_evidence = cached.kick_evidence;
        state.board_mask = cached.board_mask;
        state.line_clear_state = cached.line_clear_state;
        state.placed_pieces++;
        state.hold_automaton_state = queue_hold;
        state.reachability_relevant_state = cached.reachability_relevant_state;
        state.cleared_lines = cached.cleared_lines;
        *out_next_state = state;
        return CLR_BUILDUP_OK;
    }
    uint64_t adjusted_mask = 0u;
    int8_t adjusted_y = 0;
    clr_search_profile_span y_adjustment_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_Y_ADJUSTMENT);
    clr_buildup_status status = clearra_buildup_adjust_operation_for_line_clears(
        context->layout, state, operation, &adjusted_mask, &adjusted_y);
    (void)clr_search_profile_end(y_adjustment_span, 1u);
    if (status != CLR_BUILDUP_OK) {
        clearra_buildup_geometry_transition_cache_insert(
            context->geometry_transition_cache,
            &input_state,
            operation,
            status,
            0,
            0,
            0,
            context->reachability_trace_mode);
        return status;
    }

    clr_search_profile_span line_dependency_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_LINE_DEPENDENCY);
    status = clearra_buildup_check_line_clear_dependency(
        context->layout, state.board_mask, adjusted_mask);
    (void)clr_search_profile_end(line_dependency_span, 1u);
    if (status != CLR_BUILDUP_OK) {
        clearra_buildup_geometry_transition_cache_insert(
            context->geometry_transition_cache,
            &input_state,
            operation,
            status,
            0,
            0,
            0,
            context->reachability_trace_mode);
        return status;
    }

    clr_search_profile_span grounded_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_GROUNDED);
    status = clearra_buildup_grounded_filter_accepts(
        context->layout, state.board_mask, adjusted_mask);
    (void)clr_search_profile_end(grounded_span, 1u);
    if (status != CLR_BUILDUP_OK) {
        clearra_buildup_geometry_transition_cache_insert(
            context->geometry_transition_cache,
            &input_state,
            operation,
            status,
            0,
            0,
            0,
            context->reachability_trace_mode);
        return status;
    }

    ClearraBuildUpReachabilityResult reachability_result;
    clr_search_profile_count(
        CLR_PROFILE_BUILDUP_REACHABILITY_CACHE_LOOKUPS, 1u);
    bool reachability_cache_hit = false;
    if (context->capture_trace == 0u &&
        context->reachable_lock_cache != 0) {
        clr_search_profile_span reachability_span =
            clr_search_profile_begin(CLR_PROFILE_BUILDUP_REACHABILITY);
        status = clearra_buildup_reachable_lock_cache_check(
            context->reachable_lock_cache,
            context->layout,
            state.board_mask,
            operation,
            adjusted_y,
            context->compiled_rule,
            context->reachability_mode,
            context->reachability_frontier,
            &reachability_cache_hit,
            &reachability_result);
        (void)clr_search_profile_end(reachability_span, 1u);
    } else {
        reachability_cache_hit = clearra_buildup_reachability_cache_lookup(
            context->reachability_cache,
            state.board_mask,
            operation,
            adjusted_y,
            context->reachability_mode,
            context->reachability_trace_mode,
            &status,
            &reachability_result);
        if (!reachability_cache_hit) {
            clr_search_profile_span reachability_span =
                clr_search_profile_begin(CLR_PROFILE_BUILDUP_REACHABILITY);
            status = clearra_buildup_reachability_check_compiled(
                context->compiled_rule,
                context->layout,
                state.board_mask,
                operation,
                adjusted_y,
                context->reachability_mode,
                context->reachability_trace_mode,
                context->reachability_frontier,
                &reachability_result);
            (void)clr_search_profile_end(reachability_span, 1u);
            clearra_buildup_reachability_cache_insert(
                context->reachability_cache,
                state.board_mask,
                operation,
                adjusted_y,
                context->reachability_mode,
                context->reachability_trace_mode,
                status,
                &reachability_result);
        }
    }
    if (reachability_cache_hit) {
        clr_search_profile_count(
            CLR_PROFILE_BUILDUP_REACHABILITY_CACHE_HITS, 1u);
    }
    if (status != CLR_BUILDUP_OK) {
        clearra_buildup_geometry_transition_cache_insert(
            context->geometry_transition_cache,
            &input_state,
            operation,
            status,
            0,
            0,
            0,
            context->reachability_trace_mode);
        return status;
    }

    uint64_t placed_board = 0u;
    clr_search_profile_span place_clear_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_PLACE_AND_CLEAR);
    ClearraBoard64Status board_status = clearra_board64_place(
        context->layout, state.board_mask, adjusted_mask, &placed_board);
    if (board_status == CLEARRA_BOARD64_COLLISION) {
        (void)clr_search_profile_end(place_clear_span, 1u);
        clearra_buildup_geometry_transition_cache_insert(
            context->geometry_transition_cache,
            &input_state,
            operation,
            CLR_BUILDUP_COLLISION,
            0,
            0,
            0,
            context->reachability_trace_mode);
        return CLR_BUILDUP_COLLISION;
    }
    if (board_status != CLEARRA_BOARD64_OK) {
        (void)clr_search_profile_end(place_clear_span, 1u);
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    ClearraBoard64LineClearResult clear_result;
    if (clearra_board64_clear_lines(
            context->layout, placed_board, &clear_result) !=
        CLEARRA_BOARD64_OK) {
        (void)clr_search_profile_end(place_clear_span, 1u);
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    (void)clr_search_profile_end(place_clear_span, 1u);

    if (context->capture_trace) {
        clearra_buildup_trace_step_from_operation(
            context->problem,
            operation,
            operation_index,
            adjusted_y,
            clear_result,
            &reachability_result,
            out_trace_step,
            out_kick_evidence);
    }
    state.board_mask = clear_result.board;
    clr_search_profile_span line_state_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_LINE_STATE_UPDATE);
    status = clearra_buildup_search_update_line_clear_state(&state, clear_result);
    (void)clr_search_profile_end(line_state_span, 1u);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    state.placed_pieces++;
    state.hold_automaton_state = queue_hold;
    state.reachability_relevant_state = state.board_mask;
    clearra_buildup_geometry_transition_cache_insert(
        context->geometry_transition_cache,
        &input_state,
        operation,
        CLR_BUILDUP_OK,
        &state,
        out_trace_step,
        out_kick_evidence,
        context->reachability_trace_mode);
    *out_next_state = state;
    return CLR_BUILDUP_OK;
}
