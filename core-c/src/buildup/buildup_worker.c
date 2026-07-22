#include "buildup_search.h"
#include "buildup_workspace.h"
#include "generic_buildup.h"
#include "clr_execution_control.h"
#include "clr_search_profile.h"
void clearra_buildup_copy_success_trace_to_verification(
    const ClearraBuildUpSearchContext *context,
    clr_buildup_verification *verification);
static void verification_reject(
    clr_buildup_verification *verification,
    clr_buildup_status status,
    uint16_t step) {
    if (verification == 0) {
        return;
    }
    *verification = (clr_buildup_verification){0};
    verification->accepted = 0;
    verification->rejected_step = step;
    verification->reject_reason = (uint32_t)status;
}

static clr_buildup_status validate_buildup_problem_for_operation_count(
    const clr_buildup_problem *problem,
    uint16_t operation_count) {
    if (problem == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    clr_buildup_status generic_status =
        clearra_buildup_runtime_status_for_board(&problem->initial_board);
    if (generic_status != CLR_BUILDUP_OK) {
        return generic_status;
    }
    generic_status =
        clearra_buildup_runtime_status_for_board(&problem->packing.board);
    if (generic_status != CLR_BUILDUP_OK) {
        return generic_status;
    }
    generic_status =
        clearra_buildup_operation_set_runtime_status(operation_count);
    if (generic_status != CLR_BUILDUP_OK) {
        return generic_status;
    }
    if (!clr_buildup_problem_is_valid(problem)) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }

    clr_buildup_status status =
        clearra_buildup_verify_piece_window_for_count(problem, operation_count);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    return clearra_buildup_verify_bag_pattern(problem);
}

static clr_buildup_status validate_buildup_problem_for_search(
    const clr_buildup_problem *problem) {
    return validate_buildup_problem_for_operation_count(
        problem, problem == 0 ? 0u : problem->operation_set.operation_count);
}

static clr_buildup_status run_buildup_search(
    const clr_buildup_problem *problem,
    const ClearraGeometryCatalog *catalog,
    const uint32_t *catalog_row_ids,
    uint16_t catalog_operation_count,
    const uint8_t *representative_order_hint,
    const uint16_t *required_predecessors,
    uint8_t stop_after_first_success,
    uint8_t preserve_hold_branches,
    uint8_t capture_trace,
    uint8_t prefer_highest_t_spin_trace,
    uint8_t use_geometry_dag,
    uint64_t max_count_variants,
    uint32_t max_retained_variants,
    clr_build_variant_buffer *out_variants,
    clr_buildup_workspace *workspace,
    ClearraBuildUpSearchContext *out_context) {
    if (clr_execution_cancel_requested()) {
        return CLR_BUILDUP_CANCELLED;
    }
    uint8_t reachability_trace_mode =
        capture_trace == 0u
            ? CLEARRA_REACHABILITY_TRACE_NONE
            : (prefer_highest_t_spin_trace != 0u
                   ? CLEARRA_REACHABILITY_TRACE_HIGHEST_T_SPIN
                   : CLEARRA_REACHABILITY_TRACE_FIRST_LEGAL);
    clr_buildup_status status = CLR_BUILDUP_OK;
    if (workspace != 0) {
        status = clearra_buildup_workspace_prepare(workspace, problem);
        if (status == CLR_BUILDUP_OK) {
            status = catalog == 0
                ? clearra_buildup_search_context_init_with_reachability(
                      problem, &workspace->compiled_rule, out_context)
                : clearra_buildup_search_context_init_catalog_rows(
                      problem,
                      catalog,
                      catalog_row_ids,
                      catalog_operation_count,
                      representative_order_hint,
                      required_predecessors,
                      &workspace->compiled_rule,
                      out_context);
            if (status == CLR_BUILDUP_OK) {
                out_context->root_transition_cache = catalog == 0
                    ? &workspace->root_transition_cache
                    : 0;
                out_context->operation_variant_cache =
                    &workspace->operation_variant_cache;
                out_context->reachability_cache =
                    &workspace->reachability_cache;
                out_context->reachable_lock_cache =
                    &workspace->reachable_lock_cache;
                out_context->reachability_frontier =
                    &workspace->reachability_frontier;
                out_context->geometry_transition_cache =
                    &workspace->geometry_transition_cache;
                out_context->geometry_dag =
                    use_geometry_dag != 0u
                        ? &workspace->geometry_dag
                        : 0;
                out_context->capture_trace = capture_trace;
                out_context->reachability_trace_mode =
                    reachability_trace_mode;
                if (out_context->geometry_dag != 0) {
                    status = clearra_buildup_geometry_dag_prepare(
                        out_context->geometry_dag, out_context);
                }
            }
        }
    } else if (catalog == 0) {
        status = clearra_buildup_search_context_init(problem, out_context);
    } else {
        status = CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    if (workspace != 0) {
        clearra_buildup_completion_memo_init_with_storage(
            &out_context->completion_memo,
            problem,
            &workspace->completion_memo_storage);
    } else {
        clearra_buildup_completion_memo_init(
            &out_context->completion_memo, problem);
    }
    out_context->stop_after_first_success = stop_after_first_success;
    out_context->preserve_hold_branches = preserve_hold_branches;
    out_context->capture_trace = capture_trace;
    out_context->reachability_trace_mode = reachability_trace_mode;
    out_context->max_count_variants = max_count_variants;
    out_context->max_retained_variants = max_retained_variants;
    out_context->out_variants = out_variants;

    ClearraBuildUpQueueHold queue_hold;
    clr_search_profile_span queue_hold_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_QUEUE_HOLD_INIT);
    status = clearra_buildup_queue_hold_init(problem, &queue_hold);
    (void)clr_search_profile_end(queue_hold_span, 1u);
    if (status != CLR_BUILDUP_OK) {
        clearra_buildup_completion_memo_release(
            &out_context->completion_memo);
        return status;
    }

    ClearraBuildUpState initial_state = clearra_buildup_state_initial(problem);
    uint16_t remaining_operations = 0u;
    status = clearra_buildup_remaining_ops_bitset_for_count(
        out_context->order.count, &remaining_operations);
    if (status != CLR_BUILDUP_OK) {
        clearra_buildup_completion_memo_release(
            &out_context->completion_memo);
        return status;
    }
    clr_search_profile_span search_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_SEARCH);
    if (clearra_buildup_geometry_dag_is_available(out_context->geometry_dag)) {
        status = clearra_buildup_search_geometry_dag(
            out_context, out_context->geometry_dag, queue_hold);
    } else {
        status = clearra_buildup_search_order(
            out_context,
            initial_state,
            queue_hold,
            remaining_operations,
            0);
    }
    (void)clr_search_profile_end(
        search_span, out_context->enumerated_variant_count + 1u);
    clearra_buildup_completion_memo_release(
        &out_context->completion_memo);
    return status;
}static clr_buildup_status no_variant_status(
    const ClearraBuildUpSearchContext *context) {
    return context->first_failure == CLR_BUILDUP_OK
        ? CLR_BUILDUP_GOAL_NOT_SATISFIED
        : context->first_failure;
}static uint64_t bounded_enumeration_max_variants(uint32_t requested) {
    uint32_t max_variants = requested == 0u ? CLR_BUILDUP_MAX_VARIANTS : requested;
    return max_variants > CLR_BUILDUP_MAX_VARIANTS
        ? CLR_BUILDUP_MAX_VARIANTS
        : max_variants;
}static uint64_t count_max_variants(uint32_t requested) {
    return requested == 0u ? UINT64_MAX : requested;
}static clr_buildup_search_metrics search_metrics_from_context(
    const ClearraBuildUpSearchContext *context) {
    clr_buildup_search_metrics metrics = {0};
    if (context == 0) {
        return metrics;
    }
    metrics.expanded_state_count = context->expanded_state_count;
    metrics.memo_probes = context->completion_memo.probes;
    metrics.memo_hits = context->completion_memo.hits;
    metrics.memo_insertions = context->completion_memo.insertions;
    metrics.memo_saturation_skips =
        context->completion_memo.saturation_skips;
    metrics.memo_capacity = context->completion_memo.capacity;
    metrics.memo_max_probe_length =
        context->completion_memo.max_probe_length;
    return metrics;
}static clr_buildup_status buildup_worker_verify_internal(
    const clr_buildup_problem *problem,
    clr_buildup_workspace *workspace,
    clr_buildup_verification *out_verification) {
    if (out_verification == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    *out_verification = (clr_buildup_verification){0};

    clr_search_profile_span total_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_TOTAL);
    clr_search_profile_span validation_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_VALIDATE);

    clr_buildup_status status = validate_buildup_problem_for_search(problem);
    (void)clr_search_profile_end(validation_span, 1u);
    if (status != CLR_BUILDUP_OK) {
        verification_reject(out_verification, status, UINT16_MAX);
        (void)clr_search_profile_end(total_span, 1u);
        return status;
    }

    ClearraBuildUpSearchContext context = {0};
    status = run_buildup_search(
        problem,
        0,
        0,
        0u,
        0,
        0,
        1u,
        0u,
        1u,
        0u,
        0u,
        1u,
        1u,
        0,
        workspace,
        &context);
    if (status != CLR_BUILDUP_OK) {
        verification_reject(out_verification, status, context.first_failure_step);
        (void)clr_search_profile_end(total_span, 1u);
        return status;
    }
    if (context.enumerated_variant_count == 0u) {
        status = no_variant_status(&context);
        verification_reject(out_verification, status, context.first_failure_step);
        (void)clr_search_profile_end(total_span, 1u);
        return status;
    }

    out_verification->accepted = 1;
    out_verification->rejected_step = UINT16_MAX;
    out_verification->reject_reason = CLR_BUILDUP_OK;
    clearra_build_variant_from_state(
        problem, &context.success_state, &out_verification->variant);
    out_verification->variant.build_variant_id = 1u;
    clearra_buildup_copy_success_trace_to_verification(
        &context, out_verification);
    (void)clr_search_profile_end(total_span, 1u);
    return CLR_BUILDUP_OK;
}clr_buildup_status clr_buildup_worker_verify(
    const clr_buildup_problem *problem,
    clr_buildup_verification *out_verification) {
    return buildup_worker_verify_internal(problem, 0, out_verification);
}static clr_buildup_status buildup_worker_verify_into_buffer_internal(
    const clr_buildup_problem *problem,
    clr_buildup_workspace *workspace,
    clr_build_variant_buffer *out_buffer,
    clr_buildup_verification *out_verification) {
    if (out_buffer == 0 || out_verification == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    clr_buildup_status status =
        buildup_worker_verify_internal(problem, workspace, out_verification);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    return clr_build_variant_buffer_push_verified(out_buffer, out_verification);
}clr_buildup_status clr_buildup_worker_verify_into_buffer(
    const clr_buildup_problem *problem,
    clr_build_variant_buffer *out_buffer,
    clr_buildup_verification *out_verification) {
    return buildup_worker_verify_into_buffer_internal(
        problem, 0, out_buffer, out_verification);
}clr_buildup_status clr_buildup_verify_first(
    const clr_buildup_problem *problem,
    clr_build_variant_buffer *out_first) {
    if (out_first == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    clr_build_variant_buffer_clear(out_first);
    clr_buildup_verification verification;
    return buildup_worker_verify_into_buffer_internal(
        problem, 0, out_first, &verification);
}clr_buildup_status clr_buildup_verify_first_with_workspace(
    const clr_buildup_problem *problem,
    clr_buildup_workspace *workspace,
    clr_build_variant_buffer *out_first) {
    if (workspace == 0 || out_first == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    clr_build_variant_buffer_clear(out_first);
    clr_buildup_verification verification;
    return buildup_worker_verify_into_buffer_internal(
        problem, workspace, out_first, &verification);
}clr_buildup_status clr_buildup_exists_with_workspace(
    const clr_buildup_problem *problem,
    clr_buildup_workspace *workspace) {
    clr_search_profile_span exists_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_EXISTS);
    if (workspace == 0) {
        (void)clr_search_profile_end(exists_span, 1u);
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    clr_buildup_status status = validate_buildup_problem_for_search(problem);
    if (status != CLR_BUILDUP_OK) {
        (void)clr_search_profile_end(exists_span, 1u);
        return status;
    }
    ClearraBuildUpSearchContext context = {0};
    status = run_buildup_search(
        problem,
        0,
        0,
        0u,
        0,
        0,
        1u,
        1u,
        0u,
        0u,
        1u,
        1u,
        0u,
        0,
        workspace,
        &context);
    if (status != CLR_BUILDUP_OK) {
        (void)clr_search_profile_end(exists_span, 1u);
        return status;
    }
    status = context.enumerated_variant_count == 0u
                 ? no_variant_status(&context)
                 : CLR_BUILDUP_OK;
    (void)clr_search_profile_end(exists_span, 1u);
    return status;
}

clr_buildup_status clearra_buildup_exists_catalog_rows_with_workspace(
    const clr_buildup_problem *problem,
    const ClearraGeometryCatalog *catalog,
    const uint32_t *row_ids,
    uint16_t operation_count,
    const uint8_t *representative_order_hint,
    clr_buildup_workspace *workspace) {
    return clearra_buildup_exists_catalog_rows_with_constraints_and_workspace(
        problem,
        catalog,
        row_ids,
        operation_count,
        representative_order_hint,
        0,
        workspace);
}

clr_buildup_status
clearra_buildup_exists_catalog_rows_with_constraints_and_workspace(
    const clr_buildup_problem *problem,
    const ClearraGeometryCatalog *catalog,
    const uint32_t *row_ids,
    uint16_t operation_count,
    const uint8_t *representative_order_hint,
    const uint16_t *required_predecessors,
    clr_buildup_workspace *workspace) {
    clr_search_profile_span exists_span =
        clr_search_profile_begin(CLR_PROFILE_BUILDUP_EXISTS);
    if (catalog == 0 || row_ids == 0 || workspace == 0) {
        (void)clr_search_profile_end(exists_span, 1u);
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    clr_buildup_status status =
        validate_buildup_problem_for_operation_count(problem, operation_count);
    if (status != CLR_BUILDUP_OK) {
        (void)clr_search_profile_end(exists_span, 1u);
        return status;
    }

    ClearraBuildUpSearchContext context = {0};
    /* Existence needs one exact witness. Building the complete geometry
       language here repeats work that belongs to enumerate/count coverage. */
    status = run_buildup_search(
        problem,
        catalog,
        row_ids,
        operation_count,
        representative_order_hint,
        required_predecessors,
        1u,
        1u,
        0u,
        0u,
        0u,
        1u,
        0u,
        0,
        workspace,
        &context);
    if (status == CLR_BUILDUP_OK && context.enumerated_variant_count == 0u) {
        status = no_variant_status(&context);
    }
    (void)clr_search_profile_end(exists_span, 1u);
    return status;
}

static clr_buildup_status enumerate_variants_internal(
    const clr_buildup_problem *problem,
    const clr_buildup_enumeration_limits *limits,
    clr_buildup_workspace *workspace,
    clr_build_variant_buffer *out_variants) {
    if (out_variants == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    clr_build_variant_buffer_clear(out_variants);

    clr_buildup_status status = validate_buildup_problem_for_search(problem);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }

    ClearraBuildUpSearchContext context = {0};
    status = run_buildup_search(
        problem,
        0,
        0,
        0u,
        0,
        0,
        0u,
        1u,
        1u,
        limits == 0 ? 0u : limits->prefer_highest_t_spin_trace,
        1u,
        UINT64_MAX,
        (uint32_t)bounded_enumeration_max_variants(
            limits == 0 ? 0u : limits->max_variants),
        out_variants,
        workspace,
        &context);
    out_variants->total_variant_count = context.enumerated_variant_count;
    out_variants->search_metrics = search_metrics_from_context(&context);
    out_variants->count_complete = (uint8_t)(status == CLR_BUILDUP_OK);
    out_variants->trace_retention_truncated =
        (uint8_t)(context.enumerated_variant_count > out_variants->count);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    if (context.enumerated_variant_count == 0u) {
        return no_variant_status(&context);
    }
    return CLR_BUILDUP_OK;
}

clr_buildup_status clr_buildup_enumerate_variants(
    const clr_buildup_problem *problem,
    const clr_buildup_enumeration_limits *limits,
    clr_build_variant_buffer *out_variants) {
    return enumerate_variants_internal(problem, limits, 0, out_variants);
}

clr_buildup_status clr_buildup_enumerate_variants_with_workspace(
    const clr_buildup_problem *problem,
    const clr_buildup_enumeration_limits *limits,
    clr_buildup_workspace *workspace,
    clr_build_variant_buffer *out_variants) {
    if (workspace == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    return enumerate_variants_internal(problem, limits, workspace, out_variants);
}

clr_buildup_status clr_buildup_export_geometry_language_with_workspace(
    const clr_buildup_problem *problem,
    clr_buildup_workspace *workspace,
    clr_buildup_geometry_language_node *nodes,
    size_t node_capacity,
    clr_buildup_geometry_language_edge *edges,
    size_t edge_capacity,
    clr_buildup_geometry_language_report *out_report) {
    if (workspace == 0 || out_report == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    clr_buildup_status status = validate_buildup_problem_for_search(problem);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    status = clearra_buildup_workspace_prepare(workspace, problem);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    ClearraBuildUpSearchContext context = {0};
    status = clearra_buildup_search_context_init_with_reachability(
        problem, &workspace->compiled_rule, &context);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    context.operation_variant_cache = &workspace->operation_variant_cache;
    context.reachability_cache = &workspace->reachability_cache;
    context.reachable_lock_cache = &workspace->reachable_lock_cache;
    context.reachability_frontier = &workspace->reachability_frontier;
    context.geometry_transition_cache =
        &workspace->geometry_transition_cache;
    context.geometry_dag = &workspace->geometry_dag;
    status = clearra_buildup_geometry_dag_prepare(context.geometry_dag, &context);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    status = clearra_buildup_geometry_dag_export(
        context.geometry_dag,
        nodes,
        node_capacity,
        edges,
        edge_capacity,
        out_report);
    if (nodes != 0 || edges != 0) {
        clearra_buildup_geometry_dag_release(context.geometry_dag);
    }
    return status;
}

clr_buildup_status clr_buildup_count_variants(
    const clr_buildup_problem *problem,
    const clr_buildup_count_limits *limits,
    clr_buildup_count_report *out_report) {
    if (out_report == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    *out_report = (clr_buildup_count_report){0};

    clr_buildup_status status = validate_buildup_problem_for_search(problem);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }

    ClearraBuildUpSearchContext context = {0};
    status = run_buildup_search(
        problem,
        0,
        0,
        0u,
        0,
        0,
        0u,
        1u,
        0u,
        0u,
        0u,
        count_max_variants(limits == 0 ? 0u : limits->max_variants),
        0u,
        0,
        0,
        &context);

    out_report->total_variant_count = context.enumerated_variant_count;
    out_report->search_metrics = search_metrics_from_context(&context);
    out_report->retained_variant_count = 0u;
    out_report->trace_retained = 0u;
    out_report->search_complete = (uint8_t)(status == CLR_BUILDUP_OK);
    out_report->count_complete = out_report->search_complete;
    out_report->solution_exists =
        (uint8_t)(context.enumerated_variant_count > 0u);
    out_report->no_variant_reason = CLR_BUILDUP_OK;
    out_report->truncation_reason =
        clearra_buildup_branch_outcome_for_status(status) ==
                CLEARRA_BUILDUP_BRANCH_INCOMPLETE
            ? (uint32_t)status
            : CLR_BUILDUP_OK;
    if (status == CLR_BUILDUP_OK && context.enumerated_variant_count == 0u) {
        out_report->no_variant_reason = (uint32_t)no_variant_status(&context);
    }
    return status;
}
