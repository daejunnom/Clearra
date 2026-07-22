#include "../../include/clr_gpu_worker.h"

static void clearra_gpu_worker_result_clear(ClearraGpuWorkerResult *result) {
    if (result != 0) {
        *result = (ClearraGpuWorkerResult){0};
    }
}

ClearraGpuWorkerState clearra_gpu_worker_state(void) {
    return CLEARRA_GPU_WORKER_DISABLED;
}

uint8_t clearra_gpu_worker_trust_can_source_exact_probability(
    ClearraGpuWorkerTrustState trust_state) {
    return (uint8_t)(
        trust_state == CLEARRA_GPU_WORKER_TRUST_GPU_COMPUTED_CPU_CONFIRMED ||
        trust_state == CLEARRA_GPU_WORKER_TRUST_DETERMINISTIC_REFERENCE_MATCHED);
}

ClearraGpuWorkerStatus clearra_gpu_worker_run(
    const ClearraGpuWorkerRequest *request,
    ClearraGpuWorkerResult *out_result) {
    if (request == 0 || out_result == 0) {
        return CLEARRA_GPU_WORKER_INVALID_ARGUMENT;
    }

    clearra_gpu_worker_result_clear(out_result);
    out_result->request_id = request->request_id;
    out_result->memory_ticket_id = request->memory_ticket_id;
    out_result->fence_epoch = request->fence_epoch;
    out_result->scope_epoch = request->scope_epoch;
    out_result->byte_budget = request->byte_budget;
    out_result->cpu_confirm_required = 1u;
    out_result->trust_state = CLEARRA_GPU_WORKER_TRUST_UNAVAILABLE;
    out_result->status = CLEARRA_GPU_WORKER_UNAVAILABLE;
    out_result->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
    out_result->can_source_exact_probability = 0u;
    out_result->candidate_count = 0u;
    return out_result->status;
}
