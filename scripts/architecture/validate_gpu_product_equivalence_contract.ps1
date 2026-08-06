# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Native GPU availability depends on both the runtime device and the connected
# kernel implementation. Product equivalence therefore accepts either explicit
# unavailable reason while still requiring CPU parity and no-fallback rejection.

function Invoke-GpuProductEquivalenceContractValidation() {
    $e2e = @(
        Read-Text 'scripts/product-e2e.ps1'
        Read-Text 'scripts/lib/product-e2e-run.ps1'
        Read-Text 'scripts/lib/product-e2e-typed-assertions.ps1'
        Read-Text 'crates/clearra-cli/tests/product_contract_e2e.rs'
        Read-Text 'crates/clearra-cli/tests/product_contract_e2e/support.rs'
    ) -join "`n"
    foreach ($required in @(
        'product_backend_cpu_gpu_hybrid_same_opening_2l',
        'product_backend_cpu_gpu_hybrid_same_scenario_4l',
        'product_gpu_no_fallback_returns_error_when_unavailable',
        'product_gpu_allow_fallback_reports_reason',
        'backend_fallback_used',
        'Assert-ProductE2EJsonFieldGpuUnavailableReason',
        'Assert-ProductE2EJsonFieldHybridUnavailableReason',
        'Assert-ProductE2EJsonFieldNoFallbackReason',
        'Assert-ProductE2EJsonFieldUniqueEquals',
        'Assert-ProductE2EGpuCpuFallbackReport',
        'Assert-ProductE2EHybridCpuSelectionReport',
        'gpu_backend_not_connected', 'gpu_device_not_found', 'gpu_kernel_unavailable',
        'cpu-selected', 'reported inconsistent values'
    )) {
        if ($e2e -notlike "*$required*") {
            Add-ArchitectureError "GPU unavailable/fallback ProductE2E is missing '$required'"
        }
    }
    foreach ($forbidden in @(
        'gpu-assisted-reference', 'portable-reference',
        'product_gpu_assisted_opening_2l_matches_cpu_without_using_fallback',
        'product_gpu_assisted_scenario_4l_matches_cpu_without_using_fallback'
    )) {
        if ($e2e -like "*$forbidden*") {
            Add-ArchitectureError "ProductE2E still accepts fake GPU result '$forbidden'"
        }
    }
}
