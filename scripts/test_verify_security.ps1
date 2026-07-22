param()

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
$errors = New-Object System.Collections.Generic.List[string]

function Read-ProjectText([string]$RelativePath) {
    $path = Join-Path $Root $RelativePath
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        $errors.Add("missing required file: $RelativePath")
        return ""
    }
    return Get-Content -LiteralPath $path -Raw
}

foreach ($scriptFile in Get-ChildItem -LiteralPath (Join-Path $Root "scripts") -Recurse -File -Filter "*.ps1") {
    $tokens = $null
    $parseErrors = $null
    [void][System.Management.Automation.Language.Parser]::ParseFile(
        $scriptFile.FullName,
        [ref]$tokens,
        [ref]$parseErrors
    )
    foreach ($parseError in @($parseErrors)) {
        $errors.Add("PowerShell parse error in $($scriptFile.FullName): $($parseError.Message)")
    }
    $scriptText = Get-Content -LiteralPath $scriptFile.FullName -Raw
    $policySwitch = '-Execution' + 'Policy'
    $bypassValue = 'By' + 'pass'
    $bypassPattern = '(?i)["'']' + [regex]::Escape($policySwitch) +
        '["'']\s*,\s*["'']' + [regex]::Escape($bypassValue) + '["'']'
    if ($scriptText -match $bypassPattern) {
        $errors.Add("PowerShell child process overrides execution policy: $($scriptFile.FullName)")
    }
}

$runnerSurface = @(
    Read-ProjectText "scripts/clearra.ps1"
    Read-ProjectText "scripts/verify.ps1"
    Read-ProjectText "scripts/lib/core-c-tests.ps1"
    Read-ProjectText "scripts/lib/clearra-application-control.ps1"
    Read-ProjectText "scripts/lib/clearra-execution-surface.ps1"
    Read-ProjectText "scripts/lib/clearra-task-dispatch.ps1"
    Read-ProjectText "scripts/lib/progress/native_progress_runner.ps1"
    Read-ProjectText "scripts/lib/product-e2e-library-gate.ps1"
) -join "`n"
$managedProductGate = Read-ProjectText "scripts/lib/product-e2e-library-gate.ps1"

foreach ($required in @(
    'ValidateSet("ManagedLocal", "Trusted")',
    'ExecutionSurface = "ManagedLocal"',
    'Assert-ClearraTrustedExecutionSurface',
    'ManagedLocalProcessFree',
    'BUILD_TESTING=OFF',
    'CLEARRA_EXECUTION_SURFACE',
    'generatedExecutableLaunch',
    'cargo metadata',
    'source-contract',
    'E_WINDOWS_GENERATED_EXECUTION_REQUIRES_APPROVED_PACKAGE',
    'policy_fallback_used=false',
    'Get-ClearraWindowsRuntimeArtifactTrustReport',
    'Wait-ClearraGeneratedExecutableBlockEvidence',
    "-ParentProcessName 'ctest.exe'"
)) {
    if (-not $runnerSurface.Contains($required)) {
        $errors.Add("execution surface contract is missing '$required'")
    }
}

foreach ($forbiddenCargoArgs in @("@('check'", "@('build'", "@('test'")) {
    if ($managedProductGate.Contains($forbiddenCargoArgs)) {
        $errors.Add("ManagedLocal product source gate compiles Cargo artifacts: $forbiddenCargoArgs")
    }
}

foreach ($forbidden in @(
    'AllowPolicyFallback',
    'PolicySensitiveArtifactPreflight',
    'WithPolicyRetry',
    'App Control launch retry',
    'Set-AuthenticodeSignature',
    'New-SelfSignedCertificate',
    'Unblock-File'
)) {
    if ($runnerSurface.Contains($forbidden)) {
        $errors.Add("execution surface retains forbidden policy workaround '$forbidden'")
    }
}

foreach ($removedPath in @(
    "scripts/unblock-local-dev.ps1",
    "scripts/dev-sign.ps1",
    "scripts/dev-sign-core-tests.ps1",
    "scripts/dev-sign-cli.ps1",
    "scripts/lib/verify-policy-retry.ps1",
    "scripts/lib/core-c-test-artifacts.ps1",
    "scripts/diagnose-cargo-test-artifact.ps1"
)) {
    if (Test-Path -LiteralPath (Join-Path $Root $removedPath)) {
        $errors.Add("removed execution workaround still exists: $removedPath")
    }
}

foreach ($guardedScript in @(
    "scripts/run-rust-test.ps1",
    "scripts/ux-smoke.ps1",
    "scripts/product-e2e.ps1",
    "scripts/desktop-host-check.ps1",
    "scripts/worker-e2e.ps1",
    "scripts/wasm-command-runtime-check.ps1",
    "scripts/webgpu-backend-check.ps1",
    "scripts/job-worker-progress-check.ps1"
)) {
    if ((Read-ProjectText $guardedScript) -notlike '*Assert-ClearraTrustedExecutionSurface*') {
        $errors.Add("process script lacks Trusted execution guard: $guardedScript")
    }
}

if ($errors.Count -gt 0) {
    $errors | ForEach-Object { Write-Error $_ }
    exit 1
}

Write-Output "execution_surface_security=passed"
Write-Output "managed_local_generated_executable_launch=false"
Write-Output "trusted_execution_policy=umci-preflight-or-single-attempt-fail-closed"
exit 0
