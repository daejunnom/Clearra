#include "clr_memory_internal.h"

#include <stdlib.h>
#include <string.h>
static void clr_allocation_free_all(ClrAllocation *allocation) {
    while (allocation != NULL) {
        ClrAllocation *next = allocation->next;
        if (allocation->payload != NULL && allocation->size > 0) {
            memset(allocation->payload, CLR_POISON_BYTE, allocation->size);
        }
        free(allocation->payload);
        free(allocation);
        allocation = next;
    }
}ClrMemStatus clr_scope_create(
    ClrMemContext *context,
    ClrScopeKind kind,
    ClrScope **out_scope) {
    ClrScope *scope = NULL;
    if (context == NULL || out_scope == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }
    if (kind != CLR_SCOPE_SEARCH && kind != CLR_SCOPE_BATCH && kind != CLR_SCOPE_WORKER &&
        kind != CLR_SCOPE_GPU_TRANSFER) {
        return CLR_MEM_INVALID_ARGUMENT;
    }

    scope = (ClrScope *)malloc(sizeof(ClrScope));
    if (scope == NULL) {
        *out_scope = NULL;
        return CLR_MEM_OUT_OF_MEMORY;
    }

    *scope = (ClrScope){
        .context = context,
        .scope_id = context->next_scope_id++,
        .kind = kind,
        .state = CLR_SCOPE_ACTIVE,
        .released = false,
        .aborted = false,
        .canary = CLR_SCOPE_CANARY,
        .allocations = NULL,
        .next = context->scopes,
    };
    context->scopes = scope;

    *out_scope = scope;
    return CLR_MEM_OK;
}ClrMemStatus clr_scope_release(ClrScope *scope) {
    return clr_memory_scope_release_impl(scope, false);
}ClrMemStatus clr_scope_abort(ClrScope *scope) {
    return clr_memory_scope_release_impl(scope, true);
}ClrMemStatus clr_scope_kind(const ClrScope *scope, ClrScopeKind *out_kind) {
    if (scope == NULL || out_kind == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }
    *out_kind = scope->kind;
    return CLR_MEM_OK;
}ClrMemStatus clr_scope_state(const ClrScope *scope, ClrScopeState *out_state) {
    if (scope == NULL || out_state == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }
    *out_state = scope->state;
    return CLR_MEM_OK;
}bool clr_scope_is_released(const ClrScope *scope) {
    return scope == NULL || scope->state == CLR_SCOPE_RELEASED ||
        scope->state == CLR_SCOPE_ABORTED;
}ClrMemStatus clr_memory_scope_alloc_impl(ClrScope *scope, size_t size, void **out_ptr) {
    ClrAllocation *allocation = NULL;
    if (scope == NULL || out_ptr == NULL || size == 0) {
        return CLR_MEM_INVALID_ARGUMENT;
    }
    if (scope->state != CLR_SCOPE_ACTIVE || scope->released || scope->aborted) {
        return CLR_MEM_ABORTED;
    }

    allocation = (ClrAllocation *)malloc(sizeof(ClrAllocation));
    if (allocation == NULL) {
        *out_ptr = NULL;
        return CLR_MEM_OUT_OF_MEMORY;
    }

    void *payload = malloc(size);
    if (payload == NULL) {
        free(allocation);
        *out_ptr = NULL;
        return CLR_MEM_OUT_OF_MEMORY;
    }

    *allocation = (ClrAllocation){
        .payload = payload,
        .size = size,
        .head_canary = CLR_ALLOC_CANARY,
        .tail_canary = CLR_ALLOC_CANARY,
        .poisoned = false,
        .next = scope->allocations,
    };
    scope->allocations = allocation;

    *out_ptr = allocation->payload;
    return CLR_MEM_OK;
}
static ClrMemStatus clr_memory_scope_release_impl_internal(
    ClrScope *scope,
    bool aborted,
    bool allow_pending_release);
static bool clr_scope_is_search_child_candidate(const ClrScope *scope) {
    return scope != NULL && scope->kind != CLR_SCOPE_SEARCH &&
        scope->state == CLR_SCOPE_ACTIVE && !scope->released;
}static ClrMemStatus clr_search_scope_release_active_children(
    ClrScope *search_scope,
    bool aborted) {
    ClrScope *child = NULL;
    ClrMemStatus status = CLR_MEM_OK;
    if (search_scope == NULL || search_scope->context == NULL ||
        search_scope->kind != CLR_SCOPE_SEARCH) {
        return CLR_MEM_OK;
    }

    for (child = search_scope->context->scopes; child != NULL; child = child->next) {
        if (child == search_scope || !clr_scope_is_search_child_candidate(child)) {
            continue;
        }
        {
            ClrMemStatus child_status =
                clr_memory_scope_release_impl_internal(child, aborted, false);
            if (status == CLR_MEM_OK && child_status != CLR_MEM_OK) {
                status = child_status;
            }
        }
    }
    return status;
}static ClrMemStatus clr_memory_scope_release_impl_internal(
    ClrScope *scope,
    bool aborted,
    bool allow_pending_release) {
    ClrMemStatus check_status = CLR_MEM_OK;
    ClrMemStatus child_status = CLR_MEM_OK;
    if (scope == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }
    if (scope->state == CLR_SCOPE_RELEASED || scope->state == CLR_SCOPE_ABORTED ||
        scope->released) {
        if (scope->context != NULL) {
            scope->context->counters.double_releases++;
        }
        return CLR_MEM_DOUBLE_RELEASE;
    }
    if (scope->state == CLR_SCOPE_PENDING_RELEASE && !allow_pending_release) {
        return CLR_MEM_INVALID_STATE;
    }

    child_status = clr_search_scope_release_active_children(scope, aborted);
    check_status = clr_memory_scope_check_impl(scope);
    if (scope->context != NULL) {
        if (check_status == CLR_MEM_CANARY_CORRUPTED) {
            scope->context->counters.canary_failures++;
        } else if (check_status == CLR_MEM_DEBUG_POISONED) {
            scope->context->counters.poison_detections++;
        }

        scope->context->counters.released_scopes++;
        if (aborted) {
            scope->context->counters.aborted_scopes++;
        }
    }

    clr_allocation_free_all(scope->allocations);
    scope->allocations = NULL;
    scope->released = true;
    scope->aborted = aborted;
    scope->state = aborted ? CLR_SCOPE_ABORTED : CLR_SCOPE_RELEASED;
    return check_status != CLR_MEM_OK ? check_status : child_status;
}ClrMemStatus clr_memory_scope_release_impl(ClrScope *scope, bool aborted) {
    return clr_memory_scope_release_impl_internal(scope, aborted, false);
}ClrMemStatus clr_memory_scope_release_pending_impl(ClrScope *scope) {
    return clr_memory_scope_release_impl_internal(scope, false, true);
}ClrMemStatus clr_memory_scope_check_impl(const ClrScope *scope) {
    const ClrAllocation *allocation = NULL;
    if (scope == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }
    if (scope->canary != CLR_SCOPE_CANARY) {
        return CLR_MEM_CANARY_CORRUPTED;
    }
    for (allocation = scope->allocations; allocation != NULL; allocation = allocation->next) {
        if (allocation->head_canary != CLR_ALLOC_CANARY ||
            allocation->tail_canary != CLR_ALLOC_CANARY) {
            return CLR_MEM_CANARY_CORRUPTED;
        }
        if (allocation->poisoned) {
            return CLR_MEM_DEBUG_POISONED;
        }
    }
    return CLR_MEM_OK;
}void clr_memory_scope_free_metadata(ClrScope *scope) {
    if (scope == NULL) {
        return;
    }
    clr_allocation_free_all(scope->allocations);
    free(scope);
}
