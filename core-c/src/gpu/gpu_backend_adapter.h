#ifndef CLEARRA_GPU_BACKEND_ADAPTER_H
#define CLEARRA_GPU_BACKEND_ADAPTER_H

#include "gpu_backend.h"
#include "../../include/clr_memory.h"
typedef struct ClearraGpuContext {
    ClearraGpuBackendKind backend_kind;
    ClearraGpuBackendCapability capability;
    ClearraGpuUnavailableReason unavailable_reason;
    ClearraGpuPackingBatchDescriptor uploaded_batch;
    ClearraPackingCandidateBuffer *candidate_buffer;
    clr_pruning_proof_ledger pruning_ledger;
    ClrMemContext *memory_context;
    ClrScope *transfer_scope;
    uint64_t upload_buffer_id;
    uint64_t readback_buffer_id;
    uint64_t fence_epoch;
    uint8_t context_created;
    uint8_t batch_uploaded;
    uint8_t kernel_launched;
    uint8_t candidates_read_back;
    uint8_t memory_context_released;
} ClearraGpuContext;typedef struct ClearraGpuBackendVTable {
    ClearraGpuStatus (*query_capability)(
        ClearraGpuDeviceRequest request,
        ClearraGpuBackendCapability *out_capability);
    ClearraGpuStatus (*create_context)(ClearraGpuContext *context);
    ClearraGpuStatus (*upload_batch)(
        ClearraGpuContext *context,
        const ClearraGpuPackingBatchDescriptor *batch);
    ClearraGpuStatus (*launch_packing_kernel)(ClearraGpuContext *context);
    ClearraGpuStatus (*readback_candidates)(
        ClearraGpuContext *context,
        ClearraPackingCandidateBuffer *out_candidates);
    ClearraGpuStatus (*destroy_context)(ClearraGpuContext *context);
} ClearraGpuBackendVTable;ClearraGpuStatus clearra_gpu_device_capability_query(
    ClearraGpuDeviceRequest request,
    ClearraGpuBackendCapability *out_capability);
ClearraGpuStatus clearra_gpu_device_capability_kernel_unavailable(
    ClearraGpuBackendKind backend_kind,
    ClearraGpuBackendCapability *out_capability);ClearraGpuStatus clearra_gpu_context_create_memory(
    ClearraGpuContext *context,
    ClearraGpuBackendKind backend_kind);
ClearraGpuStatus clearra_gpu_context_destroy_memory(ClearraGpuContext *context);
ClearraGpuStatus clearra_gpu_context_upload_batch(
    ClearraGpuContext *context,
    const ClearraGpuPackingBatchDescriptor *batch);
ClearraGpuStatus clearra_gpu_context_readback_candidates(
    ClearraGpuContext *context,
    ClearraPackingCandidateBuffer *out_candidates);
ClearraGpuStatus clearra_gpu_context_launch_unavailable(
    ClearraGpuContext *context);
const ClearraGpuBackendVTable *clearra_gpu_backend_unavailable_vtable(void);
const ClearraGpuBackendVTable *clearra_gpu_backend_adapter_vtable(
    ClearraGpuBackendKind backend_kind);
ClearraGpuStatus clearra_gpu_backend_adapter_execute(
    ClearraGpuBackendKind backend_kind,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_candidates);
ClearraGpuStatus clearra_gpu_backend_adapter_execute_with_pruning_ledger(
    ClearraGpuBackendKind backend_kind,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_candidates,
    clr_pruning_proof_ledger *out_pruning_ledger);
ClearraGpuStatus clearra_gpu_backend_adapter_reject_user_shader_path(
    const char *shader_path);
#endif
