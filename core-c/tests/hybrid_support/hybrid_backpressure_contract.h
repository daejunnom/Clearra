#ifndef CLEARRA_HYBRID_BACKPRESSURE_CONTRACT_H
#define CLEARRA_HYBRID_BACKPRESSURE_CONTRACT_H

#include "hybrid_backend_metrics.h"
#include "hybrid_batch_plan.h"
typedef enum ClearraHybridThrottleReason {
    CLEARRA_HYBRID_THROTTLE_NONE = 0,
    CLEARRA_HYBRID_THROTTLE_GPU_QUEUE_DEPTH = 1,
    CLEARRA_HYBRID_THROTTLE_CPU_WORKER_QUEUE_DEPTH = 2,
    CLEARRA_HYBRID_THROTTLE_READBACK_PENDING = 3,
    CLEARRA_HYBRID_THROTTLE_BUILD_VARIANT_BUFFER_PRESSURE = 4,
    CLEARRA_HYBRID_THROTTLE_COVERAGE_ROW_BUFFER_PRESSURE = 5
} ClearraHybridThrottleReason;typedef struct ClearraHybridBackpressureReport {
    uint16_t gpu_queue_depth;
    uint16_t cpu_worker_queue_depth;
    uint16_t readback_pending_batches;
    uint16_t build_variant_buffer_pressure;
    uint16_t coverage_row_buffer_pressure;
    uint8_t throttled_backend;
    ClearraHybridThrottleReason throttle_reason;
    uint16_t candidate_queue_len;
    uint16_t candidate_queue_capacity;
    uint16_t cpu_worker_backlog;
    uint16_t gpu_readback_backlog;
    uint16_t gpu_batch_in_flight;
    uint8_t backpressure_active;
    uint16_t deferred_batch_count;
    uint16_t truncated_batch_count;
    uint8_t memory_pressure_level;
} ClearraHybridBackpressureReport;ClearraHybridBackpressureReport clearra_hybrid_backpressure_report_for(
    const ClearraHybridBatchPlan *plan,
    const ClearraHybridBackendMetrics *metrics);
#endif
