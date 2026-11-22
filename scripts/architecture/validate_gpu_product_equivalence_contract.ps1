# Historical task name retained; current product GPU requests are retired.
function Invoke-GpuProductEquivalenceContractValidation() {
    $e2e = @(
        Read-Text 'scripts/lib/product-e2e-run.ps1'
        Read-Text 'crates/clearra-cli/tests/product_contract_e2e.rs'
        Read-Text 'crates/clearra-cli/tests/process_e2e/process_e2e_setup_backend.rs'
    ) -join "`n"
    foreach ($required in @(
        'product_gpu_and_hybrid_requests_are_retired_without_cpu_fallback',
        'process_e2e_retired_gpu_has_no_cpu_fallback',
        '--allow-backend-fallback', '--no-backend-fallback',
        'webgpu_backend_retired', 'E_BACKEND_GPU_UNAVAILABLE'
    )) {
        if ($e2e -notlike "*$required*") {
            Add-ArchitectureError "GPU retirement ProductE2E is missing '$required'"
        }
    }
}
