#include "buildup_internal.h"
static uint64_t variant_operation_set_hash(const clr_buildup_problem *problem) {
    uint64_t hash = UINT64_C(1469598103934665603);
    for (uint16_t index = 0; index < problem->operation_set.operation_count; index++) {
        const clr_buildup_operation *operation = &problem->operation_set.operations[index];
        hash ^= operation->mask;
        hash *= UINT64_C(1099511628211);
        hash ^= operation->operation_id;
        hash *= UINT64_C(1099511628211);
        hash ^= operation->required_deleted_row_mask;
        hash *= UINT64_C(1099511628211);
    }
    return hash;
}void clearra_build_variant_from_state(
    const clr_buildup_problem *problem,
    const ClearraBuildUpState *state,
    clr_build_variant_view *out_variant) {
    if (out_variant == 0) {
        return;
    }
    *out_variant = (clr_build_variant_view){0};
    if (problem == 0 || state == 0) {
        return;
    }

    out_variant->candidate_id = problem->candidate_id;
    out_variant->canonical_operation_set_id =
        problem->canonical_operation_set_id;
    out_variant->operation_set_hash = variant_operation_set_hash(problem);
    out_variant->final_board = state->board_mask;
    out_variant->coverage_pattern_id = problem->coverage_pattern_id;
    out_variant->placed_count = state->placed_pieces;
    out_variant->queue_cursor = state->hold_automaton_state.cursor;
    out_variant->hold_piece = state->hold_automaton_state.hold_piece;
    out_variant->hold_empty = state->hold_automaton_state.hold_empty;
    out_variant->cleared_lines = state->cleared_lines;
    out_variant->hold_branch_kind = state->last_hold_branch_kind;
}void clr_build_variant_buffer_clear(clr_build_variant_buffer *buffer) {
    if (buffer == 0) {
        return;
    }
    buffer->count = 0u;
    buffer->reserved = 0u;
    buffer->total_variant_count = 0u;
    buffer->count_complete = 0u;
    buffer->trace_retention_truncated = 0u;
    buffer->search_metrics = (clr_buildup_search_metrics){0};
    for (uint8_t index = 0u; index < sizeof(buffer->reserved2); ++index) {
        buffer->reserved2[index] = 0u;
    }
}clr_buildup_status clr_build_variant_buffer_push_verified(
    clr_build_variant_buffer *buffer,
    const clr_buildup_verification *verification) {
    if (buffer == 0 || verification == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (!verification->accepted) {
        return (clr_buildup_status)verification->reject_reason;
    }
    if (buffer->count >= CLR_BUILDUP_MAX_VARIANTS) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }

    uint16_t variant_index = buffer->count;
    uint32_t evidence_count = verification->variant.kick_evidence_count;
    uint16_t operation_order_count =
        verification->variant.operation_order_count;
    uint16_t trace_step_count = verification->variant.trace_step_count;
    if (evidence_count > CLR_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT) {
        return CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED;
    }
    if (evidence_count > 0u && verification->variant.kick_evidence == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (operation_order_count > CLR_BUILDUP_MAX_OPERATIONS ||
        trace_step_count > CLR_BUILDUP_MAX_OPERATIONS ||
        operation_order_count != trace_step_count) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (operation_order_count > 0u &&
        (verification->variant.operation_order_ids == 0 ||
         verification->variant.trace_steps == 0)) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }

    for (uint32_t index = 0; index < evidence_count; index++) {
        buffer->kick_evidence_storage[variant_index][index] =
            verification->variant.kick_evidence[index];
    }
    for (uint16_t index = 0; index < operation_order_count; index++) {
        buffer->operation_order_storage[variant_index][index] =
            verification->variant.operation_order_ids[index];
        buffer->trace_step_storage[variant_index][index] =
            verification->variant.trace_steps[index];
    }

    buffer->variants[variant_index] = verification->variant;
    if (evidence_count > 0u) {
        buffer->variants[variant_index].kick_evidence =
            buffer->kick_evidence_storage[variant_index];
    } else {
        buffer->variants[variant_index].kick_evidence = 0;
    }
    buffer->variants[variant_index].kick_evidence_count = evidence_count;
    if (operation_order_count > 0u) {
        buffer->variants[variant_index].operation_order_ids =
            buffer->operation_order_storage[variant_index];
        buffer->variants[variant_index].trace_steps =
            buffer->trace_step_storage[variant_index];
    } else {
        buffer->variants[variant_index].operation_order_ids = 0;
        buffer->variants[variant_index].trace_steps = 0;
    }
    buffer->variants[variant_index].operation_order_count =
        operation_order_count;
    buffer->variants[variant_index].trace_step_count = trace_step_count;
    buffer->count++;
    buffer->total_variant_count = buffer->count;
    buffer->count_complete = 1u;
    buffer->trace_retention_truncated = 0u;
    return CLR_BUILDUP_OK;
}
