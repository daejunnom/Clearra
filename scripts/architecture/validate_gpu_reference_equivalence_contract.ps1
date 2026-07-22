# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# CPU reference code may confirm or implement fallback, but it is never a GPU
# backend and never upgrades a GPU trust state by label alone.

function Invoke-GpuReferenceEquivalenceContractValidation() {
    foreach ($requiredFile in @(
        'core-c/src/packing/cpu_packing_reference.c',
        'core-c/src/gpu/gpu_readback_reduce.c',
        'core-c/src/gpu/gpu_worker_unavailable.c'
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "CPU reference contract file missing: $requiredFile"
        }
    }
    $surface = @(
        Read-Text 'core-c/src/packing/cpu_packing_reference.c'
        Read-Text 'core-c/src/gpu/gpu_backend.c'
        Read-Text 'core-c/src/gpu/gpu_worker_unavailable.c'
    ) -join "`n"
    foreach ($required in @(
        'clearra_cpu_packing_reference_generate', 'used_cpu_fallback = 1u',
        'CLEARRA_GPU_WORKER_UNAVAILABLE', 'can_source_exact_probability = 0u'
    )) {
        if ($surface -notlike "*$required*") {
            Add-ArchitectureError "CPU reference/fallback contract is missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'gpu_worker_portable', 'portable_reference', 'PortableReference',
        'CLEARRA_GPU_WORKER_TRUST_PORTABLE_REFERENCE'
    )) {
        if ($surface -like "*$forbidden*") {
            Add-ArchitectureError "CPU reference is mislabeled as GPU: '$forbidden'"
        }
    }
}
