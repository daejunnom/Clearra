#include "clr_memory_internal.h"

#include <stdlib.h>
static ClrMemStatus clr_gpu_buffer_register_impl(
    ClrMemContext *context,
    const ClrScope *owner_scope,
    size_t byte_len,
    uint64_t *out_buffer_id) {
    ClrGpuBufferRecord *record = NULL;
    if (context == NULL || out_buffer_id == NULL || byte_len == 0 ||
        (owner_scope != NULL && owner_scope->context != context)) {
        return CLR_MEM_INVALID_ARGUMENT;
    }

    record = (ClrGpuBufferRecord *)malloc(sizeof(ClrGpuBufferRecord));
    if (record == NULL) {
        *out_buffer_id = 0;
        return CLR_MEM_OUT_OF_MEMORY;
    }

    *record = (ClrGpuBufferRecord){
        .buffer_id = context->next_gpu_buffer_id++,
        .owner_scope_id = owner_scope == NULL ? 0u : owner_scope->scope_id,
        .fence_epoch = 0u,
        .released_epoch = 0u,
        .byte_len = byte_len,
        .fence_epoch_set = false,
        .pending_release = false,
        .released = false,
        .next = context->gpu_buffers,
    };
    context->gpu_buffers = record;
    *out_buffer_id = record->buffer_id;
    return CLR_MEM_OK;
}ClrMemStatus clr_gpu_buffer_register(
    ClrMemContext *context,
    size_t byte_len,
    uint64_t *out_buffer_id) {
    return clr_gpu_buffer_register_impl(context, NULL, byte_len, out_buffer_id);
}ClrMemStatus clr_gpu_buffer_register_for_scope(
    ClrMemContext *context,
    const ClrScope *owner_scope,
    size_t byte_len,
    uint64_t *out_buffer_id) {
    return clr_gpu_buffer_register_impl(context, owner_scope, byte_len, out_buffer_id);
}ClrMemStatus clr_gpu_buffer_set_fence_epoch(
    ClrMemContext *context,
    uint64_t buffer_id,
    uint64_t fence_epoch) {
    ClrGpuBufferRecord *record = NULL;
    if (context == NULL || buffer_id == 0 || fence_epoch == 0) {
        return CLR_MEM_INVALID_ARGUMENT;
    }

    for (record = context->gpu_buffers; record != NULL; record = record->next) {
        if (record->buffer_id == buffer_id) {
            if (record->released || record->pending_release) {
                context->counters.double_releases++;
                return CLR_MEM_DOUBLE_RELEASE;
            }
            record->fence_epoch = fence_epoch;
            record->fence_epoch_set = true;
            return CLR_MEM_OK;
        }
    }
    return CLR_MEM_NOT_FOUND;
}ClrMemStatus clr_gpu_buffer_release(ClrMemContext *context, uint64_t buffer_id) {
    ClrGpuBufferRecord *record = NULL;
    if (context == NULL || buffer_id == 0) {
        return CLR_MEM_INVALID_ARGUMENT;
    }

    for (record = context->gpu_buffers; record != NULL; record = record->next) {
        if (record->buffer_id == buffer_id) {
            if (record->released || record->pending_release) {
                context->counters.double_releases++;
                return CLR_MEM_DOUBLE_RELEASE;
            }
            if (!record->fence_epoch_set || record->fence_epoch == 0) {
                return CLR_MEM_INVALID_STATE;
            }
            if (context->epoch < record->fence_epoch) {
                record->pending_release = true;
                return CLR_MEM_OK;
            }
            record->released = true;
            record->released_epoch = context->epoch;
            return CLR_MEM_OK;
        }
    }
    return CLR_MEM_NOT_FOUND;
}ClrMemStatus clr_gpu_buffer_drain_pending_impl(
    ClrMemContext *context,
    uint64_t through_epoch) {
    ClrGpuBufferRecord *record = NULL;
    if (context == NULL) {
        return CLR_MEM_INVALID_ARGUMENT;
    }

    for (record = context->gpu_buffers; record != NULL; record = record->next) {
        if (record->pending_release && through_epoch >= record->fence_epoch) {
            record->pending_release = false;
            record->released = true;
            record->released_epoch = through_epoch;
        }
    }
    return CLR_MEM_OK;
}
