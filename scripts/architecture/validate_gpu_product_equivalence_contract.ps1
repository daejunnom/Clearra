# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# Native GPU has no connected default implementation. Product equivalence is
# therefore CPU-vs-explicit-fallback plus no-fallback rejection.

function Invoke-GpuProductEquivalenceContractValidation() {
    $e2e = @(
        Read-Text 'scripts/product-e2e.ps1'
        Read-Text 'crates/clearra-cli/tests/product_contract_e2e.rs'
        Read-Text 'crates/clearra-cli/tests/product_contract_e2e/support.rs'
    ) -join "`n"
    foreach ($required in @(
        'product_backend_cpu_gpu_hybrid_same_opening_2l',
        'product_backend_cpu_gpu_hybrid_same_scenario_4l',
        'product_gpu_no_fallback_returns_error_when_unavailable',
        'product_gpu_allow_fallback_reports_reason',
        'backend_fallback_used', 'gpu_kernel_unavailable'
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
