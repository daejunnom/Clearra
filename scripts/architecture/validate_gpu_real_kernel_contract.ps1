# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Native GPU kernels are not connected in the default build. Finish-or-Remove
# requires an explicit unavailable backend and no prototype kernel surface.

function Invoke-GpuRealKernelContractValidation() {
    foreach ($forbiddenFile in @(
        'core-c/kernels/cuda/packing_kernel.cu',
        'core-c/kernels/opencl/packing_kernel.cl',
        'core-c/kernels/portable/packing_kernel_ref.c'
    )) {
        if (Test-Path -LiteralPath (Join-Path $Root $forbiddenFile)) {
            Add-ArchitectureError "Unconnected kernel must not ship in the default build: $forbiddenFile"
        }
    }

    $surface = @(
        Read-Text 'core-c/include/clr_gpu.h'
        Read-Text 'core-c/src/gpu/gpu_backend.c'
        Read-Text 'core-c/src/gpu/gpu_worker_unavailable.c'
    ) -join "`n"
    foreach ($required in @(
        'CLEARRA_GPU_BACKEND_NATIVE_COMPUTE', 'CLEARRA_GPU_BACKEND_DISABLED',
        'CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE',
        'CLEARRA_GPU_WORKER_TRUST_UNAVAILABLE', 'candidate_count = 0u'
    )) {
        if ($surface -notlike "*$required*") {
            Add-ArchitectureError "Unavailable native GPU contract is missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'clearra_gpu_real_kernel', 'ClearraGpuKernelPrototypeResult',
        'CLEARRA_GPU_BACKEND_CUDA', 'CLEARRA_GPU_BACKEND_OPENCL',
        'CLEARRA_GPU_BACKEND_VULKAN', 'CLEARRA_GPU_BACKEND_DIRECTX'
    )) {
        if ($surface -like "*$forbidden*") {
            Add-ArchitectureError "Default native GPU surface contains prototype marker '$forbidden'"
        }
    }
}
