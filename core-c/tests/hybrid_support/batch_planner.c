#include "hybrid_scheduler.h"

ClearraHybridBatchPlan clearra_hybrid_batch_plan_for(
    const ClearraGpuPackingBatchDescriptor *batch) {
    ClearraHybridBatchPlan plan = {0};
    uint16_t threshold = clearra_hybrid_backend_autotune_large_batch_threshold();
    plan.large_batch_threshold = threshold;
    plan.cpu_worker_count = clearra_hybrid_worker_pool_cpu_workers();
    plan.gpu_worker_count = clearra_hybrid_worker_pool_gpu_workers();
    plan.cpu_small_irregular_buildup = 1u;
    plan.gpu_readback_cpu_buildup_overlap = 1u;
    plan.batch_buffer_reuse = 1u;
    plan.memory_epoch_managed = 1u;
    plan.triple_buffer_count = 3u;
    if (batch != 0 && batch->piece_count >= threshold) {
        plan.gpu_large_packing_batch = 1u;
    }
    return plan;
}

#include "hybrid_scheduler.h"
uint16_t clearra_hybrid_triple_buffer_pipeline_reuse_count(
    ClearraHybridBatchPlan *plan) {
    if (plan == 0 || plan->batch_buffer_reuse == 0u) {
        return 0u;
    }
    if (plan->triple_buffer_count == 0u) {
        plan->triple_buffer_count = 3u;
    }
    return plan->triple_buffer_count;
}uint16_t clearra_hybrid_triple_buffer_pipeline_overlap_steps(
    const ClearraHybridBatchPlan *plan) {
    if (plan == 0 || plan->gpu_readback_cpu_buildup_overlap == 0u) {
        return 0u;
    }
    return plan->triple_buffer_count > 0u ? plan->triple_buffer_count : 3u;
}

#include "hybrid_scheduler.h"
uint8_t clearra_hybrid_worker_pool_cpu_workers(void) {
    return 1u;
}uint8_t clearra_hybrid_worker_pool_gpu_workers(void) {
    return 1u;
}

#include "hybrid_scheduler.h"

uint16_t clearra_hybrid_work_stealing_assign_small_irregular_buildup(
    const ClearraCanonicalPackingTable *table) {
    if (table == 0) {
        return 0u;
    }
    return table->candidates.count;
}

#include "hybrid_scheduler.h"

uint16_t clearra_hybrid_backend_autotune_large_batch_threshold(void) {
    return 5u;
}
