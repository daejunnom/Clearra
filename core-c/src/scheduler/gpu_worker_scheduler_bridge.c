#include "../../include/clr_gpu_worker.h"

ClearraGpuWorkerStatus clearra_gpu_worker_scheduler_bridge_run(
    ClrMemContext *context,
    ClrScope *gpu_transfer_scope,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuWorkerResult *out_result) {
    ClearraGpuWorkerRequest request;
    ClearraGpuWorkerStatus worker_status;
    uint64_t buffer_id = 0;
    uint64_t fence_epoch = 0;
    if (context == 0 || gpu_transfer_scope == 0 || batch == 0 ||
        out_result == 0) {
        return CLEARRA_GPU_WORKER_INVALID_ARGUMENT;
    }

    if (clr_epoch_advance(context, &fence_epoch) != CLR_MEM_OK ||
        clr_gpu_buffer_register_for_scope(
            context, gpu_transfer_scope, sizeof(ClearraGpuPackingBatchDescriptor),
            &buffer_id) != CLR_MEM_OK ||
        clr_gpu_buffer_set_fence_epoch(context, buffer_id, fence_epoch) !=
            CLR_MEM_OK) {
        return CLEARRA_GPU_WORKER_MEMORY_ERROR;
    }

    request = (ClearraGpuWorkerRequest){
        .request_id = buffer_id,
        .batch = *batch,
        .memory_ticket_id = buffer_id,
        .fence_epoch = fence_epoch,
        .scope_epoch = fence_epoch,
        .byte_budget = sizeof(ClearraGpuPackingBatchDescriptor),
        .cpu_confirm_required = 1u,
    };

    worker_status = clearra_gpu_worker_run(&request, out_result);

    if (clr_gpu_buffer_release(context, buffer_id) != CLR_MEM_OK ||
        clr_release_queue_drain(context, fence_epoch) != CLR_MEM_OK) {
        return CLEARRA_GPU_WORKER_MEMORY_ERROR;
    }

    return worker_status;
}
