#include "../../include/clr_gpu.h"

ClearraGpuStatus clearra_gpu_device_capability_query(
    ClearraGpuDeviceRequest request,
    ClearraGpuBackendCapability *out_capability) {
    if (out_capability == 0) {
        return CLEARRA_GPU_INVALID_ARGUMENT;
    }

    ClearraGpuBackendKind backend_kind =
        (ClearraGpuBackendKind)request.device_kind;
    if (backend_kind == 0u) {
        backend_kind = CLEARRA_GPU_BACKEND_NATIVE_COMPUTE;
    }

    *out_capability = (ClearraGpuBackendCapability){
        .backend_kind = backend_kind,
        .available = 0u,
        .connected = 0u,
        .exact_supported = 0u,
        .accepts_user_shader_path = 0u,
        .unavailable_reason = backend_kind == CLEARRA_GPU_BACKEND_DISABLED
            ? CLEARRA_GPU_UNAVAILABLE_FEATURE_DISABLED
            : CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE,
    };
    return CLEARRA_GPU_UNAVAILABLE;
}
