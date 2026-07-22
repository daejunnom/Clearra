#ifndef CLEARRA_HYBRID_SCHEDULER_RESULT_H
#define CLEARRA_HYBRID_SCHEDULER_RESULT_H

#include "hybrid_backpressure_contract.h"
#include "hybrid_batch_plan.h"
#include "hybrid_gpu_worker_contract.h"
#include "hybrid_status.h"
#include "clr_memory.h"

#include <stdbool.h>

typedef struct ClearraHybridSchedulerResult {
    ClearraHybridStatus status;
    ClearraHybridBatchPlan plan;
    ClearraHybridBackendMetrics metrics;
    ClearraHybridBackpressureReport backpressure;
    ClrMemLeakReport leak_report;
} ClearraHybridSchedulerResult;
ClearraHybridStatus clearra_hybrid_manage_memory_epoch(
    ClearraHybridSchedulerResult *result);
void clearra_hybrid_scheduler_result_clear(ClearraHybridSchedulerResult *result);
ClearraHybridStatus clearra_hybrid_finish_result(
    const clr_packing_problem *packing,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuConfirmedCandidateQueue *confirmed_queue,
    const ClearraGpuWorkerResult *worker_result,
    uint32_t gpu_worker_latency_ms,
    ClearraGpuUnavailableReason fallback_reason,
    uint8_t fallback_used,
    ClearraHybridSchedulerResult *out_result);
ClearraHybridStatus clearra_hybrid_scheduler_run_cpu_fallback(
    const clr_packing_problem *packing,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraHybridSchedulerResult *out_result);
ClearraHybridStatus clearra_hybrid_scheduler_run_cpu_fallback_candidates(
    const clr_packing_problem *packing,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraHybridSchedulerResult *out_result,
    ClearraPackingCandidateBuffer *out_confirmed_candidates);
ClearraHybridStatus clearra_hybrid_scheduler_run(
    const clr_packing_problem *packing,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuDeviceRequest request,
    bool allow_backend_fallback,
    ClearraHybridSchedulerResult *out_result);
#endif
