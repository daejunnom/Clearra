#include "gpu/gpu_backend.h"

#include <stdlib.h>
static void fill_counts_from_result(ClearraGpuPackingResult *result) {
    result->raw_candidate_count = result->raw_candidates.count;
    result->canonical_candidate_count = result->canonical_candidates.candidates.count;
}ClearraGpuStatus clearra_gpu_dominance_prefilter_apply(
    ClearraPackingCandidateBuffer *buffer,
    uint16_t *out_removed_count) {
    if (buffer == 0 || out_removed_count == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    ClearraCanonicalPackingTable *table =
        (ClearraCanonicalPackingTable *)malloc(sizeof(*table));
    if (table == 0) {
        return CLEARRA_GPU_PACKING_ERROR;
    }
    uint16_t before_count = buffer->count;
    ClearraPackingStatus status = clearra_packing_host_reduce(buffer, table);
    if (status != CLEARRA_PACKING_OK) {
        free(table);
        return CLEARRA_GPU_PACKING_ERROR;
    }

    *buffer = table->candidates;
    *out_removed_count = (uint16_t)(before_count - buffer->count);
    free(table);
    return CLEARRA_GPU_OK;
}ClearraGpuStatus clearra_gpu_readback_compress_candidates(
    const ClearraPackingCandidateBuffer *buffer,
    ClearraCanonicalPackingTable *out_table,
    uint16_t *out_compressed_count) {
    if (buffer == 0 || out_table == 0 || out_compressed_count == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    ClearraPackingStatus status = clearra_packing_host_reduce(buffer, out_table);
    if (status != CLEARRA_PACKING_OK) {
        return CLEARRA_GPU_PACKING_ERROR;
    }
    *out_compressed_count = out_table->candidates.count;
    return CLEARRA_GPU_OK;
}ClearraGpuStatus clearra_gpu_readback_reduce_result(
    ClearraGpuPackingResult *result) {
    if (result == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    result->readback_uncompressed_count = result->raw_candidates.count;

    ClearraGpuStatus status = clearra_gpu_readback_compress_candidates(
        &result->raw_candidates,
        &result->canonical_candidates,
        &result->readback_compressed_count);
    if (status != CLEARRA_GPU_OK) {
        result->status = status;
        return status;
    }
    result->dominance_prefilter_removed_count =
        (uint16_t)(result->readback_uncompressed_count -
                   result->canonical_candidates.candidates.count);
    result->dominance_prefilter_applied = 1u;
    result->readback_compressed = 1u;

    fill_counts_from_result(result);
    return clearra_gpu_candidate_hash(
        &result->canonical_candidates.candidates, &result->gpu_candidate_hash);
}

#include "gpu/gpu_backend.h"

#include "../../include/clr_memory.h"
static ClearraGpuStatus cpu_confirm_mem_status(ClrMemStatus status) {
    if (status == CLR_MEM_OK) {
        return CLEARRA_GPU_OK;
    }
    if (status == CLR_MEM_INVALID_ARGUMENT) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    return CLEARRA_GPU_PACKING_ERROR;
}ClearraGpuStatus clearra_gpu_cpu_exact_confirm_reference(
    const ClearraGpuPackingBatchDescriptor *batch,
    const ClearraGpuPackingResult *result,
    uint8_t *out_matched,
    uint64_t *out_cpu_reference_hash) {
    if (batch == 0 || result == 0 || out_matched == 0 ||
        out_cpu_reference_hash == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    ClrMemContext *context = 0;
    ClrScope *scope = 0;
    ClearraPackingCandidateBuffer *cpu_reference = 0;
    ClearraCanonicalPackingTable *cpu_table = 0;
    ClrMemStatus mem_status = clr_mem_context_create(&context);
    if (mem_status != CLR_MEM_OK) {
        return cpu_confirm_mem_status(mem_status);
    }
    mem_status = clr_scope_create(context, CLR_SCOPE_WORKER, &scope);
    if (mem_status != CLR_MEM_OK) {
        (void)clr_mem_context_release(&context);
        return cpu_confirm_mem_status(mem_status);
    }
    if (clr_scratch_alloc(
            scope,
            sizeof(ClearraPackingCandidateBuffer),
            (void **)&cpu_reference) != CLR_MEM_OK ||
        clr_scratch_alloc(
            scope,
            sizeof(ClearraCanonicalPackingTable),
            (void **)&cpu_table) != CLR_MEM_OK) {
        (void)clr_scope_abort(scope);
        (void)clr_mem_context_release(&context);
        return CLEARRA_GPU_PACKING_ERROR;
    }

    clr_packing_problem problem;
    if (clearra_gpu_batch_descriptor_to_packing_problem(batch, &problem) !=
        CLEARRA_GPU_OK) {
        (void)clr_scope_abort(scope);
        (void)clr_mem_context_release(&context);
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    clr_resource_report resource_report;
    ClearraPackingStatus status =
        clearra_packing_enumerator_cpu_generate_problem_with_resource_report(
            &problem, cpu_reference, &resource_report);
    if (status != CLEARRA_PACKING_OK &&
        status != CLEARRA_PACKING_CAPACITY_EXCEEDED) {
        (void)clr_scope_abort(scope);
        (void)clr_mem_context_release(&context);
        return CLEARRA_GPU_PACKING_ERROR;
    }
    if (result->result_complete != (uint8_t)!resource_report.truncated ||
        result->truncation_reason !=
            (uint16_t)resource_report.truncation_reason) {
        (void)clr_scope_abort(scope);
        (void)clr_mem_context_release(&context);
        return CLEARRA_GPU_PACKING_ERROR;
    }

    status = clearra_packing_host_reduce(cpu_reference, cpu_table);
    if (status != CLEARRA_PACKING_OK) {
        (void)clr_scope_abort(scope);
        (void)clr_mem_context_release(&context);
        return CLEARRA_GPU_PACKING_ERROR;
    }

    uint64_t cpu_hash = 0;
    ClearraGpuStatus gpu_status =
        clearra_gpu_candidate_hash(&cpu_table->candidates, &cpu_hash);
    if (gpu_status != CLEARRA_GPU_OK) {
        (void)clr_scope_abort(scope);
        (void)clr_mem_context_release(&context);
        return gpu_status;
    }

    *out_cpu_reference_hash = cpu_hash;
    *out_matched = (uint8_t)(
        cpu_table->candidates.count == result->canonical_candidates.candidates.count &&
        cpu_hash == result->gpu_candidate_hash &&
        clearra_packing_candidate_buffer_exactly_matches(
            &cpu_table->candidates, &result->canonical_candidates.candidates));
    mem_status = clr_scope_release(scope);
    if (mem_status == CLR_MEM_OK) {
        mem_status = clr_mem_context_release(&context);
    } else {
        (void)clr_mem_context_release(&context);
    }
    if (mem_status != CLR_MEM_OK) {
        return cpu_confirm_mem_status(mem_status);
    }
    return CLEARRA_GPU_OK;
}

#include "gpu/gpu_backend.h"
void clearra_gpu_confirmed_candidate_queue_clear(
    ClearraGpuConfirmedCandidateQueue *queue) {
    if (queue != 0) {
        *queue = (ClearraGpuConfirmedCandidateQueue){0};
    }
}uint8_t clearra_gpu_raw_candidate_buffer_can_enter_buildup_queue(
    const ClearraPackingCandidateBuffer *raw_buffer) {
    (void)raw_buffer;
    return 0u;
}uint8_t clearra_gpu_raw_candidate_buffer_can_create_coverage_row(
    const ClearraPackingCandidateBuffer *raw_buffer) {
    (void)raw_buffer;
    return 0u;
}ClearraGpuStatus clearra_gpu_confirmed_candidate_queue_from_result(
    const ClearraGpuPackingResult *result,
    ClearraGpuConfirmedCandidateQueue *out_queue) {
    if (result == 0 || out_queue == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    if (result->cpu_exact_confirmed == 0u ||
        result->cpu_reference_matched == 0u ||
        result->candidate_is_solution != 0u) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    clearra_gpu_confirmed_candidate_queue_clear(out_queue);
    out_queue->table = &result->canonical_candidates;
    out_queue->count = result->canonical_candidates.candidates.count;
    out_queue->cpu_exact_confirmed = 1u;
    out_queue->candidate_is_solution = 0u;
    out_queue->can_enter_cpu_buildup_queue = 1u;
    out_queue->can_create_coverage_row = 0u;
    return CLEARRA_GPU_OK;
}ClearraGpuStatus clearra_gpu_confirmed_candidate_queue_candidate_at(
    const ClearraGpuConfirmedCandidateQueue *queue,
    uint16_t index,
    ClearraPackingCandidateView *out_candidate) {
    if (queue == 0 || queue->table == 0 || out_candidate == 0 ||
        queue->cpu_exact_confirmed == 0u ||
        queue->can_enter_cpu_buildup_queue == 0u ||
        queue->can_create_coverage_row != 0u ||
        queue->candidate_is_solution != 0u) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    ClearraPackingStatus status = clearra_packing_candidate_buffer_candidate_at(
        &queue->table->candidates, index, out_candidate);
    if (status == CLEARRA_PACKING_OK) {
        out_candidate->candidate_id =
            (uint64_t)queue->table->candidate_ids[index] + UINT64_C(1);
        out_candidate->canonical_operation_set_id = out_candidate->candidate_id;
    }
    return status == CLEARRA_PACKING_OK ? CLEARRA_GPU_OK
                                        : CLEARRA_GPU_PACKING_ERROR;
}

#include "gpu/gpu_backend_adapter.h"

#include <string.h>
ClearraGpuStatus clearra_gpu_backend_reject_user_provided_shader_path(
    const char *shader_path) {
    if (shader_path == 0 || shader_path[0] == '\0') {
        return CLEARRA_GPU_OK;
    }

    return CLEARRA_GPU_INVALID_ARGUMENT;
}ClearraGpuStatus clearra_gpu_backend_adapter_reject_user_shader_path(
    const char *shader_path) {
    return clearra_gpu_backend_reject_user_provided_shader_path(shader_path);
}static ClearraGpuStatus gpu_status_from_transfer_memory(ClrMemStatus status) {
    if (status == CLR_MEM_OK) {
        return CLEARRA_GPU_OK;
    }
    if (status == CLR_MEM_INVALID_ARGUMENT || status == CLR_MEM_INVALID_STATE) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    return CLEARRA_GPU_UNAVAILABLE;
}ClearraGpuStatus clearra_gpu_context_upload_batch(
    ClearraGpuContext *context,
    const ClearraGpuPackingBatchDescriptor *batch) {
    ClrMemStatus status;
    uint64_t fence_epoch;
    if (context == 0 || batch == 0 || context->memory_context == 0 ||
        context->transfer_scope == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    if (clearra_gpu_batch_descriptor_validate(batch) != CLEARRA_GPU_OK) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    context->uploaded_batch = *batch;
    status = clr_gpu_buffer_register_for_scope(
        context->memory_context,
        context->transfer_scope,
        sizeof(ClearraGpuPackingBatchDescriptor),
        &context->upload_buffer_id);
    if (status != CLR_MEM_OK) {
        context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
        return gpu_status_from_transfer_memory(status);
    }

    fence_epoch = clr_epoch_current(context->memory_context) + 1u;
    status = clr_gpu_buffer_set_fence_epoch(
        context->memory_context, context->upload_buffer_id, fence_epoch);
    if (status != CLR_MEM_OK) {
        context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
        return gpu_status_from_transfer_memory(status);
    }

    context->fence_epoch = fence_epoch;
    context->batch_uploaded = 1u;
    return CLEARRA_GPU_OK;
}ClearraGpuStatus clearra_gpu_context_readback_candidates(
    ClearraGpuContext *context,
    ClearraPackingCandidateBuffer *out_candidates) {
    ClrMemStatus status;
    if (context == 0 || out_candidates == 0 || context->memory_context == 0 ||
        context->transfer_scope == 0 || context->candidate_buffer == 0 ||
        !context->kernel_launched) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    status = clr_gpu_buffer_register_for_scope(
        context->memory_context,
        context->transfer_scope,
        sizeof(ClearraPackingCandidateBuffer),
        &context->readback_buffer_id);
    if (status != CLR_MEM_OK) {
        context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
        return gpu_status_from_transfer_memory(status);
    }

    status = clr_gpu_buffer_set_fence_epoch(
        context->memory_context, context->readback_buffer_id, context->fence_epoch);
    if (status != CLR_MEM_OK) {
        context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
        return gpu_status_from_transfer_memory(status);
    }

    *out_candidates = *context->candidate_buffer;
    context->candidates_read_back = 1u;
    return CLEARRA_GPU_OK;
}
