# ManagedLocal validates the typed product source contract without asking Cargo
# to compile build helpers. Trusted execution owns compilation and execution.

function Invoke-ProductLibraryContractCheck {
    param(
        [string]$Root
    )

    $contractPath = Join-Path $Root 'crates/clearra-cli/tests/product_contract_e2e.rs'
    if (-not (Test-Path -LiteralPath $contractPath -PathType Leaf)) {
        throw "Product contract source is missing: $contractPath"
    }
    $contractText = Get-Content -LiteralPath $contractPath -Raw
    foreach ($marker in @(
        'run_with_args',
        'library_route_product_e2e_opening_2l_empty_matches_golden',
        'product_gpu_no_fallback_returns_error_when_unavailable'
    )) {
        if (-not $contractText.Contains($marker)) {
            throw "Product contract source is missing '$marker'"
        }
    }

    Write-Output '[product-e2e] source contract passed; no Rust source artifact was compiled or launched'
    Write-Output '[product-e2e] gate summary | execution_surface=ManagedLocal | product_e2e_route=source-contract | rust_test_execution=not-built | native_c_binding=disabled | process-launch=False | policy_fallback_used=false'
}
