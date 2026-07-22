# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-GpuPackingBackendContractValidation() {
    $rustSurface = @(
        Read-Text 'crates/clearra-core-executor/src/backend/gpu_trust_state.rs'
        Read-Text 'crates/clearra-core-executor/src/backend/gpu_worker/gpu_backend_kind.rs'
        Read-Text 'crates/clearra-core-executor/src/backend/gpu_worker/gpu_backend_capability.rs'
        Read-Text 'crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_exactness_gate.rs'
    ) -join "`n"
    foreach ($required in @(
        'NativeCompute', 'Disabled', 'native_gpu_backend_not_built',
        'GpuComputedUnconfirmed', 'GpuComputedCpuConfirmed',
        'DeterministicReferenceMatched', 'GpuComputedMismatch',
        'can_source_exact_probability'
    )) {
        if ($rustSurface -notlike "*$required*") {
            Add-ArchitectureError "Rust GPU trust contract is missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'PortableReference', 'CudaUnavailable', 'OpenClUnavailable',
        'VulkanUnavailable', 'DirectXUnavailable'
    )) {
        if ($rustSurface -like "*$forbidden*") {
            Add-ArchitectureError "Rust registers unimplemented GPU kind '$forbidden'"
        }
    }

    $cSurface = @(
        Read-Text 'core-c/include/clr_gpu.h'
        Read-Text 'core-c/include/clr_gpu_worker.h'
        Read-Text 'core-c/src/gpu/gpu_backend.c'
        Read-Text 'core-c/src/gpu/gpu_worker_unavailable.c'
    ) -join "`n"
    foreach ($required in @(
        'CLEARRA_GPU_BACKEND_NATIVE_COMPUTE', 'CLEARRA_GPU_BACKEND_DISABLED',
        'clearra_gpu_fallback_to_cpu_packing', 'used_cpu_fallback',
        'CLEARRA_GPU_WORKER_TRUST_GPU_COMPUTED_UNCONFIRMED',
        'CLEARRA_GPU_WORKER_TRUST_GPU_COMPUTED_CPU_CONFIRMED'
    )) {
        if ($cSurface -notlike "*$required*") {
            Add-ArchitectureError "C GPU contract is missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'CLEARRA_GPU_BACKEND_PORTABLE_REFERENCE', 'CLEARRA_GPU_BACKEND_CUDA',
        'CLEARRA_GPU_BACKEND_OPENCL', 'CLEARRA_GPU_BACKEND_VULKAN',
        'CLEARRA_GPU_BACKEND_DIRECTX'
    )) {
        if ($cSurface -like "*$forbidden*") {
            Add-ArchitectureError "C registers unimplemented GPU backend '$forbidden'"
        }
    }
}
