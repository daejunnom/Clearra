#include "gpu/gpu_backend.h"
#include "gpu/gpu_backend_adapter.h"

ClearraGpuStatus clearra_gpu_context_launch_unavailable(
    ClearraGpuContext *context) {
    if (context == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    context->kernel_launched = 0u;
    context->unavailable_reason = CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE;
    return CLEARRA_GPU_UNAVAILABLE;
}

ClearraGpuStatus clearra_gpu_host_confirm_candidates(
    const ClearraPackingCandidateBuffer *buffer,
    uint8_t *out_confirmed) {
    if (buffer == 0 || out_confirmed == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    *out_confirmed = 1u;
    for (uint16_t index = 0; index < buffer->count; index++) {
        ClearraPackingCandidateView candidate;
        ClearraPackingStatus status =
            clearra_packing_candidate_buffer_candidate_at(buffer, index, &candidate);
        if (status != CLEARRA_PACKING_OK ||
            !clearra_packing_hash_confirm_exact(buffer, index, &candidate)) {
            *out_confirmed = 0u;
            return CLEARRA_GPU_PACKING_ERROR;
        }
    }

    return CLEARRA_GPU_OK;
}

ClearraGpuStatus clearra_gpu_shape_union_mask(
    const ClearraPackingCandidateBuffer *buffer,
    ClearraShapeUnionMask *out_shape_union_mask) {
    if (buffer == 0 || out_shape_union_mask == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    uint64_t shape_union_mask = 0;
    for (uint16_t index = 0; index < buffer->count; index++) {
        shape_union_mask |= buffer->shape_masks[index];
    }
    out_shape_union_mask->value = shape_union_mask;
    return CLEARRA_GPU_OK;
}

ClearraGpuStatus clearra_gpu_candidate_hash(
    const ClearraPackingCandidateBuffer *buffer,
    uint64_t *out_hash) {
    if (buffer == 0 || out_hash == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    uint64_t hash = UINT64_C(1469598103934665603);
    for (uint16_t index = 0; index < buffer->count; index++) {
        ClearraPackingCandidateView candidate;
        ClearraPackingStatus status =
            clearra_packing_candidate_buffer_candidate_at(buffer, index, &candidate);
        if (status != CLEARRA_PACKING_OK) {
            return CLEARRA_GPU_PACKING_ERROR;
        }
        hash = clearra_cache_key_mix_u64(
            hash, clearra_packing_candidate_identity_key(&candidate));
    }
    *out_hash = hash;
    return CLEARRA_GPU_OK;
}
