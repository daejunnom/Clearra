#include "hybrid_scheduler.h"

ClearraHybridStatus clearra_hybrid_buildup_variants_from_confirmed_queue(
    const clr_packing_problem *packing,
    const ClearraGpuConfirmedCandidateQueue *queue,
    clr_build_variant_buffer *out_buffer) {
    ClearraHybridBuildVariantCollection collection;
    static clr_build_variant_buffer candidate_scratch;
    return clearra_hybrid_collect_build_variants_from_confirmed_queue(
        packing,
        queue,
        CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS,
        &candidate_scratch,
        out_buffer,
        &collection);
}

#include "hybrid_scheduler.h"
static uint32_t gpu_queue_average_or_zero(uint32_t total, uint32_t count) {
    return count == 0u ? 0u : total / count;
}void clearra_hybrid_gpu_queue_init(ClearraHybridGpuQueueStats *queue) {
    if (queue != 0) {
        *queue = (ClearraHybridGpuQueueStats){0};
    }
}void clearra_hybrid_gpu_queue_submit(
    ClearraHybridGpuQueueStats *queue,
    uint32_t batch_count) {
    if (queue == 0 || batch_count == 0u) {
        return;
    }

    queue->batches_submitted += batch_count;
    queue->pending_batches += batch_count;
    if (queue->pending_batches > queue->max_queue_depth) {
        queue->max_queue_depth = queue->pending_batches;
    }
}void clearra_hybrid_gpu_queue_complete(
    ClearraHybridGpuQueueStats *queue,
    uint32_t batch_count,
    uint32_t latency_ms) {
    if (queue == 0 || batch_count == 0u) {
        return;
    }

    queue->batches_completed += batch_count;
    queue->total_batch_latency_ms += latency_ms;
    if (batch_count >= queue->pending_batches) {
        queue->pending_batches = 0u;
    } else {
        queue->pending_batches -= batch_count;
    }
}void clearra_hybrid_gpu_queue_apply_metrics(
    const ClearraHybridGpuQueueStats *queue,
    ClearraHybridBackendMetrics *backend_metrics,
    ClearraHybridAutotuneMetrics *autotune_metrics) {
    if (queue == 0) {
        return;
    }

    if (backend_metrics != 0) {
        backend_metrics->gpu_batches_submitted = queue->batches_submitted;
        backend_metrics->gpu_batches_completed = queue->batches_completed;
        backend_metrics->gpu_queue_depth = (uint16_t)queue->max_queue_depth;
        backend_metrics->average_batch_latency_ms =
            gpu_queue_average_or_zero(queue->total_batch_latency_ms, queue->batches_completed);
    }
    if (autotune_metrics != 0) {
        autotune_metrics->gpu_batches_submitted = queue->batches_submitted;
        autotune_metrics->gpu_batches_completed = queue->batches_completed;
        autotune_metrics->average_batch_latency_ms =
            gpu_queue_average_or_zero(queue->total_batch_latency_ms, queue->batches_completed);
    }
}

#include "hybrid_scheduler.h"
void clearra_hybrid_readback_queue_init(ClearraHybridReadbackQueueStats *queue) {
    if (queue != 0) {
        *queue = (ClearraHybridReadbackQueueStats){0};
    }
}void clearra_hybrid_readback_queue_enqueue(
    ClearraHybridReadbackQueueStats *queue,
    uint32_t batch_count,
    uint32_t candidate_pressure) {
    if (queue == 0 || batch_count == 0u) {
        return;
    }

    queue->batches_enqueued += batch_count;
    queue->pending_batches += batch_count;
    if (queue->pending_batches > queue->max_pending_batches) {
        queue->max_pending_batches = queue->pending_batches;
    }
    if (candidate_pressure > queue->candidate_buffer_pressure) {
        queue->candidate_buffer_pressure = candidate_pressure;
    }
}void clearra_hybrid_readback_queue_complete(
    ClearraHybridReadbackQueueStats *queue,
    uint32_t batch_count) {
    if (queue == 0 || batch_count == 0u) {
        return;
    }

    queue->batches_completed += batch_count;
    if (batch_count >= queue->pending_batches) {
        queue->pending_batches = 0u;
    } else {
        queue->pending_batches -= batch_count;
    }
}void clearra_hybrid_readback_queue_apply_metrics(
    const ClearraHybridReadbackQueueStats *queue,
    ClearraHybridBackendMetrics *backend_metrics,
    ClearraHybridAutotuneMetrics *autotune_metrics) {
    if (queue == 0) {
        return;
    }

    if (backend_metrics != 0) {
        backend_metrics->readback_pending_batches =
            (uint16_t)queue->max_pending_batches;
        backend_metrics->gpu_readback_pending = queue->max_pending_batches;
        backend_metrics->candidate_buffer_pressure =
            queue->candidate_buffer_pressure;
    }
    if (autotune_metrics != 0) {
        autotune_metrics->gpu_readback_pending = queue->max_pending_batches;
        autotune_metrics->candidate_buffer_pressure =
            queue->candidate_buffer_pressure;
    }
}

#include "hybrid_scheduler.h"
static uint32_t cpu_confirm_average_or_zero(uint32_t total, uint32_t count) {
    return count == 0u ? 0u : total / count;
}void clearra_hybrid_cpu_confirm_queue_init(
    ClearraHybridCpuConfirmQueueStats *queue) {
    if (queue != 0) {
        *queue = (ClearraHybridCpuConfirmQueueStats){0};
    }
}void clearra_hybrid_cpu_confirm_queue_enqueue(
    ClearraHybridCpuConfirmQueueStats *queue,
    uint32_t candidate_count) {
    if (queue == 0 || candidate_count == 0u) {
        return;
    }

    queue->candidates_enqueued += candidate_count;
    if (candidate_count > queue->max_confirm_queue_depth) {
        queue->max_confirm_queue_depth = candidate_count;
    }
}void clearra_hybrid_cpu_confirm_queue_complete(
    ClearraHybridCpuConfirmQueueStats *queue,
    uint32_t candidate_count,
    uint32_t build_variant_count,
    uint32_t latency_ms) {
    if (queue == 0 || candidate_count == 0u) {
        return;
    }

    queue->candidates_confirmed += candidate_count;
    queue->build_variants_enqueued += build_variant_count;
    queue->total_confirm_latency_ms += latency_ms;
    if (build_variant_count > queue->max_buildup_queue_depth) {
        queue->max_buildup_queue_depth = build_variant_count;
    }
}void clearra_hybrid_cpu_confirm_queue_apply_metrics(
    const ClearraHybridCpuConfirmQueueStats *queue,
    ClearraHybridBackendMetrics *backend_metrics,
    ClearraHybridAutotuneMetrics *autotune_metrics) {
    if (queue == 0) {
        return;
    }

    if (backend_metrics != 0) {
        backend_metrics->cpu_confirm_queue_depth =
            queue->max_confirm_queue_depth;
        backend_metrics->cpu_exact_confirm_queue_depth =
            (uint16_t)queue->max_confirm_queue_depth;
        backend_metrics->cpu_buildup_queue_depth =
            queue->max_buildup_queue_depth;
        backend_metrics->cpu_buildup_backlog =
            (uint16_t)queue->max_buildup_queue_depth;
        backend_metrics->average_cpu_confirm_latency_ms =
            cpu_confirm_average_or_zero(
                queue->total_confirm_latency_ms,
                queue->candidates_confirmed);
    }
    if (autotune_metrics != 0) {
        autotune_metrics->cpu_confirm_queue_depth =
            queue->max_confirm_queue_depth;
        autotune_metrics->cpu_buildup_queue_depth =
            queue->max_buildup_queue_depth;
        autotune_metrics->average_cpu_confirm_latency_ms =
            cpu_confirm_average_or_zero(
                queue->total_confirm_latency_ms,
                queue->candidates_confirmed);
    }
}

#include "hybrid_scheduler.h"

#include <time.h>
static ClearraHybridStatus memory_status_to_hybrid(ClrMemStatus status) {
    return status == CLR_MEM_OK ? CLEARRA_HYBRID_OK : CLEARRA_HYBRID_MEMORY_ERROR;
}static uint32_t elapsed_ms_since(clock_t started) {
    clock_t ended = clock();
    double elapsed_ms;
    if (ended <= started) {
        return 1u;
    }
    elapsed_ms = ((double)(ended - started) * 1000.0) / (double)CLOCKS_PER_SEC;
    if (elapsed_ms < 1.0) {
        return 1u;
    }
    if (elapsed_ms > 4294967295.0) {
        return UINT32_MAX;
    }
    return (uint32_t)elapsed_ms;
}void clearra_hybrid_copy_gpu_worker_metrics(
    const ClearraGpuWorkerResult *worker_result,
    ClearraHybridBackendMetrics *metrics) {
    if (worker_result == 0 || metrics == 0) {
        return;
    }

    metrics->gpu_worker_request_submitted = 1u;
    metrics->gpu_worker_request_id = worker_result->request_id;
    metrics->gpu_worker_memory_ticket_id = worker_result->memory_ticket_id;
    metrics->gpu_worker_fence_epoch = worker_result->fence_epoch;
    metrics->gpu_worker_trust_state = worker_result->trust_state;
    metrics->gpu_queue_depth = worker_result->backpressure.gpu_queue_depth;
    metrics->readback_pending_batches =
        worker_result->backpressure.readback_pending_batches;
}ClearraHybridStatus clearra_hybrid_submit_gpu_worker_request(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuWorkerResult *out_worker_result,
    uint32_t *out_latency_ms) {
    ClrMemContext *context = 0;
    ClrScope *gpu_transfer_scope = 0;
    uint64_t epoch = 0;
    clock_t worker_started;
    ClearraHybridStatus status;

    if (batch == 0 || out_worker_result == 0 || out_latency_ms == 0) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }
    *out_latency_ms = 0u;

    status = memory_status_to_hybrid(clr_mem_context_create(&context));
    if (status != CLEARRA_HYBRID_OK) {
        return status;
    }
    status = memory_status_to_hybrid(
        clr_scope_create(context, CLR_SCOPE_GPU_TRANSFER, &gpu_transfer_scope));
    if (status != CLEARRA_HYBRID_OK) {
        goto cleanup;
    }

    worker_started = clock();
    if (clearra_gpu_worker_scheduler_bridge_run(
            context, gpu_transfer_scope, batch, out_worker_result) !=
        CLEARRA_GPU_WORKER_OK) {
        status = CLEARRA_HYBRID_GPU_UNAVAILABLE;
        goto cleanup;
    }
    *out_latency_ms = elapsed_ms_since(worker_started);

    status = memory_status_to_hybrid(
        clr_release_queue_defer_scope(context, gpu_transfer_scope, 1));
    if (status != CLEARRA_HYBRID_OK) {
        goto cleanup;
    }
    status = memory_status_to_hybrid(clr_epoch_advance(context, &epoch));
    if (status != CLEARRA_HYBRID_OK) {
        goto cleanup;
    }
    status = memory_status_to_hybrid(clr_release_queue_drain(context, epoch));

cleanup:
    if (status != CLEARRA_HYBRID_OK && gpu_transfer_scope != 0 &&
        !clr_scope_is_released(gpu_transfer_scope)) {
        (void)clr_scope_abort(gpu_transfer_scope);
    }
    if (context != 0 &&
        clr_mem_context_release(&context) != CLR_MEM_OK &&
        status == CLEARRA_HYBRID_OK) {
        status = CLEARRA_HYBRID_MEMORY_ERROR;
    }
    return status;
}
