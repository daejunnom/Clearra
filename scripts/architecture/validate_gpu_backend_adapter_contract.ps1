# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-GpuBackendAdapterContractValidation() {
    foreach ($requiredFile in @(
        'core-c/src/gpu/gpu_backend_adapter.h',
        'core-c/src/gpu/gpu_backend.c',
        'core-c/src/gpu/gpu_worker_unavailable.c'
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "GPU adapter file missing: $requiredFile"
        }
    }
    $header = Read-Text 'core-c/src/gpu/gpu_backend_adapter.h'
    foreach ($required in @(
        'ClearraGpuBackendVTable', 'query_capability', 'create_context',
        'upload_batch', 'launch_packing_kernel', 'readback_candidates',
        'destroy_context'
    )) {
        if ($header -notlike "*$required*") {
            Add-ArchitectureError "GPU adapter ABI is missing '$required'"
        }
    }
    $source = Read-Text 'core-c/src/gpu/gpu_backend.c'
    foreach ($required in @(
        'clearra_gpu_backend_unavailable_vtable',
        'CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE',
        'clearra_gpu_backend_reject_user_provided_shader_path'
    )) {
        if ($source -notlike "*$required*") {
            Add-ArchitectureError "GPU unavailable adapter is missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'portable_vtable', 'real_stub_vtable', 'launch_real_stub', 'launch_portable'
    )) {
        if ($source -like "*$forbidden*") {
            Add-ArchitectureError "GPU adapter registers disconnected implementation '$forbidden'"
        }
    }
}
