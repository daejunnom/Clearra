#include "buildup_search_internal.h"
uint64_t clearra_buildup_reachability_path_digest(
    const ClearraReachabilityReport *report) {
    if (report == 0 || !report->reachable) {
        return 0u;
    }
    uint64_t hash = UINT64_C(1469598103934665603);
    for (uint8_t index = 0; index < report->debug_step_count; index++) {
        const ClearraReachabilityDebugStep *step = &report->debug_steps[index];
        hash ^= step->rotation;
        hash *= UINT64_C(1099511628211);
        hash ^= (uint8_t)step->x;
        hash *= UINT64_C(1099511628211);
        hash ^= (uint8_t)step->y;
        hash *= UINT64_C(1099511628211);
        hash ^= step->transition_kind;
        hash *= UINT64_C(1099511628211);
    }
    hash = clearra_cache_key_mix_u64(hash, report->path_complete);
    hash = clearra_cache_key_mix_u64(hash, report->has_rotation_evidence);
    hash = clearra_cache_key_mix_u64(hash, report->first_success_confirmed);
    if (report->has_rotation_evidence) {
        hash = clearra_cache_key_mix_u64(hash, report->rotation_from);
        hash = clearra_cache_key_mix_u64(hash, report->rotation_to);
        hash = clearra_cache_key_mix_u64(hash, report->rotation_request);
        hash = clearra_cache_key_mix_u64(hash, report->kick_index);
        hash = clearra_cache_key_mix_u64(hash, (uint8_t)report->kick_dx);
        hash = clearra_cache_key_mix_u64(hash, (uint8_t)report->kick_dy);
        hash = clearra_cache_key_mix_u64(hash, (uint8_t)report->predecessor_x);
        hash = clearra_cache_key_mix_u64(hash, (uint8_t)report->predecessor_y);
        hash = clearra_cache_key_mix_u64(hash, (uint8_t)report->result_x);
        hash = clearra_cache_key_mix_u64(hash, (uint8_t)report->result_y);
    }
    return hash == 0u ? 1u : hash;
}
void clearra_buildup_trace_step_from_operation(
    const clr_buildup_problem *problem,
    const clr_buildup_operation *operation,
    uint16_t operation_index,
    int8_t adjusted_y,
    ClearraBoard64LineClearResult clear_result,
    const ClearraBuildUpReachabilityResult *reachability,
    clr_buildup_trace_step *out_step,
    clr_kick_evidence_view *out_kick_evidence) {
    if (out_step == 0 || out_kick_evidence == 0) {
        return;
    }
    *out_step = (clr_buildup_trace_step){0};
    *out_kick_evidence = (clr_kick_evidence_view){0};
    out_step->kick_evidence_index = UINT8_MAX;
    if (problem == 0 || operation == 0 || reachability == 0) {
        return;
    }
    bool reachable = clearra_buildup_reachability_result_has_flag(
        reachability, CLEARRA_BUILDUP_REACHABLE_FLAG);
    bool used_kick = clearra_buildup_reachability_result_has_flag(
        reachability, CLEARRA_BUILDUP_USED_KICK_FLAG);
    bool used_180 = clearra_buildup_reachability_result_has_flag(
        reachability, CLEARRA_BUILDUP_USED_180_FLAG);
    bool first_success = clearra_buildup_reachability_result_has_flag(
        reachability, CLEARRA_BUILDUP_FIRST_SUCCESS_FLAG);
    bool path_complete = clearra_buildup_reachability_result_has_flag(
        reachability, CLEARRA_BUILDUP_PATH_COMPLETE_FLAG);
    bool last_action_was_rotation =
        clearra_buildup_reachability_result_has_flag(
            reachability, CLEARRA_BUILDUP_LAST_ACTION_ROTATION_FLAG);
    out_step->operation_id = operation->operation_id;
    out_step->operation_index = operation_index;
    out_step->piece = operation->piece;
    out_step->rotation = operation->rotation;
    out_step->adjusted_x = operation->x;
    out_step->adjusted_y = adjusted_y;
    out_step->cleared_row_mask = clear_result.deleted_row_mask;
    out_step->target_frame_mask = operation->mask;
    out_step->reachability.reachable = reachable ? 1u : 0u;
    out_step->reachability.exhaustive = 1u;
    out_step->reachability.used_kick = used_kick ? 1u : 0u;
    out_step->reachability.used_180 = used_180 ? 1u : 0u;
    out_step->reachability.visited_states = reachability->visited_states;
    out_step->reachability.last_action_was_rotation =
        last_action_was_rotation ? 1u : 0u;
    out_step->reachability.rotation_evidence_complete =
        (uint8_t)(path_complete &&
                  (!last_action_was_rotation || first_success));
    out_step->reachability.path_digest = reachability->path_digest;
    if (last_action_was_rotation && path_complete && first_success) {
        out_kick_evidence->has_kick_evidence = 1u;
        out_kick_evidence->from_rotation = reachability->rotation_from;
        out_kick_evidence->to_rotation = reachability->rotation_to;
        out_kick_evidence->rotation_request = reachability->rotation_request;
        out_kick_evidence->kick_index = reachability->kick_index;
        out_kick_evidence->kick_dx = reachability->kick_dx;
        out_kick_evidence->kick_dy = reachability->kick_dy;
        out_kick_evidence->kick_table_id = problem->rule.kick_profile_id;
        out_kick_evidence->kick_profile_id = problem->rule.kick_profile_id;
        out_kick_evidence->first_success_confirmed =
            first_success ? 1u : 0u;
        out_kick_evidence->predecessor_x = reachability->predecessor_x;
        out_kick_evidence->predecessor_y = reachability->predecessor_y;
        out_kick_evidence->result_x = reachability->result_x;
        out_kick_evidence->result_y = reachability->result_y;
    }
}
uint64_t clearra_buildup_trace_identity(
    const clr_buildup_trace_step *steps,
    uint16_t step_count) {
    if (steps == 0 || step_count == 0u ||
        step_count > CLR_BUILDUP_MAX_OPERATIONS) {
        return 0u;
    }
    uint64_t hash = UINT64_C(1469598103934665603);
    for (uint16_t index = 0; index < step_count; index++) {
        const clr_buildup_trace_step *step = &steps[index];
        hash = clearra_cache_key_mix_u64(hash, step->operation_id);
        hash = clearra_cache_key_mix_u64(hash, step->operation_index);
        hash = clearra_cache_key_mix_u64(hash, step->piece);
        hash = clearra_cache_key_mix_u64(hash, step->rotation);
        hash = clearra_cache_key_mix_u64(hash, step->hold_branch_kind);
        hash = clearra_cache_key_mix_u64(hash, step->used_hold);
        hash = clearra_cache_key_mix_u64(hash, step->incoming_piece);
        hash = clearra_cache_key_mix_u64(hash, step->held_piece_before);
        hash = clearra_cache_key_mix_u64(hash, step->hold_empty_before);
        hash = clearra_cache_key_mix_u64(hash, (uint8_t)step->adjusted_x);
        hash = clearra_cache_key_mix_u64(hash, (uint8_t)step->adjusted_y);
        hash = clearra_cache_key_mix_u64(hash, step->cleared_row_mask);
        hash = clearra_cache_key_mix_u64(hash, step->target_frame_mask);
        hash = clearra_cache_key_mix_u64(hash, step->reachability.reachable);
        hash = clearra_cache_key_mix_u64(hash, step->reachability.exhaustive);
        hash = clearra_cache_key_mix_u64(hash, step->reachability.used_kick);
        hash = clearra_cache_key_mix_u64(hash, step->reachability.used_180);
        hash = clearra_cache_key_mix_u64(hash, step->reachability.visited_states);
        hash = clearra_cache_key_mix_u64(hash, step->reachability.path_digest);
        hash = clearra_cache_key_mix_u64(hash, step->kick_evidence_index);
    }
    return hash == 0u ? 1u : hash;
}
uint64_t clearra_buildup_trace_operation_set_hash(
    const clr_buildup_trace_step *steps,
    uint16_t step_count) {
    if (steps == 0 || step_count == 0u ||
        step_count > CLR_BUILDUP_MAX_OPERATIONS) {
        return 0u;
    }
    uint64_t descriptors[CLR_BUILDUP_MAX_OPERATIONS];
    for (uint16_t index = 0u; index < step_count; ++index) {
        const clr_buildup_trace_step *step = &steps[index];
        uint64_t descriptor = UINT64_C(1469598103934665603);
        descriptor = clearra_cache_key_mix_u64(descriptor, step->operation_id);
        descriptor = clearra_cache_key_mix_u64(descriptor, step->piece);
        descriptor = clearra_cache_key_mix_u64(descriptor, step->rotation);
        descriptor = clearra_cache_key_mix_u64(
            descriptor, (uint8_t)step->adjusted_x);
        descriptor = clearra_cache_key_mix_u64(
            descriptor, (uint8_t)step->adjusted_y);
        descriptors[index] = descriptor;
    }
    for (uint16_t index = 1u; index < step_count; ++index) {
        uint64_t value = descriptors[index];
        uint16_t cursor = index;
        while (cursor > 0u && descriptors[cursor - 1u] > value) {
            descriptors[cursor] = descriptors[cursor - 1u];
            --cursor;
        }
        descriptors[cursor] = value;
    }
    uint64_t hash = UINT64_C(1469598103934665603);
    hash = clearra_cache_key_mix_u64(hash, step_count);
    for (uint16_t index = 0u; index < step_count; ++index) {
        hash = clearra_cache_key_mix_u64(hash, descriptors[index]);
    }
    return hash == 0u ? 1u : hash;
}
void clearra_buildup_apply_kick_trace_completeness(
    const clr_buildup_trace_step *steps,
    uint16_t step_count,
    clr_build_variant_view *variant) {
    if (steps == 0 || variant == 0) {
        return;
    }
    uint8_t missing = 0u;
    for (uint16_t index = 0; index < step_count; index++) {
        const clr_reachability_evidence_view *evidence =
            &steps[index].reachability;
        if (evidence->last_action_was_rotation != 0u &&
            evidence->rotation_evidence_complete == 0u) {
            missing = 1u;
            break;
        }
    }
    if (missing != 0u) {
        variant->trace_completeness_flags |=
            CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING;
    } else {
        variant->trace_completeness_flags &=
            ~CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING;
    }
}
void clearra_buildup_capture_success_path(
    ClearraBuildUpSearchContext *context,
    uint16_t step_count) {
    if (context == 0 || !context->capture_trace ||
        step_count > CLR_BUILDUP_MAX_OPERATIONS) {
        return;
    }
    context->success_trace_step_count = step_count;
    context->success_kick_evidence_count = 0u;
    for (uint16_t index = 0; index < step_count; index++) {
        context->success_trace_steps[index] = context->current_trace_steps[index];
        context->success_operation_order_ids[index] =
            context->current_trace_steps[index].operation_id;
        context->success_trace_steps[index].kick_evidence_index = UINT8_MAX;
        if (context->current_kick_evidence[index].has_kick_evidence != 0u) {
            context->success_trace_steps[index].kick_evidence_index =
                (uint8_t)context->success_kick_evidence_count;
            context->success_kick_evidence
                [context->success_kick_evidence_count++] =
                context->current_kick_evidence[index];
        }
    }
}
void clearra_buildup_attach_success_trace(
    const ClearraBuildUpSearchContext *context,
    clr_build_variant_view *variant) {
    if (context == 0 || variant == 0 || !context->capture_trace) {
        return;
    }
    variant->operation_order_ids = context->success_operation_order_ids;
    variant->operation_order_count = context->success_trace_step_count;
    variant->trace_steps = context->success_trace_steps;
    variant->trace_step_count = context->success_trace_step_count;
    variant->trace_identity = clearra_buildup_trace_identity(
        context->success_trace_steps, context->success_trace_step_count);
    variant->operation_set_hash = clearra_buildup_trace_operation_set_hash(
        context->success_trace_steps, context->success_trace_step_count);
    variant->kick_evidence = context->success_kick_evidence;
    variant->kick_evidence_count = context->success_kick_evidence_count;
    clearra_buildup_apply_kick_trace_completeness(
        context->success_trace_steps,
        context->success_trace_step_count,
        variant);
}
void clearra_buildup_copy_success_trace_to_verification(
    const ClearraBuildUpSearchContext *context,
    clr_buildup_verification *verification) {
    if (context == 0 || verification == 0 || !context->capture_trace) {
        return;
    }
    for (uint16_t index = 0; index < context->success_trace_step_count; index++) {
        verification->operation_order_storage[index] =
            context->success_operation_order_ids[index];
        verification->trace_step_storage[index] =
            context->success_trace_steps[index];
    }
    for (uint16_t index = 0; index < context->success_kick_evidence_count;
         index++) {
        verification->kick_evidence_storage[index] =
            context->success_kick_evidence[index];
    }
    verification->variant.operation_order_ids =
        verification->operation_order_storage;
    verification->variant.operation_order_count =
        context->success_trace_step_count;
    verification->variant.trace_steps = verification->trace_step_storage;
    verification->variant.trace_step_count = context->success_trace_step_count;
    verification->variant.trace_identity = clearra_buildup_trace_identity(
        verification->trace_step_storage,
        verification->variant.trace_step_count);
    verification->variant.operation_set_hash =
        clearra_buildup_trace_operation_set_hash(
            verification->trace_step_storage,
            verification->variant.trace_step_count);
    verification->variant.kick_evidence =
        context->success_kick_evidence_count == 0u
            ? 0
            : verification->kick_evidence_storage;
    verification->variant.kick_evidence_count =
        context->success_kick_evidence_count;
    clearra_buildup_apply_kick_trace_completeness(
        verification->trace_step_storage,
        verification->variant.trace_step_count,
        &verification->variant);
}
