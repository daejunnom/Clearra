#include "clr_memory_internal.h"

#include <stdlib.h>

uint64_t clr_epoch_current(const ClrMemContext *context) {
    return context == NULL ? 0 : context->epoch;
}

ClrMemStatus clr_epoch_advance(ClrMemContext *context, uint64_t *out_epoch) {
    if (context == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }
    context->epoch++;
    if (out_epoch != NULL) {
        *out_epoch = context->epoch;
    }
    return CLR_MEM_OK;
}

ClrMemStatus clr_release_queue_defer_scope(
    ClrMemContext *context,
    ClrScope *scope,
    uint64_t release_epoch) {
    ClrReleaseQueueEntry *entry = NULL;
    if (context == NULL || scope == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }
    if (scope->released || scope->state == CLR_SCOPE_RELEASED ||
        scope->state == CLR_SCOPE_ABORTED) {
        context->counters.double_releases++;
        return CLR_MEM_DOUBLE_RELEASE;
    }
    if (scope->state == CLR_SCOPE_PENDING_RELEASE) {
        return CLR_MEM_INVALID_STATE;
    }

    entry = (ClrReleaseQueueEntry *)malloc(sizeof(ClrReleaseQueueEntry));
    if (entry == NULL) {
        return CLR_MEM_OUT_OF_MEMORY;
    }
    *entry = (ClrReleaseQueueEntry){
        .scope = scope,
        .release_epoch = release_epoch,
        .next = context->release_queue,
    };
    context->release_queue = entry;
    scope->state = CLR_SCOPE_PENDING_RELEASE;
    return CLR_MEM_OK;
}ClrMemStatus clr_release_queue_drain(ClrMemContext *context, uint64_t through_epoch) {
    ClrReleaseQueueEntry **cursor = NULL;
    ClrMemStatus status = CLR_MEM_OK;
    if (context == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }

    cursor = &context->release_queue;
    while (*cursor != NULL) {
        ClrReleaseQueueEntry *entry = *cursor;
        if (entry->release_epoch <= through_epoch) {
            ClrMemStatus release_status = clr_memory_scope_release_pending_impl(entry->scope);
            *cursor = entry->next;
            free(entry);
            if (status == CLR_MEM_OK && release_status != CLR_MEM_OK) {
                status = release_status;
            }
        } else {
            cursor = &entry->next;
        }
    }
    {
        ClrMemStatus gpu_status =
            clr_gpu_buffer_drain_pending_impl(context, through_epoch);
        if (status == CLR_MEM_OK && gpu_status != CLR_MEM_OK) {
            status = gpu_status;
        }
    }
    return status;
}
