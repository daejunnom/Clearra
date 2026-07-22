# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Native search GPU is unavailable in the default build. CPU fallback remains
# a CPU result with an explicit reason. WebGPU is owned by clearra-webgpu.

function Invoke-GpuPackingBackendValidation() {
    foreach ($requiredFile in @(
        'core-c/include/clr_gpu.h',
        'core-c/include/clr_gpu_worker.h',
        'core-c/src/gpu/gpu_backend.c',
        'core-c/src/gpu/gpu_worker_unavailable.c',
        'core-c/src/packing/cpu_packing_reference.c',
        'crates/clearra-core-ffi/Cargo.toml',
        'crates/clearra-core-executor/Cargo.toml',
        'crates/clearra-core-executor/src/backend/mod.rs'
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "GPU contract file missing: $requiredFile"
        }
    }

    $nativeAbi = @(
        Read-Text 'core-c/include/clr_gpu.h'
        Read-Text 'core-c/src/gpu/gpu_backend.c'
        Read-Text 'core-c/src/gpu/gpu_worker_unavailable.c'
    ) -join "`n"
    foreach ($required in @(
        'piece_source_id', 'piece_multiset_window', 'pattern_universe_id',
        'pattern_weight_model_id', 'used_cpu_fallback', 'fallback_used',
        'can_source_exact_probability'
    )) {
        if ($nativeAbi -notlike "*$required*") {
            Add-ArchitectureError "GPU contract is missing '$required'"
        }
    }

    $ffiCargo = Read-Text 'crates/clearra-core-ffi/Cargo.toml'
    $executorCargo = Read-Text 'crates/clearra-core-executor/Cargo.toml'
    $backendModule = Read-Text 'crates/clearra-core-executor/src/backend/mod.rs'
    if (-not $ffiCargo.Contains('experimental-native-gpu = []') -or
        -not $executorCargo.Contains('experimental-native-gpu = ["clearra-core-ffi/experimental-native-gpu"]')) {
        Add-ArchitectureError 'Disconnected native GPU worker contracts must be isolated behind the default-off experimental-native-gpu feature'
    }
    foreach ($guardedModule in @('gpu_trust_state', 'gpu_worker', 'hybrid_backpressure_report')) {
        $pattern = "(?s)#\[cfg\(feature = `"experimental-native-gpu`"\)\]\s*pub mod $guardedModule;"
        if ($backendModule -notmatch $pattern) {
            Add-ArchitectureError "Default backend module must feature-gate '$guardedModule'"
        }
    }

    $surface = "$nativeAbi`n$backendModule"
    foreach ($forbidden in @(
        'PortableReference', 'portable-reference', 'gpu_worker_portable',
        'CLEARRA_GPU_BACKEND_CUDA', 'CLEARRA_GPU_BACKEND_OPENCL',
        'CLEARRA_GPU_BACKEND_VULKAN', 'CLEARRA_GPU_BACKEND_DIRECTX',
        'GpuBfsBackendStub', 'gpu-assisted-reference', 'pub mod gpu_worker;'
    )) {
        if ($forbidden -eq 'pub mod gpu_worker;') {
            if ($backendModule -match '(?m)^pub mod gpu_worker;$') {
                Add-ArchitectureError "GPU product surface contains unguarded backend '$forbidden'"
            }
        } elseif ($surface -like "*$forbidden*") {
            Add-ArchitectureError "GPU product surface contains disconnected backend '$forbidden'"
        }
    }
}

. (Join-Path $PSScriptRoot 'validate_hybrid_scheduler.ps1')
