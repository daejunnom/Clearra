#include "gpu/gpu_backend_adapter.h"
static ClearraGpuStatus gpu_status_from_memory_status(ClrMemStatus status) {
    switch (status) {
        case CLR_MEM_OK:
            return CLEARRA_GPU_OK;
        case CLR_MEM_INVALID_ARGUMENT:
        case CLR_MEM_DOUBLE_RELEASE:
        case CLR_MEM_INVALID_STATE:
            return CLEARRA_GPU_INVALID_ARGUMENT;
        case CLR_MEM_OUT_OF_MEMORY:
        case CLR_MEM_ABORTED:
        case CLR_MEM_CANARY_CORRUPTED:
        case CLR_MEM_DEBUG_POISONED:
        case CLR_MEM_NOT_FOUND:
            return CLEARRA_GPU_UNAVAILABLE;
    }
    return CLEARRA_GPU_UNAVAILABLE;
}void clearra_gpu_packing_result_clear(ClearraGpuPackingResult *result) {
    if (result == 0) {
        return;
    }
    result->status = CLEARRA_GPU_UNAVAILABLE;
    result->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_NONE;
    result->result_complete = 0u;
    result->truncation_reason = CLR_RESOURCE_TRUNCATION_NONE;
    result->used_cpu_fallback = 0u;
    result->candidate_is_solution = 0u;
    result->hash_exact_confirmed = 0u;
    result->deterministic_result = 0u;
    result->larger_batch_planner_enabled = 0u;
    result->planned_batch_count = 0u;
    result->batch_candidate_capacity = 0u;
    result->dominance_prefilter_applied = 0u;
    result->dominance_prefilter_removed_count = 0u;
    result->shape_union_mask_applied = 0u;
    result->gpu_shape_union_mask.value = 0u;
    result->gpu_candidate_hash = 0u;
    result->cpu_reference_hash = 0u;
    result->readback_compressed = 0u;
    result->readback_uncompressed_count = 0u;
    result->readback_compressed_count = 0u;
    result->cpu_exact_confirmed = 0u;
    result->cpu_exact_confirm_optimized = 0u;
    result->cpu_reference_matched = 0u;
    result->raw_candidate_count = 0u;
    result->canonical_candidate_count = 0u;
    clr_pruning_proof_ledger_init(&result->pruning_ledger);
    clearra_packing_candidate_buffer_clear(&result->raw_candidates);
    clearra_canonical_packing_table_clear(&result->canonical_candidates);
}ClearraGpuStatus clearra_gpu_context_create_memory(
    ClearraGpuContext *context,
    ClearraGpuBackendKind backend_kind) {
    ClrMemStatus status;
    if (context == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    context->backend_kind = backend_kind;
    context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_NONE;
    status = clr_mem_context_create(&context->memory_context);
    if (status != CLR_MEM_OK) {
        context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
        return gpu_status_from_memory_status(status);
    }

    status = clr_scope_create(
        context->memory_context, CLR_SCOPE_GPU_TRANSFER, &context->transfer_scope);
    if (status != CLR_MEM_OK) {
        (void)clr_mem_context_release(&context->memory_context);
        context->memory_context = 0;
        context->transfer_scope = 0;
        context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
        return gpu_status_from_memory_status(status);
    }

    status = clr_arena_alloc(
        context->transfer_scope,
        sizeof(ClearraPackingCandidateBuffer),
        (void **)&context->candidate_buffer);
    if (status != CLR_MEM_OK) {
        (void)clr_scope_abort(context->transfer_scope);
        (void)clr_mem_context_release(&context->memory_context);
        context->memory_context = 0;
        context->transfer_scope = 0;
        context->candidate_buffer = 0;
        context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
        return gpu_status_from_memory_status(status);
    }
    clearra_packing_candidate_buffer_clear(context->candidate_buffer);

    context->context_created = 1u;
    context->memory_context_released = 0u;
    return CLEARRA_GPU_OK;
}ClearraGpuStatus clearra_gpu_context_destroy_memory(ClearraGpuContext *context) {
    ClrMemStatus status = CLR_MEM_OK;
    ClrMemStatus release_status;
    uint64_t epoch = 0u;

    if (context == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    if (context->memory_context == 0) {
        context->memory_context_released = 1u;
        return CLEARRA_GPU_OK;
    }

    if (context->upload_buffer_id != 0u) {
        release_status =
            clr_gpu_buffer_release(context->memory_context, context->upload_buffer_id);
        if (status == CLR_MEM_OK && release_status != CLR_MEM_OK) {
            status = release_status;
        }
    }
    if (context->readback_buffer_id != 0u) {
        release_status =
            clr_gpu_buffer_release(context->memory_context, context->readback_buffer_id);
        if (status == CLR_MEM_OK && release_status != CLR_MEM_OK) {
            status = release_status;
        }
    }

    release_status = clr_epoch_advance(context->memory_context, &epoch);
    if (status == CLR_MEM_OK && release_status != CLR_MEM_OK) {
        status = release_status;
    }
    release_status = clr_release_queue_drain(context->memory_context, epoch);
    if (status == CLR_MEM_OK && release_status != CLR_MEM_OK) {
        status = release_status;
    }
    if (context->transfer_scope != 0 &&
        !clr_scope_is_released(context->transfer_scope)) {
        release_status = clr_scope_release(context->transfer_scope);
        if (status == CLR_MEM_OK && release_status != CLR_MEM_OK) {
            status = release_status;
        }
    }

    release_status = clr_mem_context_release(&context->memory_context);
    if (status == CLR_MEM_OK && release_status != CLR_MEM_OK) {
        status = release_status;
    }

    context->memory_context = 0;
    context->transfer_scope = 0;
    context->candidate_buffer = 0;
    context->upload_buffer_id = 0u;
    context->readback_buffer_id = 0u;
    context->memory_context_released = 1u;
    return gpu_status_from_memory_status(status);
}
