# Prepare the real native archive and test the CLI in the same managed owner.
param([string]$ExecutionSurface = '')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$Root = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
. (Join-Path $Root 'scripts/lib/clearra-path-helpers.ps1')
. (Join-Path $Root 'scripts/lib/clearra-native-helpers.ps1')
. (Join-Path $Root 'scripts/lib/clearra-execution-surface.ps1')
. (Join-Path $Root 'scripts/lib/product-e2e-build.ps1')
if (-not (Test-StartTestsWindows)) {
    throw 'This helper binds the Windows MSVC native archive; use the existing platform-native CI entrypoint elsewhere.'
}
if ([string]::IsNullOrWhiteSpace($env:CARGO_TARGET_DIR)) {
    throw 'Run check-native-cli-contracts through invoke-clearra-build.ps1.'
}
Assert-ClearraCanonicalCargoTargetDir $env:CARGO_TARGET_DIR | Out-Null
Assert-ClearraTrustedExecutionSurface $ExecutionSurface 'native CLI process contracts'
$previousFlags = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS
$previousDebug = $env:CARGO_PROFILE_TEST_DEBUG
try {
    $libraryDirectory = Resolve-ProductE2ENativeLibraryDir
    Sync-ClearraNativeCargoLinkState -LibraryDirectory $libraryDirectory `
        -CargoTargetDirectory $env:CARGO_TARGET_DIR -WorkspaceRoot $Root | Out-Host
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS =
        Add-ClearraWindowsNativeRustLinkFlags $previousFlags $libraryDirectory
    $env:CARGO_PROFILE_TEST_DEBUG = '0'
    Push-Location $Root
    try {
        & cargo test --locked --offline -p clearra-cli --features native-c-core,wasm-cpu-runtime `
            --test process_e2e -j 2 -- --test-threads=2
        if ($LASTEXITCODE -ne 0) { throw "native CLI process contracts failed: $LASTEXITCODE" }
    } finally { Pop-Location }
} finally {
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $previousFlags
    $env:CARGO_PROFILE_TEST_DEBUG = $previousDebug
}
