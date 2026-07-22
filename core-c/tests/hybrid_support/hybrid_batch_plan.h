#ifndef CLEARRA_HYBRID_BATCH_PLAN_H
#define CLEARRA_HYBRID_BATCH_PLAN_H

#include "../../src/gpu/gpu_backend.h"

#include <stdint.h>

typedef struct ClearraHybridBatchPlan {
    uint8_t gpu_large_packing_batch;
    uint8_t cpu_small_irregular_buildup;
    uint8_t gpu_readback_cpu_buildup_overlap;
    uint8_t batch_buffer_reuse;
    uint8_t memory_epoch_managed;
    uint8_t triple_buffer_count;
    uint8_t cpu_worker_count;
    uint8_t gpu_worker_count;
    uint16_t large_batch_threshold;
} ClearraHybridBatchPlan;
ClearraHybridBatchPlan clearra_hybrid_batch_plan_for(
    const ClearraGpuPackingBatchDescriptor *batch);
uint16_t clearra_hybrid_backend_autotune_large_batch_threshold(void);
uint8_t clearra_hybrid_worker_pool_cpu_workers(void);
uint8_t clearra_hybrid_worker_pool_gpu_workers(void);
uint16_t clearra_hybrid_work_stealing_assign_small_irregular_buildup(
    const ClearraCanonicalPackingTable *table);
uint16_t clearra_hybrid_triple_buffer_pipeline_reuse_count(
    ClearraHybridBatchPlan *plan);
uint16_t clearra_hybrid_triple_buffer_pipeline_overlap_steps(
    const ClearraHybridBatchPlan *plan);
#endif
