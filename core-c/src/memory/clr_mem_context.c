#include "clr_memory_internal.h"

#include <stdlib.h>
ClrMemStatus clr_mem_context_create(ClrMemContext **out_context) {
    ClrMemContext *context = NULL;
    if (out_context == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }

    context = (ClrMemContext *)malloc(sizeof(ClrMemContext));
    if (context == NULL) {
        *out_context = NULL;
        return CLR_MEM_OUT_OF_MEMORY;
    }

    *context = (ClrMemContext){
        .next_scope_id = 1u,
        .next_gpu_buffer_id = 1u,
    };
    *out_context = context;
    return CLR_MEM_OK;
}ClrMemStatus clr_mem_context_release(ClrMemContext **context) {
    ClrMemContext *owned = NULL;
    ClrScope *scope = NULL;
    ClrGpuBufferRecord *gpu = NULL;
    ClrReleaseQueueEntry *entry = NULL;
    ClrMemStatus status = CLR_MEM_OK;

    if (context == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }
    if (*context == NULL) {
        return CLR_MEM_DOUBLE_RELEASE;
    }
    owned = *context;

    scope = owned->scopes;
    while (scope != NULL) {
        ClrScope *next = scope->next;
        if (!scope->released) {
            ClrMemStatus release_status =
                scope->state == CLR_SCOPE_PENDING_RELEASE
                    ? clr_memory_scope_release_pending_impl(scope)
                    : clr_memory_scope_release_impl(scope, true);
            if (status == CLR_MEM_OK && release_status != CLR_MEM_OK) {
                status = release_status;
            }
        }
        clr_memory_scope_free_metadata(scope);
        scope = next;
    }

    entry = owned->release_queue;
    while (entry != NULL) {
        ClrReleaseQueueEntry *next = entry->next;
        free(entry);
        entry = next;
    }

    gpu = owned->gpu_buffers;
    while (gpu != NULL) {
        ClrGpuBufferRecord *next = gpu->next;
        free(gpu);
        gpu = next;
    }

    free(owned);
    *context = NULL;
    return status;
}ClrMemStatus clr_mem_context_leak_report(
    const ClrMemContext *context,
    ClrMemLeakReport *out_report) {
    const ClrScope *scope = NULL;
    const ClrReleaseQueueEntry *entry = NULL;
    const ClrGpuBufferRecord *gpu = NULL;
    ClrMemLeakReport report;

    if (context == NULL || out_report == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }

    report = context->counters;

    for (scope = context->scopes; scope != NULL; scope = scope->next) {
        const ClrAllocation *allocation = NULL;
        if (scope->released) {
            continue;
        }
        report.live_scopes++;
        for (allocation = scope->allocations; allocation != NULL; allocation = allocation->next) {
            report.live_allocations++;
        }
    }

    for (gpu = context->gpu_buffers; gpu != NULL; gpu = gpu->next) {
        if (!gpu->released) {
            report.live_gpu_buffers++;
            if (gpu->pending_release) {
                report.pending_gpu_buffer_releases++;
            }
        }
    }

    for (entry = context->release_queue; entry != NULL; entry = entry->next) {
        report.pending_release_queue++;
    }

    *out_report = report;
    return CLR_MEM_OK;
}
