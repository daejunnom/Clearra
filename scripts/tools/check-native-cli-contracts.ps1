# Test the ordinary native executable and its Rust product library.
param([string]$ExecutionSurface = '', [switch]$FetchDependencies)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $Root 'scripts/lib/clearra-path-helpers.ps1')
. (Join-Path $Root 'scripts/lib/clearra-execution-surface.ps1')
if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    throw 'Run check-native-cli-contracts through invoke-clearra-build.ps1.'
}
Assert-ClearraCanonicalCargoTargetDir $env:CARGO_TARGET_DIR | Out-Null
Assert-ClearraTrustedExecutionSurface $ExecutionSurface 'Rust CLI product process contracts'
if ($FetchDependencies) {
    Push-Location $Root
    try {
        & cargo fetch --locked
        if ($LASTEXITCODE -ne 0) { throw "Rust CLI dependency fetch failed: $LASTEXITCODE" }
    } finally { Pop-Location }
}
$previousDebug = $env:CARGO_PROFILE_TEST_DEBUG
try {
    $env:CARGO_PROFILE_TEST_DEBUG = '0'
    Push-Location $Root
    try {
        & cargo test --locked --offline -p clearra-cli --features wasm-cpu-runtime `
            --test process_e2e --test rust_product_process_contract --test terminal_supply_public_contract `
            -j 2 -- --test-threads=1
        if ($LASTEXITCODE -ne 0) { throw "Rust CLI process contracts failed: $LASTEXITCODE" }
        & cargo test --locked --offline -p clearra-cli --features wasm-cpu-runtime `
            --lib tablebase_download -j 2 -- --test-threads=2
        if ($LASTEXITCODE -ne 0) { throw "Rust CLI TB download contracts failed: $LASTEXITCODE" }
    } finally { Pop-Location }
} finally {
    $env:CARGO_PROFILE_TEST_DEBUG = $previousDebug
}
