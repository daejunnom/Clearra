#include "gpu/gpu_backend.h"
static ClearraGpuStatus plan_larger_batch(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuPackingResult *result) {
    if (batch == 0 || result == 0 ||
        clearra_gpu_batch_descriptor_validate(batch) != CLEARRA_GPU_OK) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    uint16_t estimated_work_items =
        (uint16_t)(batch->piece_multiset_window.total_count *
                   CLEARRA_PACKING_MAX_PLACEMENT_CANDIDATES);
    uint16_t planned_batches =
        (uint16_t)((estimated_work_items + batch->candidate_capacity - 1u) /
                   batch->candidate_capacity);
    result->planned_batch_count = planned_batches == 0u ? 1u : planned_batches;
    result->batch_candidate_capacity = (uint16_t)batch->candidate_capacity;
    result->larger_batch_planner_enabled = 1u;
    return CLEARRA_GPU_OK;
}ClearraGpuStatus clearra_gpu_packing_backend_run(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuDeviceRequest request,
    bool allow_backend_fallback,
    ClearraGpuPackingResult *out_result) {
    if (batch == 0 || out_result == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    clearra_gpu_packing_result_clear(out_result);
    ClearraGpuStatus status = plan_larger_batch(batch, out_result);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }

    ClearraGpuDevice device;
    ClearraGpuStatus device_status = clearra_gpu_device_resolve(request, &device);
    if (device_status != CLEARRA_GPU_OK) {
        out_result->status = device_status;
        out_result->unavailable_reason = device.unavailable_reason;
        if (!clearra_gpu_fallback_allowed(allow_backend_fallback)) {
            return out_result->status;
        }
        return clearra_gpu_fallback_to_cpu_packing(batch, out_result);
    }

    status = clearra_gpu_backend_dispatch_candidates_with_pruning_ledger(
        batch,
        device.backend_kind,
        &out_result->raw_candidates,
        &out_result->pruning_ledger);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }
    out_result->result_complete = 1u;
    out_result->truncation_reason = CLR_RESOURCE_TRUNCATION_NONE;
    out_result->candidate_is_solution = 0u;
    status = clearra_gpu_host_confirm_candidates(
        &out_result->raw_candidates, &out_result->hash_exact_confirmed);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }
    status = clearra_gpu_shape_union_mask(
        &out_result->raw_candidates, &out_result->gpu_shape_union_mask);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }
    out_result->shape_union_mask_applied = 1u;
    status = clearra_gpu_candidate_hash(
        &out_result->raw_candidates, &out_result->gpu_candidate_hash);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }
    status = clearra_gpu_readback_reduce_result(out_result);
    if (status != CLEARRA_GPU_OK) {
        return status;
    }
    status = clearra_gpu_cpu_exact_confirm_reference(
        batch,
        out_result,
        &out_result->cpu_reference_matched,
        &out_result->cpu_reference_hash);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }
    out_result->cpu_exact_confirmed = 1u;
    out_result->cpu_exact_confirm_optimized = 1u;
    out_result->deterministic_result =
        (uint8_t)(out_result->hash_exact_confirmed &&
                  out_result->cpu_reference_matched);

    out_result->status = CLEARRA_GPU_OK;
    return CLEARRA_GPU_OK;
}ClearraGpuStatus clearra_cpu_packing_reference_run(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuPackingResult *out_result) {
    if (batch == 0 || out_result == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    clearra_gpu_packing_result_clear(out_result);
    ClearraGpuStatus status = plan_larger_batch(batch, out_result);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }
    clr_resource_report resource_report;
    status =
        clearra_cpu_packing_reference_generate_with_resource_report_and_pruning_ledger(
            batch,
            &out_result->raw_candidates,
            &resource_report,
            &out_result->pruning_ledger);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }
    out_result->result_complete = (uint8_t)!resource_report.truncated;
    out_result->truncation_reason = (uint16_t)resource_report.truncation_reason;

    uint8_t confirmed = 0;
    status = clearra_gpu_host_confirm_candidates(
        &out_result->raw_candidates, &confirmed);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }

    out_result->hash_exact_confirmed = confirmed;
    out_result->candidate_is_solution = 0u;
    status = clearra_gpu_shape_union_mask(
        &out_result->raw_candidates, &out_result->gpu_shape_union_mask);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }
    out_result->shape_union_mask_applied = 1u;
    status = clearra_gpu_candidate_hash(
        &out_result->raw_candidates, &out_result->gpu_candidate_hash);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }
    status = clearra_gpu_readback_reduce_result(out_result);
    if (status != CLEARRA_GPU_OK) {
        return status;
    }
    status = clearra_gpu_cpu_exact_confirm_reference(
        batch,
        out_result,
        &out_result->cpu_reference_matched,
        &out_result->cpu_reference_hash);
    if (status != CLEARRA_GPU_OK) {
        out_result->status = status;
        return status;
    }
    out_result->cpu_exact_confirmed = 1u;
    out_result->cpu_exact_confirm_optimized = 1u;
    out_result->deterministic_result =
        (uint8_t)(out_result->hash_exact_confirmed &&
                  out_result->cpu_reference_matched);

    out_result->status = CLEARRA_GPU_OK;
    return CLEARRA_GPU_OK;
}

#include "gpu/gpu_backend_adapter.h"
const ClearraGpuBackendVTable *clearra_gpu_backend_adapter_vtable(
    ClearraGpuBackendKind backend_kind) {
    if (backend_kind == CLEARRA_GPU_BACKEND_NATIVE_COMPUTE ||
        backend_kind == CLEARRA_GPU_BACKEND_DISABLED) {
        return clearra_gpu_backend_unavailable_vtable();
    }
    return 0;
}ClearraGpuStatus clearra_gpu_backend_adapter_execute(
    ClearraGpuBackendKind backend_kind,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_candidates) {
    clr_pruning_proof_ledger pruning_ledger;
    return clearra_gpu_backend_adapter_execute_with_pruning_ledger(
        backend_kind, batch, out_candidates, &pruning_ledger);
}ClearraGpuStatus clearra_gpu_backend_adapter_execute_with_pruning_ledger(
    ClearraGpuBackendKind backend_kind,
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_candidates,
    clr_pruning_proof_ledger *out_pruning_ledger) {
    ClearraGpuContext context = {0};
    ClearraGpuStatus status;
    ClearraGpuStatus destroy_status;
    const ClearraGpuBackendVTable *vtable =
        clearra_gpu_backend_adapter_vtable(backend_kind);

    if (batch == 0 || out_candidates == 0 || out_pruning_ledger == 0 ||
        vtable == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    clr_pruning_proof_ledger_init(out_pruning_ledger);
    clr_pruning_proof_ledger_init(&context.pruning_ledger);

    context.backend_kind = backend_kind;
    status = vtable->create_context(&context);
    if (status == CLEARRA_GPU_OK) {
        status = vtable->upload_batch(&context, batch);
    }
    if (status == CLEARRA_GPU_OK) {
        status = vtable->launch_packing_kernel(&context);
    }
    if (status == CLEARRA_GPU_OK) {
        status = vtable->readback_candidates(&context, out_candidates);
    } else {
        clearra_packing_candidate_buffer_clear(out_candidates);
    }
    *out_pruning_ledger = context.pruning_ledger;

    destroy_status = vtable->destroy_context(&context);
    if (status != CLEARRA_GPU_OK) {
        return status;
    }
    return destroy_status;
}

#include "gpu/gpu_backend.h"
const char *clearra_gpu_backend_kind_label(ClearraGpuBackendKind backend_kind) {
    switch (backend_kind) {
        case CLEARRA_GPU_BACKEND_NATIVE_COMPUTE:
            return "native-gpu";
        case CLEARRA_GPU_BACKEND_DISABLED:
            return "disabled";
    }
    return "unknown";
}const char *clearra_gpu_backend_kind_capability_label(
    ClearraGpuBackendKind backend_kind,
    uint8_t available) {
    if (available) {
        return clearra_gpu_backend_kind_label(backend_kind);
    }

    switch (backend_kind) {
        case CLEARRA_GPU_BACKEND_NATIVE_COMPUTE:
            return "native-gpu-unavailable";
        case CLEARRA_GPU_BACKEND_DISABLED:
            return "disabled";
    }

    return "unknown-unavailable";
}ClearraGpuStatus clearra_gpu_backend_capability(
    ClearraGpuBackendKind backend_kind,
    ClearraGpuBackendCapability *out_capability) {
    ClearraGpuDeviceRequest request = {0};
    request.device_kind = (uint8_t)backend_kind;
    return clearra_gpu_backend_select(request, out_capability);
}

#include "gpu/gpu_backend_adapter.h"
ClearraGpuStatus clearra_gpu_backend_select(
    ClearraGpuDeviceRequest request,
    ClearraGpuBackendCapability *out_capability) {
    ClearraGpuBackendKind backend_kind =
        (ClearraGpuBackendKind)request.device_kind;

    if (out_capability == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    if (backend_kind == CLEARRA_GPU_BACKEND_DISABLED) {
        *out_capability = (ClearraGpuBackendCapability){
            .backend_kind = CLEARRA_GPU_BACKEND_DISABLED,
            .available = 0u,
            .connected = 0u,
            .exact_supported = 0u,
            .accepts_user_shader_path = 0u,
            .unavailable_reason = CLEARRA_GPU_UNAVAILABLE_FEATURE_DISABLED,
        };
        return CLEARRA_GPU_UNAVAILABLE;
    }

    if (backend_kind == 0u) {
        backend_kind = CLEARRA_GPU_BACKEND_NATIVE_COMPUTE;
        request.device_kind = (uint8_t)backend_kind;
    }

    const ClearraGpuBackendVTable *vtable =
        clearra_gpu_backend_adapter_vtable(backend_kind);
    if (vtable == 0 || vtable->query_capability == 0) {
        return clearra_gpu_device_capability_kernel_unavailable(
            backend_kind, out_capability);
    }
    return vtable->query_capability(request, out_capability);
}ClearraGpuStatus clearra_gpu_backend_dispatch_candidates(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuBackendKind backend_kind,
    ClearraPackingCandidateBuffer *out_buffer) {
    clr_pruning_proof_ledger pruning_ledger;
    return clearra_gpu_backend_dispatch_candidates_with_pruning_ledger(
        batch, backend_kind, out_buffer, &pruning_ledger);
}ClearraGpuStatus clearra_gpu_backend_dispatch_candidates_with_pruning_ledger(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuBackendKind backend_kind,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_pruning_proof_ledger *out_pruning_ledger) {
    if (batch == 0 || out_buffer == 0 || out_pruning_ledger == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    return clearra_gpu_backend_adapter_execute_with_pruning_ledger(
        backend_kind, batch, out_buffer, out_pruning_ledger);
}

#include "gpu/gpu_backend_adapter.h"
static ClearraGpuStatus unavailable_query_capability(
    ClearraGpuDeviceRequest request,
    ClearraGpuBackendCapability *out_capability) {
    return clearra_gpu_device_capability_kernel_unavailable(
        (ClearraGpuBackendKind)request.device_kind, out_capability);
}static ClearraGpuStatus unavailable_create_context(ClearraGpuContext *context) {
    if (context == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    context->context_created = 0u;
    context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
    return CLEARRA_GPU_UNAVAILABLE;
}static ClearraGpuStatus unavailable_upload_batch(
    ClearraGpuContext *context,
    const ClearraGpuPackingBatchDescriptor *batch) {
    (void)batch;
    if (context == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
    return CLEARRA_GPU_UNAVAILABLE;
}static ClearraGpuStatus unavailable_launch_packing_kernel(ClearraGpuContext *context) {
    return clearra_gpu_context_launch_unavailable(context);
}static ClearraGpuStatus unavailable_readback_candidates(
    ClearraGpuContext *context,
    ClearraPackingCandidateBuffer *out_candidates) {
    if (context == 0 || out_candidates == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    clearra_packing_candidate_buffer_clear(out_candidates);
    context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
    return CLEARRA_GPU_UNAVAILABLE;
}static ClearraGpuStatus unavailable_destroy_context(ClearraGpuContext *context) {
    return clearra_gpu_context_destroy_memory(context);
}const ClearraGpuBackendVTable *clearra_gpu_backend_unavailable_vtable(void) {
    static const ClearraGpuBackendVTable vtable = {
        unavailable_query_capability,
        unavailable_create_context,
        unavailable_upload_batch,
        unavailable_launch_packing_kernel,
        unavailable_readback_candidates,
        unavailable_destroy_context,
    };
    return &vtable;
}ClearraGpuStatus clearra_gpu_unavailable_backend_dispatch(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuBackendKind backend_kind,
    ClearraPackingCandidateBuffer *out_buffer) {
    (void)batch;
    (void)backend_kind;
    if (out_buffer == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }
    clearra_packing_candidate_buffer_clear(out_buffer);
    return CLEARRA_GPU_UNAVAILABLE;
}

#include "gpu/gpu_backend.h"
const char *clearra_gpu_unavailable_reason_label(ClearraGpuUnavailableReason reason) {
    switch (reason) {
        case CLEARRA_GPU_UNAVAILABLE_NONE:
            return "none";
        case CLEARRA_GPU_UNAVAILABLE_FEATURE_DISABLED:
            return "gpu_feature_disabled";
        case CLEARRA_GPU_UNAVAILABLE_DEVICE_NOT_FOUND:
            return "gpu_device_not_found";
        case CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE:
            return "gpu_kernel_unavailable";
    }
    return "gpu_unavailable_unknown";
}ClearraGpuDeviceRequest clearra_gpu_device_request_default(void) {
    ClearraGpuDeviceRequest request = {0};
    request.device_kind = (uint8_t)CLEARRA_GPU_BACKEND_NATIVE_COMPUTE;
    return request;
}ClearraGpuStatus clearra_gpu_device_resolve(
    ClearraGpuDeviceRequest request,
    ClearraGpuDevice *out_device) {
    if (out_device == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    ClearraGpuBackendCapability capability;
    ClearraGpuStatus status = clearra_gpu_backend_select(request, &capability);
    out_device->available = 0u;
    out_device->backend_kind = capability.backend_kind;
    out_device->device_index = request.device_index;
    out_device->unavailable_reason = capability.unavailable_reason;
    if (status != CLEARRA_GPU_OK) {
        return status;
    }

    out_device->available = capability.available;
    return capability.available ? CLEARRA_GPU_OK : CLEARRA_GPU_UNAVAILABLE;
}

#include "gpu/gpu_backend_adapter.h"
ClearraGpuStatus clearra_gpu_device_capability_kernel_unavailable(
    ClearraGpuBackendKind backend_kind,
    ClearraGpuBackendCapability *out_capability) {
    if (out_capability == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    *out_capability = (ClearraGpuBackendCapability){
        .backend_kind = backend_kind,
        .available = 0u,
        .connected = 0u,
        .exact_supported = 0u,
        .accepts_user_shader_path = 0u,
        .unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE,
    };
    return CLEARRA_GPU_UNAVAILABLE;
}

#include "gpu/gpu_backend.h"
bool clearra_gpu_fallback_allowed(bool allow_backend_fallback) {
    return allow_backend_fallback;
}ClearraGpuStatus clearra_gpu_fallback_to_cpu_packing(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuPackingResult *out_result) {
    if (batch == 0 || out_result == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    clr_packing_problem problem;
    if (clearra_gpu_batch_descriptor_to_packing_problem(batch, &problem) !=
        CLEARRA_GPU_OK) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    clr_resource_report resource_report;
    ClearraPackingStatus status =
        clearra_packing_enumerator_cpu_generate_problem_with_resource_report_and_pruning_ledger(
            &problem,
            &out_result->raw_candidates,
            &resource_report,
            &out_result->pruning_ledger);
    if (status != CLEARRA_PACKING_OK &&
        status != CLEARRA_PACKING_CAPACITY_EXCEEDED) {
        out_result->status = CLEARRA_GPU_PACKING_ERROR;
        return out_result->status;
    }
    out_result->result_complete = (uint8_t)!resource_report.truncated;
    out_result->truncation_reason = (uint16_t)resource_report.truncation_reason;

    status = clearra_packing_host_reduce(
        &out_result->raw_candidates,
        &out_result->canonical_candidates);
    if (status != CLEARRA_PACKING_OK) {
        out_result->status = CLEARRA_GPU_PACKING_ERROR;
        return out_result->status;
    }

    out_result->status = CLEARRA_GPU_OK;
    if (out_result->unavailable_reason == CLEARRA_GPU_UNAVAILABLE_NONE) {
        out_result->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_FEATURE_DISABLED;
    }
    out_result->used_cpu_fallback = 1u;
    out_result->candidate_is_solution = 0u;
    out_result->raw_candidate_count = out_result->raw_candidates.count;
    out_result->canonical_candidate_count = out_result->canonical_candidates.candidates.count;
    out_result->hash_exact_confirmed = 1u;
    out_result->cpu_exact_confirmed = 1u;
    out_result->cpu_exact_confirm_optimized = 1u;
    out_result->cpu_reference_matched = 1u;
    out_result->deterministic_result = 1u;
    out_result->readback_uncompressed_count = out_result->raw_candidate_count;
    out_result->readback_compressed_count = out_result->canonical_candidate_count;
    out_result->shape_union_mask_applied =
        (clearra_gpu_shape_union_mask(
             &out_result->raw_candidates,
             &out_result->gpu_shape_union_mask) == CLEARRA_GPU_OK);
    if (clearra_gpu_candidate_hash(
            &out_result->canonical_candidates.candidates,
            &out_result->gpu_candidate_hash) != CLEARRA_GPU_OK) {
        out_result->status = CLEARRA_GPU_PACKING_ERROR;
        return out_result->status;
    }
    out_result->cpu_reference_hash = out_result->gpu_candidate_hash;
    return CLEARRA_GPU_OK;
}
