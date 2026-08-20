function Invoke-TestPolicyArchitectureValidation() {
    $clearra = Read-PhysicalText "scripts/clearra.ps1"
    $verify = Read-PhysicalText "scripts/verify.ps1"
    $surface = Read-PhysicalText "scripts/lib/clearra-execution-surface.ps1"
    $coreTests = Read-PhysicalText "scripts/lib/core-c-tests.ps1"
    $productGate = Read-PhysicalText "scripts/lib/product-e2e-library-gate.ps1"
    $productProcess = Read-PhysicalText "scripts/lib/product-process-surface.ps1"
    $applicationControl = Read-PhysicalText "scripts/lib/clearra-application-control.ps1"
    $testPolicy = Read-PhysicalText "docs/test-policy.md"
    $readme = Read-PhysicalText "README.md"

    foreach ($required in @(
        'ValidateSet("ManagedLocal", "Trusted")',
        '[string]$ExecutionSurface = "ManagedLocal"',
        '$resolvedRequestedRuntime = Resolve-ClearraRuntimeEnvironment $RuntimeEnvironment',
        'Assert-ClearraRequestedTaskSurfaces',
        'Assert-ClearraTrustedExecutionSurface'
    )) {
        if (-not "$clearra`n$surface".Contains($required)) {
            Add-ArchitectureError "execution surface contract is missing '$required'"
        }
    }

    foreach ($required in @(
        'generated_executable_policy = if ($blocked) { "deny" } else { "allow" }',
        'E_WINDOWS_GENERATED_EXECUTION_REQUIRES_APPROVED_PACKAGE',
        "`$status.generated_executable_policy -ne 'allow'",
        "[string]`$signature.Status -ne 'Valid'",
        'Get-ClearraWindowsRuntimeArtifactTrustReport',
        "'deny-unapproved-artifact'",
        'policy_ids = $policyIds'
    )) {
        if (-not $applicationControl.Contains($required)) {
            Add-ArchitectureError "Windows execution preflight is missing '$required'"
        }
    }

    foreach ($required in @(
        "`$runtime -eq 'windows'",
        "`$runtime -eq 'wsl'",
        'Invoke-CoreCTestWsl',
        'Assert-ClearraRuntimeEnvironmentAvailable'
    )) {
        if (-not "$surface`n$coreTests".Contains($required)) {
            Add-ArchitectureError "independent runtime selection is missing '$required'"
        }
    }

    foreach ($required in @(
        '-BuildOnly:(-not $trustedExecution)',
        '"metadata", "--no-deps", "--format-version", "1"',
        'Invoke-VerifyCargoCompile',
        'Rust test targets were neither compiled nor launched on ManagedLocal',
        'policy_fallback_used=false'
    )) {
        if (-not $verify.Contains($required)) {
            Add-ArchitectureError "verify runner is missing process-free marker '$required'"
        }
    }

    foreach ($required in @(
        '[switch]$BuildOnly',
        "'-DBUILD_TESTING=OFF'",
        'ManagedLocalProcessFree',
        'no CTest executable was generated',
        "Assert-ClearraWindowsGeneratedExecutionAllowed 'core-c CTest execution'",
        'Wait-ClearraGeneratedExecutableBlockEvidence',
        "-ParentProcessName 'ctest.exe'"
    )) {
        if (-not $coreTests.Contains($required)) {
            Add-ArchitectureError "C core runner is missing managed-local marker '$required'"
        }
    }

    foreach ($required in @(
        'Invoke-ProductLibraryContractCheck',
        'product_e2e_route=source-contract',
        'no Rust source artifact was compiled or launched',
        'Assert-ClearraTrustedExecutionSurface'
    )) {
        if (-not "$productGate`n$productProcess".Contains($required)) {
            Add-ArchitectureError "product gate is missing execution-surface marker '$required'"
        }
    }

    foreach ($forbiddenCargoArgs in @("@('check'", "@('build'", "@('test'")) {
        if ($productGate.Contains($forbiddenCargoArgs)) {
            Add-ArchitectureError 'ManagedLocal product source gate must not compile Cargo artifacts'
        }
    }
    if ($productGate.Contains('Invoke-ProductCargoOnce')) {
        Add-ArchitectureError 'ManagedLocal product source gate must not own a Cargo compile helper'
    }

    $defaultRunnerSurface = @(
        $clearra,
        $verify,
        $surface,
        $productGate,
        (Read-PhysicalText "scripts/export-pc-artifact.ps1"),
        (Read-PhysicalText "scripts/desktop-host-check.ps1"),
        (Read-PhysicalText "scripts/wasm-command-runtime-check.ps1"),
        (Read-PhysicalText "scripts/lib/clearra-task-dispatch.ps1"),
        (Read-PhysicalText "scripts/lib/product-process-surface.ps1"),
        (Read-PhysicalText "scripts/run-rust-test.ps1"),
        (Read-PhysicalText "scripts/lib/progress/native_progress_runner.ps1")
    ) -join "`n"
    foreach ($forbidden in @(
        'AllowPolicyFallback',
        'PolicySensitiveArtifactPreflight',
        'WithPolicyRetry',
        'App Control launch retry',
        'Set-AuthenticodeSignature',
        'New-SelfSignedCertificate',
        'Unblock-File',
        'wsl.exe',
        'Invoke-ClearraWslScript',
        'RunnerSurface'
    )) {
        if ($defaultRunnerSurface.Contains($forbidden)) {
            Add-ArchitectureError "default runner retains policy workaround '$forbidden'"
        }
    }

    foreach ($removedPath in @(
        "scripts/unblock-local-dev.ps1",
        "scripts/dev-sign.ps1",
        "scripts/dev-sign-core-tests.ps1",
        "scripts/dev-sign-cli.ps1",
        "scripts/lib/verify-policy-retry.ps1",
        "scripts/lib/core-c-test-artifacts.ps1",
        "scripts/lib/product-e2e-preflight.ps1",
        "scripts/diagnose-cargo-test-artifact.ps1",
        "scripts/lib/clearra-wsl-execution.ps1"
    )) {
        if (Test-Path -LiteralPath (Join-Path $Root $removedPath)) {
            Add-ArchitectureError "removed policy workaround still exists: $removedPath"
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
        if ((Read-PhysicalText $guardedScript) -notlike '*Assert-ClearraTrustedExecutionSurface*') {
            Add-ArchitectureError "process script lacks Trusted execution guard: $guardedScript"
        }
    }

    $policySwitch = '-Execution' + 'Policy'
    $bypassValue = 'By' + 'pass'
    $bypassPattern = '(?i)["'']' + [regex]::Escape($policySwitch) +
        '["'']\s*,\s*["'']' + [regex]::Escape($bypassValue) + '["'']'
    foreach ($scriptFile in Get-ChildItem -LiteralPath (Join-Path $Root 'scripts') -Recurse -File -Filter '*.ps1') {
        if ((Get-Content -LiteralPath $scriptFile.FullName -Raw) -match $bypassPattern) {
            Add-ArchitectureError "PowerShell child process overrides execution policy: $($scriptFile.FullName)"
        }
    }

    foreach ($requiredDoc in @(
        'ManagedLocal',
        'Trusted',
        'no generated test executable launch',
        'does not compile workspace',
        'fail closed',
        'enterprise-approved package',
        'policy_fallback_used=false',
        'Code Integrity events 3033',
        'policy ID'
    )) {
        if ("$testPolicy`n$readme" -notlike "*$requiredDoc*") {
            Add-ArchitectureError "test policy documentation is missing '$requiredDoc'"
        }
    }

    if (Test-Path -LiteralPath (Join-Path $Root "build.rs")) {
        Add-ArchitectureError "workspace root build.rs is forbidden"
    }
    foreach ($cargoToml in Get-ChildItem -LiteralPath (Join-Path $Root "crates") -Recurse -File -Filter Cargo.toml) {
        $buildScript = Join-Path $cargoToml.Directory.FullName "build.rs"
        if (Test-Path -LiteralPath $buildScript -PathType Leaf) {
            Add-ArchitectureError "crate-local Cargo build script is forbidden: $buildScript"
        }
    }

    if (Test-Path -LiteralPath (Join-Path $Root "package.json")) {
        $packageJson = Read-PhysicalText "package.json"
        if ($packageJson -like '*"test"*' -and $packageJson -notlike '*scripts/verify.ps1*') {
            Add-ArchitectureError "package.json test script must delegate to scripts/verify.ps1"
        }
    }
}
