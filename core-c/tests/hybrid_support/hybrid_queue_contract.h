#ifndef CLEARRA_HYBRID_QUEUE_CONTRACT_H
#define CLEARRA_HYBRID_QUEUE_CONTRACT_H

#include "hybrid_autotune_contract.h"
#include "hybrid_backend_metrics.h"
typedef struct ClearraHybridGpuQueueStats {
    uint32_t batches_submitted;
    uint32_t batches_completed;
    uint32_t pending_batches;
    uint32_t max_queue_depth;
    uint32_t total_batch_latency_ms;
} ClearraHybridGpuQueueStats;typedef struct ClearraHybridReadbackQueueStats {
    uint32_t batches_enqueued;
    uint32_t batches_completed;
    uint32_t pending_batches;
    uint32_t max_pending_batches;
    uint32_t candidate_buffer_pressure;
} ClearraHybridReadbackQueueStats;typedef struct ClearraHybridCpuConfirmQueueStats {
    uint32_t candidates_enqueued;
    uint32_t candidates_confirmed;
    uint32_t build_variants_enqueued;
    uint32_t max_confirm_queue_depth;
    uint32_t max_buildup_queue_depth;
    uint32_t total_confirm_latency_ms;
} ClearraHybridCpuConfirmQueueStats;void clearra_hybrid_gpu_queue_init(ClearraHybridGpuQueueStats *queue);
void clearra_hybrid_gpu_queue_submit(
    ClearraHybridGpuQueueStats *queue,
    uint32_t batch_count);
void clearra_hybrid_gpu_queue_complete(
    ClearraHybridGpuQueueStats *queue,
    uint32_t batch_count,
    uint32_t latency_ms);
void clearra_hybrid_gpu_queue_apply_metrics(
    const ClearraHybridGpuQueueStats *queue,
    ClearraHybridBackendMetrics *backend_metrics,
    ClearraHybridAutotuneMetrics *autotune_metrics);
void clearra_hybrid_readback_queue_init(ClearraHybridReadbackQueueStats *queue);
void clearra_hybrid_readback_queue_enqueue(
    ClearraHybridReadbackQueueStats *queue,
    uint32_t batch_count,
    uint32_t candidate_pressure);
void clearra_hybrid_readback_queue_complete(
    ClearraHybridReadbackQueueStats *queue,
    uint32_t batch_count);
void clearra_hybrid_readback_queue_apply_metrics(
    const ClearraHybridReadbackQueueStats *queue,
    ClearraHybridBackendMetrics *backend_metrics,
    ClearraHybridAutotuneMetrics *autotune_metrics);
void clearra_hybrid_cpu_confirm_queue_init(
    ClearraHybridCpuConfirmQueueStats *queue);
void clearra_hybrid_cpu_confirm_queue_enqueue(
    ClearraHybridCpuConfirmQueueStats *queue,
    uint32_t candidate_count);
void clearra_hybrid_cpu_confirm_queue_complete(
    ClearraHybridCpuConfirmQueueStats *queue,
    uint32_t candidate_count,
    uint32_t build_variant_count,
    uint32_t latency_ms);
void clearra_hybrid_cpu_confirm_queue_apply_metrics(
    const ClearraHybridCpuConfirmQueueStats *queue,
    ClearraHybridBackendMetrics *backend_metrics,
    ClearraHybridAutotuneMetrics *autotune_metrics);
#endif
