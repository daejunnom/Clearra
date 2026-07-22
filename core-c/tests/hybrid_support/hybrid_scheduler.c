#include "hybrid_scheduler.h"

/* Implementations are split by scheduler responsibility. */

#include "hybrid_scheduler.h"
#include "hybrid_scheduler_internal.h"

#include <stddef.h>
#include <string.h>
#include <time.h>

ClearraHybridStatus clearra_hybrid_status_from_packing(ClearraPackingStatus status) {
    return status == CLEARRA_PACKING_OK ? CLEARRA_HYBRID_OK : CLEARRA_HYBRID_PACKING_ERROR;
}

#include "hybrid_scheduler.h"
#include "hybrid_scheduler_internal.h"

#include <stddef.h>
#include <string.h>
#include <time.h>

ClearraHybridStatus clearra_hybrid_status_from_memory(ClrMemStatus status) {
    return status == CLR_MEM_OK ? CLEARRA_HYBRID_OK : CLEARRA_HYBRID_MEMORY_ERROR;
}

#include "hybrid_scheduler.h"
#include "hybrid_scheduler_internal.h"

#include <stddef.h>
#include <string.h>
#include <time.h>

uint32_t clearra_hybrid_elapsed_ms_since(clock_t started) {
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
}

#include "hybrid_scheduler.h"
#include "hybrid_scheduler_internal.h"

#include <stddef.h>
#include <string.h>
#include <time.h>

void clearra_hybrid_scheduler_result_clear(ClearraHybridSchedulerResult *result) {
    if (result == 0) {
        return;
    }
    memset(result, 0, sizeof(*result));
}

#include "hybrid_scheduler.h"
#include "hybrid_scheduler_internal.h"

#include <stddef.h>
#include <string.h>
#include <time.h>

ClearraHybridStatus clearra_hybrid_reduce_cpu_reference(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *raw,
    ClearraCanonicalPackingTable *out_table) {
    clr_packing_problem problem;
    if (raw == 0 ||
        clearra_gpu_batch_descriptor_to_packing_problem(batch, &problem) !=
            CLEARRA_GPU_OK) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }

    clr_resource_report resource_report;
    ClearraPackingStatus status =
        clearra_packing_enumerator_cpu_generate_problem_with_resource_report(
            &problem, raw, &resource_report);
    if (status != CLEARRA_PACKING_OK &&
        status != CLEARRA_PACKING_CAPACITY_EXCEEDED) {
        return clearra_hybrid_status_from_packing(status);
    }
    status = clearra_packing_host_reduce(raw, out_table);
    return clearra_hybrid_status_from_packing(status);
}

#include "hybrid_scheduler.h"
#include "hybrid_scheduler_internal.h"

#include <stddef.h>
#include <string.h>
#include <time.h>

ClearraHybridStatus clearra_hybrid_finish_result(
    const clr_packing_problem *packing,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuConfirmedCandidateQueue *confirmed_queue,
    const ClearraGpuWorkerResult *worker_result,
    uint32_t gpu_worker_latency_ms,
    ClearraGpuUnavailableReason fallback_reason,
    uint8_t fallback_used,
    ClearraHybridSchedulerResult *out_result) {
    ClrMemContext *scratch_context = 0;
    ClrScope *scratch_scope = 0;
    ClearraHybridScratch scratch = {0};
    uint64_t scratch_epoch = 0;
    ClearraHybridStatus status = clearra_hybrid_status_from_memory(
        clr_mem_context_create(&scratch_context));
    out_result->metrics.failure_stage = CLEARRA_HYBRID_STAGE_SCRATCH;
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        return status;
    }
    status = clearra_hybrid_status_from_memory(
        clr_scope_create(scratch_context, CLR_SCOPE_WORKER, &scratch_scope));
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        goto cleanup;
    }
    status = clearra_hybrid_scratch_create(scratch_scope, &scratch);
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        goto cleanup;
    }

    out_result->metrics.failure_stage =
        CLEARRA_HYBRID_STAGE_CPU_REFERENCE_PACKING;
    status = clearra_hybrid_reduce_cpu_reference(batch, scratch.cpu_raw_candidates, scratch.cpu_table);
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        goto cleanup;
    }

    ClearraGpuConfirmedCandidateQueue cpu_reference_queue = {
        .table = scratch.cpu_table,
        .count = scratch.cpu_table->candidates.count,
        .cpu_exact_confirmed = 1u,
        .candidate_is_solution = 0u,
        .can_enter_cpu_buildup_queue = 1u,
        .can_create_coverage_row = 0u,
    };
    ClearraHybridBuildVariantCollection cpu_collection;
    out_result->metrics.failure_stage = CLEARRA_HYBRID_STAGE_CPU_BUILDUP;
    status = clearra_hybrid_collect_build_variants_from_confirmed_queue(
        packing,
        &cpu_reference_queue,
        CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS,
        scratch.candidate_variants,
        scratch.cpu_variants,
        &cpu_collection);
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        goto cleanup;
    }
    ClearraHybridBuildVariantCollection hybrid_collection;
    clock_t cpu_confirm_started = clock();
    out_result->metrics.failure_stage = CLEARRA_HYBRID_STAGE_HYBRID_BUILDUP;
    status = clearra_hybrid_collect_build_variants_from_confirmed_queue(
        packing,
        confirmed_queue,
        CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS,
        scratch.candidate_variants,
        scratch.hybrid_variants,
        &hybrid_collection);
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        goto cleanup;
    }
    uint32_t cpu_confirm_latency_ms = clearra_hybrid_elapsed_ms_since(cpu_confirm_started);

    ClearraHybridCoverageRowBridgeReport cpu_coverage_report;
    out_result->metrics.failure_stage = CLEARRA_HYBRID_STAGE_CPU_COVERAGE;
    status = clearra_hybrid_coverage_rows_from_build_variants(
        CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS,
        scratch.cpu_variants,
        batch->piece_source_id,
        batch->pattern_universe_id,
        batch->pattern_weight_model_id,
        batch->pattern_count,
        scratch.cpu_coverage_rows,
        CLR_BUILDUP_MAX_VARIANTS,
        &cpu_coverage_report);
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        goto cleanup;
    }
    ClearraHybridCoverageRowBridgeReport hybrid_coverage_report;
    out_result->metrics.failure_stage = CLEARRA_HYBRID_STAGE_HYBRID_COVERAGE;
    status = clearra_hybrid_coverage_rows_from_build_variants(
        CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS,
        scratch.hybrid_variants,
        batch->piece_source_id,
        batch->pattern_universe_id,
        batch->pattern_weight_model_id,
        batch->pattern_count,
        scratch.hybrid_coverage_rows,
        CLR_BUILDUP_MAX_VARIANTS,
        &hybrid_coverage_report);
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        goto cleanup;
    }

    ClearraHybridGpuQueueStats gpu_queue;
    ClearraHybridReadbackQueueStats readback_queue;
    ClearraHybridCpuConfirmQueueStats cpu_confirm_queue;
    ClearraHybridAutotuneMetrics autotune_metrics = {0};
    ClearraHybridAutotuneDecision autotune_decision;
    ClearraHybridAutotuneBudget autotune_budget =
        clearra_hybrid_autotune_budget_default();
    uint32_t readback_candidate_pressure =
        confirmed_queue->table != NULL
            ? confirmed_queue->table->raw_count
            : confirmed_queue->count;

    clearra_hybrid_gpu_queue_init(&gpu_queue);
    clearra_hybrid_readback_queue_init(&readback_queue);
    clearra_hybrid_cpu_confirm_queue_init(&cpu_confirm_queue);
    if (worker_result != 0) {
        clearra_hybrid_gpu_queue_submit(&gpu_queue, 1u);
        clearra_hybrid_gpu_queue_complete(
            &gpu_queue, 1u, gpu_worker_latency_ms);
    }
    if (confirmed_queue->count > 0u) {
        if (!fallback_used) {
            clearra_hybrid_readback_queue_enqueue(
                &readback_queue, 1u, readback_candidate_pressure);
            clearra_hybrid_readback_queue_complete(&readback_queue, 1u);
        }
        clearra_hybrid_cpu_confirm_queue_enqueue(
            &cpu_confirm_queue, confirmed_queue->count);
        clearra_hybrid_cpu_confirm_queue_complete(
            &cpu_confirm_queue,
            confirmed_queue->count,
            hybrid_collection.variant_count,
            cpu_confirm_latency_ms);
    }

    out_result->metrics.cpu_preprocessor_batch_descriptor_created = 1u;
    clearra_hybrid_copy_gpu_worker_metrics(worker_result, &out_result->metrics);
    out_result->metrics.cpu_reference_candidate_count = scratch.cpu_table->candidates.count;
    out_result->metrics.hybrid_candidate_count = confirmed_queue->count;
    out_result->metrics.cpu_reference_build_variant_count = cpu_collection.variant_count;
    out_result->metrics.hybrid_build_variant_count = hybrid_collection.variant_count;
    out_result->metrics.coverage_row_buffer_pressure =
        hybrid_coverage_report.row_count;
    clearra_hybrid_gpu_queue_apply_metrics(
        &gpu_queue, &out_result->metrics, &autotune_metrics);
    clearra_hybrid_readback_queue_apply_metrics(
        &readback_queue, &out_result->metrics, &autotune_metrics);
    clearra_hybrid_cpu_confirm_queue_apply_metrics(
        &cpu_confirm_queue, &out_result->metrics, &autotune_metrics);
    autotune_metrics.coverage_row_buffer_pressure =
        out_result->metrics.coverage_row_buffer_pressure;
    autotune_metrics.memory_ticket_live_count =
        worker_result != 0 ? 1u : 0u;
    autotune_metrics.pending_release_queue_depth = 0u;
    out_result->metrics.memory_ticket_live_count =
        autotune_metrics.memory_ticket_live_count;
    out_result->metrics.pending_release_queue_depth =
        autotune_metrics.pending_release_queue_depth;
    autotune_decision =
        clearra_hybrid_autotune_evaluate(&autotune_budget, &autotune_metrics);
    out_result->metrics.cpu_exact_confirm_queue_received =
        (uint8_t)(confirmed_queue->count > 0u);
    out_result->metrics.gpu_assisted_buildup_reached =
        (uint8_t)(hybrid_collection.variant_count > 0u);
    out_result->metrics.buildup_dispatch_mode =
        (uint8_t)CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS;
    out_result->metrics.cpu_reference_coverage_row_count =
        cpu_coverage_report.row_count;
    out_result->metrics.hybrid_coverage_row_count =
        hybrid_coverage_report.row_count;
    out_result->metrics.coverage_rows_from_enumerate_variants =
        hybrid_coverage_report.from_enumerate_variants;
    out_result->metrics.verify_first_used_for_coverage =
        hybrid_coverage_report.rejected_verify_first;
    out_result->metrics.memory_pressure_level =
        (uint8_t)autotune_decision.memory_pressure.level;
    out_result->metrics.fallback_reason = fallback_reason;
    out_result->metrics.fallback_used = fallback_used;
    out_result->metrics.backend_metrics_reported = 1u;
    out_result->metrics.batch_buffers_reused =
        clearra_hybrid_triple_buffer_pipeline_reuse_count(&out_result->plan);
    out_result->metrics.gpu_readback_overlap_steps = fallback_used
        ? 0u
        : clearra_hybrid_triple_buffer_pipeline_overlap_steps(&out_result->plan);
    out_result->metrics.work_steal_count =
        clearra_hybrid_work_stealing_assign_small_irregular_buildup(
            confirmed_queue->table);
    out_result->metrics.gpu_only_packing_cpu_buildup_matches_cpu_reference =
        (uint8_t)(out_result->metrics.cpu_reference_candidate_count ==
                      out_result->metrics.hybrid_candidate_count &&
                  out_result->metrics.cpu_reference_build_variant_count ==
                      out_result->metrics.hybrid_build_variant_count);

    out_result->metrics.failure_stage = CLEARRA_HYBRID_STAGE_RESULT_COMPARE;
    if (!out_result->metrics.gpu_only_packing_cpu_buildup_matches_cpu_reference) {
        out_result->status = CLEARRA_HYBRID_RESULT_MISMATCH;
        status = out_result->status;
        goto cleanup;
    }

    out_result->backpressure = clearra_hybrid_backpressure_report_for(
        &out_result->plan, &out_result->metrics);

    out_result->metrics.failure_stage = CLEARRA_HYBRID_STAGE_MEMORY_EPOCH;
    status = clearra_hybrid_manage_memory_epoch(out_result);
    out_result->status = status;
    if (status == CLEARRA_HYBRID_OK) {
        out_result->metrics.failure_stage = CLEARRA_HYBRID_STAGE_NONE;
    }

cleanup:
    if (scratch_scope != 0 && !clr_scope_is_released(scratch_scope)) {
        ClrMemStatus release_status;
        if (status == CLEARRA_HYBRID_OK) {
            release_status = clr_release_queue_defer_scope(scratch_context, scratch_scope, 1);
            if (release_status == CLR_MEM_OK) {
                release_status = clr_epoch_advance(scratch_context, &scratch_epoch);
            }
            if (release_status == CLR_MEM_OK) {
                release_status = clr_release_queue_drain(scratch_context, scratch_epoch);
            }
        } else {
            release_status = clr_scope_abort(scratch_scope);
        }
        if (release_status != CLR_MEM_OK && status == CLEARRA_HYBRID_OK) {
            status = CLEARRA_HYBRID_MEMORY_ERROR;
            out_result->status = status;
        }
    }
    if (scratch_context != 0 &&
        clr_mem_context_release(&scratch_context) != CLR_MEM_OK &&
        status == CLEARRA_HYBRID_OK) {
        status = CLEARRA_HYBRID_MEMORY_ERROR;
        out_result->status = status;
    }
    return status;
}

#include "hybrid_scheduler.h"
#include "hybrid_scheduler_internal.h"

#include <stddef.h>
#include <string.h>
#include <time.h>

ClearraHybridStatus clearra_hybrid_scheduler_run(
    const clr_packing_problem *packing,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuDeviceRequest request,
    bool allow_backend_fallback,
    ClearraHybridSchedulerResult *out_result) {
    if (packing == 0 || batch == 0 || out_result == 0) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }

    clearra_hybrid_scheduler_result_clear(out_result);
    out_result->plan = clearra_hybrid_batch_plan_for(batch);

    ClrMemContext *gpu_context = 0;
    ClrScope *gpu_scope = 0;
    ClearraGpuPackingResult *gpu_result = 0;
    ClearraGpuConfirmedCandidateQueue confirmed_queue;
    ClearraHybridStatus status = clearra_hybrid_status_from_memory(
        clr_mem_context_create(&gpu_context));
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        return status;
    }
    status = clearra_hybrid_status_from_memory(
        clr_scope_create(gpu_context, CLR_SCOPE_WORKER, &gpu_scope));
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        (void)clr_mem_context_release(&gpu_context);
        return status;
    }
    if (clr_scratch_alloc(
            gpu_scope,
            sizeof(ClearraGpuPackingResult),
            (void **)&gpu_result) != CLR_MEM_OK) {
        out_result->status = CLEARRA_HYBRID_MEMORY_ERROR;
        (void)clr_scope_abort(gpu_scope);
        (void)clr_mem_context_release(&gpu_context);
        return out_result->status;
    }

    ClearraGpuStatus gpu_status =
        clearra_gpu_packing_backend_run(batch, request, allow_backend_fallback, gpu_result);
    if (gpu_status != CLEARRA_GPU_OK) {
        out_result->status = CLEARRA_HYBRID_GPU_UNAVAILABLE;
        out_result->metrics.fallback_reason = gpu_result->unavailable_reason;
        out_result->metrics.backend_metrics_reported = 1u;
        (void)clr_scope_abort(gpu_scope);
        (void)clr_mem_context_release(&gpu_context);
        return out_result->status;
    }
    if (clearra_gpu_confirmed_candidate_queue_from_result(
            gpu_result, &confirmed_queue) != CLEARRA_GPU_OK) {
        out_result->status = CLEARRA_HYBRID_PACKING_ERROR;
        (void)clr_scope_abort(gpu_scope);
        (void)clr_mem_context_release(&gpu_context);
        return out_result->status;
    }

    ClearraGpuWorkerResult worker_result;
    uint32_t gpu_worker_latency_ms = 0u;
    ClearraGpuWorkerResult *worker_result_ptr = 0;
    if (gpu_result->used_cpu_fallback == 0u) {
        ClearraHybridStatus worker_status =
            clearra_hybrid_submit_gpu_worker_request(
                batch, &worker_result, &gpu_worker_latency_ms);
        if (worker_status != CLEARRA_HYBRID_OK) {
            out_result->status = worker_status;
            (void)clr_scope_abort(gpu_scope);
            (void)clr_mem_context_release(&gpu_context);
            return worker_status;
        }
        worker_result_ptr = &worker_result;
    }

    status = clearra_hybrid_finish_result(
        packing,
        batch,
        &confirmed_queue,
        worker_result_ptr,
        gpu_worker_latency_ms,
        gpu_result->unavailable_reason,
        gpu_result->used_cpu_fallback,
        out_result);
    if (status == CLEARRA_HYBRID_OK) {
        (void)clr_scope_release(gpu_scope);
    } else {
        (void)clr_scope_abort(gpu_scope);
    }
    if (clr_mem_context_release(&gpu_context) != CLR_MEM_OK &&
        status == CLEARRA_HYBRID_OK) {
        status = CLEARRA_HYBRID_MEMORY_ERROR;
        out_result->status = status;
    }
    return status;
}

#include "hybrid_scheduler.h"
static ClearraHybridStatus reference_memory_status_to_hybrid(ClrMemStatus status) {
    return status == CLR_MEM_OK ? CLEARRA_HYBRID_OK : CLEARRA_HYBRID_MEMORY_ERROR;
}ClearraHybridStatus clearra_hybrid_scheduler_run_cpu_fallback(
    const clr_packing_problem *packing,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraHybridSchedulerResult *out_result) {
    return clearra_hybrid_scheduler_run_cpu_fallback_candidates(
        packing, batch, out_result, 0);
}ClearraHybridStatus clearra_hybrid_scheduler_run_cpu_fallback_candidates(
    const clr_packing_problem *packing,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraHybridSchedulerResult *out_result,
    ClearraPackingCandidateBuffer *out_confirmed_candidates) {
    if (packing == 0 || batch == 0 || out_result == 0) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }

    clearra_hybrid_scheduler_result_clear(out_result);
    if (out_confirmed_candidates != 0) {
        clearra_packing_candidate_buffer_clear(out_confirmed_candidates);
    }
    out_result->plan = clearra_hybrid_batch_plan_for(batch);

    ClrMemContext *gpu_context = 0;
    ClrScope *gpu_scope = 0;
    ClearraGpuPackingResult *gpu_result = 0;
    ClearraGpuConfirmedCandidateQueue confirmed_queue;
    ClearraHybridStatus status = reference_memory_status_to_hybrid(
        clr_mem_context_create(&gpu_context));
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        return status;
    }
    status = reference_memory_status_to_hybrid(
        clr_scope_create(gpu_context, CLR_SCOPE_WORKER, &gpu_scope));
    if (status != CLEARRA_HYBRID_OK) {
        out_result->status = status;
        (void)clr_mem_context_release(&gpu_context);
        return status;
    }
    if (clr_scratch_alloc(
            gpu_scope,
            sizeof(ClearraGpuPackingResult),
            (void **)&gpu_result) != CLR_MEM_OK) {
        out_result->status = CLEARRA_HYBRID_MEMORY_ERROR;
        (void)clr_scope_abort(gpu_scope);
        (void)clr_mem_context_release(&gpu_context);
        return out_result->status;
    }

    out_result->metrics.failure_stage = CLEARRA_HYBRID_STAGE_GPU_PACKING;
    ClearraGpuStatus gpu_status =
        clearra_cpu_packing_reference_run(batch, gpu_result);
    if (gpu_status != CLEARRA_GPU_OK) {
        out_result->status = CLEARRA_HYBRID_PACKING_ERROR;
        (void)clr_scope_abort(gpu_scope);
        (void)clr_mem_context_release(&gpu_context);
        return out_result->status;
    }
    out_result->metrics.failure_stage = CLEARRA_HYBRID_STAGE_CONFIRMED_QUEUE;
    if (clearra_gpu_confirmed_candidate_queue_from_result(
            gpu_result, &confirmed_queue) != CLEARRA_GPU_OK) {
        out_result->status = CLEARRA_HYBRID_PACKING_ERROR;
        (void)clr_scope_abort(gpu_scope);
        (void)clr_mem_context_release(&gpu_context);
        return out_result->status;
    }
    if (out_confirmed_candidates != 0) {
        *out_confirmed_candidates = confirmed_queue.table->candidates;
    }

    status = clearra_hybrid_finish_result(
        packing,
        batch,
        &confirmed_queue,
        0,
        0u,
        CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE,
        1u,
        out_result);
    if (status == CLEARRA_HYBRID_OK) {
        (void)clr_scope_release(gpu_scope);
    } else {
        (void)clr_scope_abort(gpu_scope);
    }
    if (clr_mem_context_release(&gpu_context) != CLR_MEM_OK &&
        status == CLEARRA_HYBRID_OK) {
        status = CLEARRA_HYBRID_MEMORY_ERROR;
        out_result->status = status;
    }
    return status;
}

#include "hybrid_scheduler.h"

ClearraHybridStatus clearra_hybrid_scratch_create(
    ClrScope *owner_scope,
    ClearraHybridScratch *out_scratch) {
    ClearraHybridScratch scratch = {0};
    if (owner_scope == 0 || out_scratch == 0) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }

    scratch.owner_scope = owner_scope;
    if (clr_arena_alloc(
            owner_scope,
            sizeof(ClearraCanonicalPackingTable),
            (void **)&scratch.cpu_table) != CLR_MEM_OK ||
        clr_arena_alloc(
            owner_scope,
            sizeof(ClearraPackingCandidateBuffer),
            (void **)&scratch.cpu_raw_candidates) != CLR_MEM_OK ||
        clr_arena_alloc(
            owner_scope,
            sizeof(clr_build_variant_buffer),
            (void **)&scratch.candidate_variants) != CLR_MEM_OK ||
        clr_arena_alloc(
            owner_scope,
            sizeof(clr_build_variant_buffer),
            (void **)&scratch.cpu_variants) != CLR_MEM_OK ||
        clr_arena_alloc(
            owner_scope,
            sizeof(clr_build_variant_buffer),
            (void **)&scratch.hybrid_variants) != CLR_MEM_OK ||
        clr_arena_alloc(
            owner_scope,
            sizeof(clr_coverage_row_view) * CLR_BUILDUP_MAX_VARIANTS,
            (void **)&scratch.cpu_coverage_rows) != CLR_MEM_OK ||
        clr_arena_alloc(
            owner_scope,
            sizeof(clr_coverage_row_view) * CLR_BUILDUP_MAX_VARIANTS,
            (void **)&scratch.hybrid_coverage_rows) != CLR_MEM_OK) {
        *out_scratch = (ClearraHybridScratch){0};
        return CLEARRA_HYBRID_MEMORY_ERROR;
    }

    *out_scratch = scratch;
    return CLEARRA_HYBRID_OK;
}

#include "hybrid_scheduler.h"
static ClearraHybridStatus memory_epoch_status_to_hybrid(ClrMemStatus status) {
    return status == CLR_MEM_OK ? CLEARRA_HYBRID_OK : CLEARRA_HYBRID_MEMORY_ERROR;
}ClearraHybridStatus clearra_hybrid_manage_memory_epoch(
    ClearraHybridSchedulerResult *result) {
    ClrMemContext *context = 0;
    ClrScope *search_scope = 0;
    ClrScope *batch_scope = 0;
    ClrScope *gpu_scope = 0;
    void *memory = 0;
    uint64_t buffer_id = 0;
    uint64_t epoch = 0;

    if (result == 0 || clr_mem_context_create(&context) != CLR_MEM_OK) {
        return CLEARRA_HYBRID_MEMORY_ERROR;
    }

    result->metrics.memory_epoch_start = clr_epoch_current(context);
    if (clr_scope_create(context, CLR_SCOPE_SEARCH, &search_scope) != CLR_MEM_OK ||
        clr_scope_create(context, CLR_SCOPE_BATCH, &batch_scope) != CLR_MEM_OK ||
        clr_scope_create(context, CLR_SCOPE_GPU_TRANSFER, &gpu_scope) != CLR_MEM_OK) {
        (void)clr_mem_context_release(&context);
        return CLEARRA_HYBRID_MEMORY_ERROR;
    }
    if (clr_arena_alloc(search_scope, 16, &memory) != CLR_MEM_OK ||
        clr_pool_alloc(batch_scope, 32, &memory) != CLR_MEM_OK ||
        clr_scratch_alloc(gpu_scope, 32, &memory) != CLR_MEM_OK) {
        (void)clr_mem_context_release(&context);
        return CLEARRA_HYBRID_MEMORY_ERROR;
    }
    if (clr_gpu_buffer_register(context, 1024, &buffer_id) != CLR_MEM_OK ||
        clr_gpu_buffer_set_fence_epoch(context, buffer_id, 1) != CLR_MEM_OK ||
        clr_gpu_buffer_release(context, buffer_id) != CLR_MEM_OK) {
        (void)clr_mem_context_release(&context);
        return CLEARRA_HYBRID_MEMORY_ERROR;
    }
    if (clr_release_queue_defer_scope(context, batch_scope, 1) != CLR_MEM_OK ||
        clr_release_queue_defer_scope(context, gpu_scope, 1) != CLR_MEM_OK) {
        (void)clr_mem_context_release(&context);
        return CLEARRA_HYBRID_MEMORY_ERROR;
    }
    if (clr_scope_release(search_scope) != CLR_MEM_OK ||
        clr_epoch_advance(context, &epoch) != CLR_MEM_OK ||
        clr_release_queue_drain(context, epoch) != CLR_MEM_OK ||
        clr_mem_context_leak_report(context, &result->leak_report) != CLR_MEM_OK) {
        (void)clr_mem_context_release(&context);
        return CLEARRA_HYBRID_MEMORY_ERROR;
    }

    result->metrics.memory_epoch_end = epoch;
    result->metrics.memory_leak_report_clean =
        (uint8_t)(result->leak_report.live_scopes == 0 &&
                  result->leak_report.live_allocations == 0 &&
                  result->leak_report.live_gpu_buffers == 0 &&
                  result->leak_report.pending_release_queue == 0);

    if (clr_mem_context_release(&context) != CLR_MEM_OK) {
        return CLEARRA_HYBRID_MEMORY_ERROR;
    }
    return memory_epoch_status_to_hybrid(
        result->metrics.memory_leak_report_clean ? CLR_MEM_OK : CLR_MEM_INVALID_STATE);
}
