#ifndef CLEARRA_HYBRID_GPU_WORKER_CONTRACT_H
#define CLEARRA_HYBRID_GPU_WORKER_CONTRACT_H

#include "hybrid_backend_metrics.h"
#include "hybrid_status.h"
void clearra_hybrid_copy_gpu_worker_metrics(
    const ClearraGpuWorkerResult *worker_result,
    ClearraHybridBackendMetrics *metrics);
ClearraHybridStatus clearra_hybrid_submit_gpu_worker_request(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuWorkerResult *out_worker_result,
    uint32_t *out_latency_ms);
#endif
