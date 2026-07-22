#include "buildup_workspace.h"

#include "buildup_search_internal.h"
#include "clr_execution_control.h"

#include <stdlib.h>
#include <string.h>

static bool layout_matches(
    ClearraBoard64Layout left,
    ClearraBoard64Layout right) {
    return left.width == right.width && left.height == right.height;
}

static bool workspace_identity_matches(
    const clr_buildup_workspace *workspace,
    const clr_buildup_problem *problem,
    ClearraBoard64Layout layout) {
    return workspace->initialized != 0u &&
           layout_matches(workspace->layout, layout) &&
           memcmp(
               &workspace->rule_descriptor,
               &problem->rule,
               sizeof(problem->rule)) == 0;
}

static bool board_descriptor_matches(
    const clr_board_descriptor *left,
    const clr_board_descriptor *right) {
    return left->width == right->width &&
           left->visible_height == right->visible_height &&
           left->search_height == right->search_height &&
           left->initial_mask == right->initial_mask &&
           left->initial_mask_hi == right->initial_mask_hi &&
           left->backend_kind == right->backend_kind &&
           left->cell_count == right->cell_count;
}

static bool operation_matches(
    const clr_buildup_operation *left,
    const clr_buildup_operation *right) {
    return left->piece == right->piece &&
           left->rotation == right->rotation &&
           left->x == right->x &&
           left->y == right->y &&
           left->operation_id == right->operation_id &&
           left->required_deleted_row_mask ==
               right->required_deleted_row_mask &&
           left->mask == right->mask;
}

static bool operation_set_matches(
    const clr_buildup_operation_set *left,
    const clr_buildup_operation_set *right) {
    if (left->operation_count != right->operation_count ||
        left->geometry_variant_domains != right->geometry_variant_domains) {
        return false;
    }
    for (uint16_t index = 0u; index < left->operation_count; ++index) {
        if (left->representative_order_hint[index] !=
                right->representative_order_hint[index] ||
            !operation_matches(
                &left->operations[index], &right->operations[index])) {
            return false;
        }
    }
    return true;
}

static bool root_transition_cache_matches(
    const ClearraBuildUpRootTransitionCache *cache,
    const clr_buildup_problem *problem) {
    return cache->prepared != 0u &&
           cache->candidate_id == problem->candidate_id &&
           cache->canonical_operation_set_id ==
               problem->canonical_operation_set_id &&
           board_descriptor_matches(
               &cache->initial_board, &problem->initial_board) &&
           operation_set_matches(
               &cache->operation_set, &problem->operation_set);
}

static void advance_root_transition_generation(
    ClearraBuildUpRootTransitionCache *cache) {
    cache->generation++;
    if (cache->generation != 0u) {
        return;
    }
    for (uint16_t index = 0u; index < CLR_BUILDUP_MAX_OPERATIONS; ++index) {
        cache->operations[index].generation = 0u;
    }
    cache->generation = 1u;
}

static clr_buildup_status prepare_root_transition_cache(
    clr_buildup_workspace *workspace,
    const clr_buildup_problem *problem) {
    ClearraBuildUpRootTransitionCache *cache =
        &workspace->root_transition_cache;
    if (root_transition_cache_matches(cache, problem)) {
        return CLR_BUILDUP_OK;
    }

    advance_root_transition_generation(cache);
    cache->initial_board = problem->initial_board;
    cache->operation_set = problem->operation_set;
    cache->candidate_id = problem->candidate_id;
    cache->canonical_operation_set_id =
        problem->canonical_operation_set_id;
    cache->prepared = 1u;
    return CLR_BUILDUP_OK;
}

clr_buildup_status clearra_buildup_root_transition_cache_prepare_operation(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *initial_state,
    uint16_t operation_index,
    ClearraBuildUpRootOperationTransitions *operation_cache) {
    if (context == 0 || initial_state == 0 || operation_cache == 0 ||
        operation_index >= context->problem->operation_set.operation_count) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    ClearraBuildUpRootTransitionCache *root_cache =
        context->root_transition_cache;
    if (root_cache != 0 &&
        (root_cache->capture_trace != context->capture_trace ||
         root_cache->reachability_trace_mode !=
             context->reachability_trace_mode)) {
        advance_root_transition_generation(root_cache);
        root_cache->capture_trace = context->capture_trace;
        root_cache->reachability_trace_mode =
            context->reachability_trace_mode;
    }
    if (root_cache != 0 &&
        operation_cache->generation == root_cache->generation) {
        return CLR_BUILDUP_OK;
    }
    if (clr_execution_cancel_requested()) {
        return CLR_BUILDUP_CANCELLED;
    }

    clr_buildup_operation variants[CLR_BUILDUP_MAX_OPERATION_VARIANTS];
    uint8_t variant_count = 0u;
    operation_cache->count = 0u;
    operation_cache->preparation_status =
        clearra_buildup_operation_variants_for_state(
            context,
            initial_state,
            operation_index,
            variants,
            &variant_count);
    if (operation_cache->preparation_status == CLR_BUILDUP_OK) {
        operation_cache->count = variant_count;
        for (uint8_t variant_index = 0u;
             variant_index < variant_count;
             ++variant_index) {
            ClearraBuildUpRootTransition *transition =
                &operation_cache->transitions[variant_index];
            transition->status = clearra_buildup_search_try_operation(
                context,
                *initial_state,
                initial_state->hold_automaton_state,
                &variants[variant_index],
                operation_index,
                &transition->next_state,
                &transition->trace_step,
                &transition->kick_evidence);
            if (transition->status == CLR_BUILDUP_CANCELLED) {
                return transition->status;
            }
        }
    }
    bool cacheable =
        clearra_buildup_branch_outcome_for_status(
            operation_cache->preparation_status) !=
            CLEARRA_BUILDUP_BRANCH_INCOMPLETE &&
        clearra_buildup_branch_outcome_for_status(
            operation_cache->preparation_status) !=
            CLEARRA_BUILDUP_BRANCH_FATAL;
    for (uint8_t index = 0u;
         cacheable && index < operation_cache->count;
         ++index) {
        ClearraBuildUpBranchOutcome outcome =
            clearra_buildup_branch_outcome_for_status(
                operation_cache->transitions[index].status);
        cacheable = outcome != CLEARRA_BUILDUP_BRANCH_INCOMPLETE &&
                    outcome != CLEARRA_BUILDUP_BRANCH_FATAL;
    }
    if (cacheable) {
        operation_cache->generation =
            root_cache == 0 ? 1u : root_cache->generation;
    }
    return CLR_BUILDUP_OK;
}

clr_buildup_workspace *clr_buildup_workspace_create(void) {
    clr_buildup_workspace *workspace =
        (clr_buildup_workspace *)malloc(sizeof(*workspace));
    if (workspace != 0) {
        *workspace = (clr_buildup_workspace){0};
    }
    return workspace;
}

void clr_buildup_workspace_release(clr_buildup_workspace *workspace) {
    if (workspace == 0) {
        return;
    }
    clearra_buildup_completion_memo_storage_release(
        &workspace->completion_memo_storage);
    clearra_buildup_operation_variant_cache_release(
        &workspace->operation_variant_cache);
    clearra_buildup_reachability_cache_release(
        &workspace->reachability_cache);
    clearra_buildup_reachable_lock_cache_release(
        &workspace->reachable_lock_cache);
    clearra_buildup_geometry_transition_cache_release(
        &workspace->geometry_transition_cache);
    clearra_buildup_geometry_dag_release(&workspace->geometry_dag);
    clearra_realization_feasibility_workspace_release(
        &workspace->realization_feasibility);
    free(workspace);
}

static size_t retained_bytes_add(size_t total, size_t count, size_t item_size) {
    if (count != 0u && item_size > (SIZE_MAX - total) / count) {
        return SIZE_MAX;
    }
    return total + count * item_size;
}

size_t clr_buildup_workspace_retained_bytes(
    const clr_buildup_workspace *workspace) {
    if (workspace == 0) {
        return 0u;
    }
    size_t bytes = sizeof(*workspace);
    if (workspace->completion_memo_storage.entries != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->completion_memo_storage.capacity,
            sizeof(*workspace->completion_memo_storage.entries));
    }
    if (workspace->completion_memo_storage.occupied_generations != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->completion_memo_storage.capacity,
            sizeof(*workspace->completion_memo_storage.occupied_generations));
    }
    if (workspace->reachability_cache.entries_allocation != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->reachability_cache.capacity,
            sizeof(*workspace->reachability_cache.entries));
        bytes = retained_bytes_add(
            bytes,
            CLEARRA_BUILDUP_REACHABILITY_CACHE_LINE_BYTES - 1u,
            1u);
    }
    if (workspace->reachability_cache.epochs != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->reachability_cache.capacity,
            sizeof(*workspace->reachability_cache.epochs));
    }
    if (workspace->reachable_lock_cache.entries_allocation != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->reachable_lock_cache.capacity,
            sizeof(*workspace->reachable_lock_cache.entries));
        bytes = retained_bytes_add(
            bytes,
            CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_LINE_BYTES - 1u,
            1u);
    }
    if (workspace->reachable_lock_cache.epochs != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->reachable_lock_cache.capacity,
            sizeof(*workspace->reachable_lock_cache.epochs));
    }
    if (workspace->operation_variant_cache.keys != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->operation_variant_cache.capacity,
            sizeof(*workspace->operation_variant_cache.keys));
    }
    if (workspace->operation_variant_cache.values != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->operation_variant_cache.capacity,
            sizeof(*workspace->operation_variant_cache.values));
    }
    if (workspace->operation_variant_cache.occupied != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->operation_variant_cache.capacity,
            sizeof(*workspace->operation_variant_cache.occupied));
    }
    if (workspace->geometry_transition_cache.hot_entries_allocation != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->geometry_transition_cache.capacity,
            sizeof(*workspace->geometry_transition_cache.hot_entries));
        bytes = retained_bytes_add(
            bytes,
            CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_LINE_BYTES - 1u,
            1u);
    }
    if (workspace->geometry_transition_cache.cold_results_allocation != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->geometry_transition_cache.capacity,
            sizeof(*workspace->geometry_transition_cache.cold_results));
        bytes = retained_bytes_add(
            bytes,
            CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_LINE_BYTES - 1u,
            1u);
    }
    if (workspace->geometry_transition_cache.epochs != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->geometry_transition_cache.capacity,
            sizeof(*workspace->geometry_transition_cache.epochs));
    }
    if (workspace->geometry_transition_cache.cold_epochs != 0) {
        bytes = retained_bytes_add(
            bytes,
            workspace->geometry_transition_cache.capacity,
            sizeof(*workspace->geometry_transition_cache.cold_epochs));
    }
    bytes = retained_bytes_add(
        bytes,
        clearra_buildup_geometry_dag_retained_bytes(&workspace->geometry_dag),
        1u);
    bytes = retained_bytes_add(
        bytes,
        clearra_realization_feasibility_workspace_retained_bytes(
            &workspace->realization_feasibility),
        1u);
    return bytes;
}

clr_buildup_status clearra_buildup_workspace_prepare(
    clr_buildup_workspace *workspace,
    const clr_buildup_problem *problem) {
    if (workspace == 0 || problem == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    ClearraBoard64Layout layout;
    clr_buildup_status status =
        clearra_buildup_search_layout_from_problem(problem, &layout);
    if (status != CLR_BUILDUP_OK) {
        return status;
    }
    bool identity_matches = workspace_identity_matches(workspace, problem, layout);
    clearra_buildup_operation_variant_cache_prepare(
        &workspace->operation_variant_cache, problem);
    clearra_buildup_reachability_cache_prepare(
        &workspace->reachability_cache, problem, !identity_matches);
    clearra_buildup_reachable_lock_cache_prepare(
        &workspace->reachable_lock_cache, problem, !identity_matches);
    clearra_buildup_geometry_transition_cache_prepare(
        &workspace->geometry_transition_cache, problem, !identity_matches);
    if (identity_matches) {
        return prepare_root_transition_cache(workspace, problem);
    }

    ClearraCompactRuleProfile compiled_rule = {0};
    ClearraReachabilityStatus reachability_status =
        clearra_reachability_compile_rule(&problem->rule, &compiled_rule);
    if (reachability_status != CLEARRA_REACHABILITY_OK) {
        return clearra_buildup_status_from_reachability_status(
            reachability_status);
    }
    uint8_t mode = clearra_buildup_reachability_mode_for_rule(&problem->rule);
    if (mode == 0u) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }

    workspace->compiled_rule = compiled_rule;
    workspace->rule_descriptor = problem->rule;
    workspace->layout = layout;
    workspace->reachability_mode = mode;
    workspace->initialized = 1u;
    workspace->root_transition_cache.prepared = 0u;
    return prepare_root_transition_cache(workspace, problem);
}
