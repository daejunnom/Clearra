#ifndef CLR_GPU_WORKER_H
#define CLR_GPU_WORKER_H

#include "clr_gpu.h"
#include "clr_memory.h"

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef enum ClearraGpuWorkerStatus {
    CLEARRA_GPU_WORKER_OK = 0,
    CLEARRA_GPU_WORKER_INVALID_ARGUMENT = 1,
    CLEARRA_GPU_WORKER_UNAVAILABLE = 2,
    CLEARRA_GPU_WORKER_MEMORY_ERROR = 3
} ClearraGpuWorkerStatus;

typedef enum ClearraGpuWorkerState {
    CLEARRA_GPU_WORKER_DISABLED = 0,
    CLEARRA_GPU_WORKER_AVAILABLE = 1,
    CLEARRA_GPU_WORKER_BUSY = 2,
    CLEARRA_GPU_WORKER_DRAINING = 3,
    CLEARRA_GPU_WORKER_FAILED = 4
} ClearraGpuWorkerState;

typedef enum ClearraGpuWorkerTrustState {
    CLEARRA_GPU_WORKER_TRUST_NOT_USED = 0,
    CLEARRA_GPU_WORKER_TRUST_UNAVAILABLE = 1,
    CLEARRA_GPU_WORKER_TRUST_FALLBACK_USED = 2,
    CLEARRA_GPU_WORKER_TRUST_RESERVED_REMOVED = 3,
    CLEARRA_GPU_WORKER_TRUST_GPU_COMPUTED_UNCONFIRMED = 4,
    CLEARRA_GPU_WORKER_TRUST_GPU_COMPUTED_CPU_CONFIRMED = 5,
    CLEARRA_GPU_WORKER_TRUST_GPU_COMPUTED_MISMATCH = 6,
    CLEARRA_GPU_WORKER_TRUST_DETERMINISTIC_REFERENCE_MATCHED = 7
} ClearraGpuWorkerTrustState;

typedef struct ClearraGpuWorkerRequest {
    uint64_t request_id;
    ClearraGpuPackingBatchDescriptor batch;
    uint64_t memory_ticket_id;
    uint64_t fence_epoch;
    uint64_t scope_epoch;
    uint64_t byte_budget;
    uint8_t cpu_confirm_required;
} ClearraGpuWorkerRequest;

typedef struct ClearraGpuWorkerBackpressure {
    uint16_t gpu_queue_depth;
    uint16_t cpu_worker_queue_depth;
    uint16_t readback_pending_batches;
    uint16_t build_variant_buffer_pressure;
    uint16_t coverage_row_buffer_pressure;
    uint8_t throttled_backend;
    uint8_t throttle_reason;
} ClearraGpuWorkerBackpressure;

typedef struct ClearraGpuWorkerResult {
    uint64_t request_id;
    ClearraGpuWorkerStatus status;
    ClearraGpuWorkerTrustState trust_state;
    ClearraGpuUnavailableReason unavailable_reason;
    uint64_t memory_ticket_id;
    uint64_t fence_epoch;
    uint64_t scope_epoch;
    uint64_t byte_budget;
    uint8_t cpu_confirm_required;
    uint8_t can_source_exact_probability;
    uint16_t candidate_count;
    ClearraGpuWorkerBackpressure backpressure;
} ClearraGpuWorkerResult;

ClearraGpuWorkerState clearra_gpu_worker_state(void);
uint8_t clearra_gpu_worker_trust_can_source_exact_probability(
    ClearraGpuWorkerTrustState trust_state);
ClearraGpuWorkerStatus clearra_gpu_worker_run(
    const ClearraGpuWorkerRequest *request,
    ClearraGpuWorkerResult *out_result);
ClearraGpuWorkerStatus clearra_gpu_worker_scheduler_bridge_run(
    ClrMemContext *context,
    ClrScope *gpu_transfer_scope,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuWorkerResult *out_result);

#ifdef __cplusplus
}
#endif

#endif
