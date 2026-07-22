#include "gpu_test_support.h"

#include <string.h>

void gpu_unavailable_reports_reason(void) {
    ClearraGpuDevice device;
    ClearraGpuDeviceRequest request = clearra_gpu_device_request_default();

    EXPECT_GPU_STATUS(clearra_gpu_device_resolve(request, &device),
                      CLEARRA_GPU_UNAVAILABLE);
    EXPECT_FALSE(device.available);
    EXPECT_U64(device.backend_kind, CLEARRA_GPU_BACKEND_NATIVE_COMPUTE);
    EXPECT_U64(device.unavailable_reason,
               CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE);
}

void gpu_backend_selection_defaults_to_unavailable_native(void) {
    ClearraGpuBackendCapability capability;
    ClearraGpuDeviceRequest request = clearra_gpu_device_request_default();

    EXPECT_GPU_STATUS(clearra_gpu_backend_select(request, &capability),
                      CLEARRA_GPU_UNAVAILABLE);
    EXPECT_U64(capability.backend_kind, CLEARRA_GPU_BACKEND_NATIVE_COMPUTE);
    EXPECT_FALSE(capability.available);
    EXPECT_FALSE(capability.accepts_user_shader_path);
}

void gpu_backend_native_unavailable_reports_reason(void) {
    ClearraGpuBackendCapability capability;

    EXPECT_GPU_STATUS(clearra_gpu_backend_capability(
                          CLEARRA_GPU_BACKEND_NATIVE_COMPUTE, &capability),
                      CLEARRA_GPU_UNAVAILABLE);
    EXPECT_FALSE(capability.available);
    EXPECT_U64(capability.unavailable_reason,
               CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE);
    EXPECT_TRUE(strcmp(clearra_gpu_backend_kind_capability_label(
                           capability.backend_kind, capability.available),
                       "native-gpu-unavailable") == 0);
}

void gpu_backend_registry_excludes_unimplemented_apis(void) {
    EXPECT_TRUE(strcmp(clearra_gpu_backend_kind_label(
                           CLEARRA_GPU_BACKEND_NATIVE_COMPUTE),
                       "native-gpu") == 0);
    EXPECT_TRUE(strcmp(clearra_gpu_backend_kind_capability_label(
                           CLEARRA_GPU_BACKEND_DISABLED, 0u),
                       "disabled") == 0);
    EXPECT_TRUE(strcmp(clearra_gpu_backend_kind_label((ClearraGpuBackendKind)42),
                       "unknown") == 0);
}

void gpu_backend_rejects_user_provided_shader_path(void) {
    EXPECT_GPU_STATUS(clearra_gpu_backend_reject_user_provided_shader_path(
                          "user-kernel.spv"),
                      CLEARRA_GPU_INVALID_ARGUMENT);
    EXPECT_GPU_STATUS(clearra_gpu_backend_reject_user_provided_shader_path(""),
                      CLEARRA_GPU_OK);
}

void gpu_backend_adapter_unavailable_does_not_execute(void) {
    ClearraGpuDeviceRequest request = clearra_gpu_device_request_default();
    ClearraGpuBackendCapability capability;
    const ClearraGpuBackendVTable *vtable =
        clearra_gpu_backend_adapter_vtable(CLEARRA_GPU_BACKEND_NATIVE_COMPUTE);

    EXPECT_TRUE(vtable != 0);
    EXPECT_GPU_STATUS(vtable->query_capability(request, &capability),
                      CLEARRA_GPU_UNAVAILABLE);
    EXPECT_FALSE(capability.available);
}

void gpu_backend_adapter_reports_kernel_unavailable(void) {
    ClearraGpuContext context = {
        .backend_kind = CLEARRA_GPU_BACKEND_NATIVE_COMPUTE,
    };
    static ClearraPackingCandidateBuffer output;

    EXPECT_GPU_STATUS(clearra_gpu_unavailable_backend_dispatch(
                          0, CLEARRA_GPU_BACKEND_NATIVE_COMPUTE, &output),
                      CLEARRA_GPU_UNAVAILABLE);
    EXPECT_GPU_STATUS(clearra_gpu_context_launch_unavailable(&context),
                      CLEARRA_GPU_UNAVAILABLE);
    EXPECT_U64(context.unavailable_reason,
               CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE);
}

void gpu_backend_adapter_rejects_user_shader_path(void) {
    EXPECT_GPU_STATUS(clearra_gpu_backend_adapter_reject_user_shader_path(
                          "unchecked-user-shader.spv"),
                      CLEARRA_GPU_INVALID_ARGUMENT);
    EXPECT_GPU_STATUS(clearra_gpu_backend_adapter_reject_user_shader_path(0),
                      CLEARRA_GPU_OK);
}

void gpu_context_destroy_releases_memory_context(void) {
    ClearraGpuContext context = {0};

    EXPECT_GPU_STATUS(clearra_gpu_context_create_memory(
                          &context, CLEARRA_GPU_BACKEND_NATIVE_COMPUTE),
                      CLEARRA_GPU_OK);
    EXPECT_TRUE(context.memory_context != 0);
    EXPECT_GPU_STATUS(clearra_gpu_context_destroy_memory(&context),
                      CLEARRA_GPU_OK);
    EXPECT_TRUE(context.memory_context_released);
    EXPECT_TRUE(context.memory_context == 0);
    EXPECT_TRUE(context.transfer_scope == 0);
}
