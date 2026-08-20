param(
    [switch]$Strict,
    [ValidateSet("ManagedLocal", "Trusted")]
    [string]$ExecutionSurface = "ManagedLocal",
    [switch]$RunVerifySecurity,
    [string]$ReportPath,
    [switch]$VerboseLog,
    [switch]$ShowWarnings,
    [int]$OutputExcerptLines = 40,
    [int]$WarningDetailLimit = 5,
    [int]$Workers = [Math]::Max(1, [Environment]::ProcessorCount),
    [string]$CoreCBuildDir,
    [string]$CargoPath = "cargo",
    [string]$PowerShellPath = "powershell"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot "lib/clearra-path-helpers.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-execution-surface.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-application-control.ps1")
. (Join-Path $PSScriptRoot "lib/core-c-tests.ps1")
. (Join-Path $PSScriptRoot "lib/architecture-validation.ps1")
. (Join-Path $PSScriptRoot "lib/progress.ps1")

if ($Workers -lt 1) {
    throw "-Workers must be at least 1."
}
if ($OutputExcerptLines -lt 1) {
    throw "-OutputExcerptLines must be at least 1."
}
if ($Strict.IsPresent) {
    Assert-ClearraTrustedExecutionSurface $ExecutionSurface "Strict verification"
}
if ($RunVerifySecurity.IsPresent -and
    -not (Test-ClearraTrustedExecutionSurface $ExecutionSurface)) {
    Assert-ClearraTrustedExecutionSurface $ExecutionSurface "runner security execution"
}

$Root = Resolve-ClearraRoot
$trustedExecution = Test-ClearraTrustedExecutionSurface $ExecutionSurface
$applicationControl = Get-ClearraApplicationControlStatus
$nativeTestExecutionAllowed = $trustedExecution
$previousCargoTargetDir = $env:CARGO_TARGET_DIR
$setCargoTargetDir = $false
$cargoTargetDir = if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
    Get-ClearraCargoTargetDir
} else {
    Assert-ClearraCanonicalCargoTargetDir $previousCargoTargetDir
}
$coreBuildDir = if ([string]::IsNullOrWhiteSpace($CoreCBuildDir)) {
    $defaultCoreBuildName = if ($trustedExecution) {
        "core-c-test-cache"
    } else {
        "core-c-library-cache"
    }
    Resolve-ClearraArtifactPath $defaultCoreBuildName $Root
} else {
    Resolve-ClearraArtifactPath $CoreCBuildDir $Root
}

function Invoke-VerifyNative(
    [string]$FileName,
    [string[]]$Arguments,
    [string]$Label
) {
    if ($VerboseLog.IsPresent) {
        Write-Output "==> $FileName $($Arguments -join ' ')"
    }
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $output = @(& $FileName @Arguments 2>&1)
        $exitCode = $LASTEXITCODE
    } finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($VerboseLog.IsPresent) {
        $output | ForEach-Object { Write-Output $_.ToString() }
    }
    if ($exitCode -ne 0) {
        if (Test-ClearraApplicationControlBlockOutput $output) {
            throw (New-ClearraLocalSourceBuildBlockedMessage $Label $applicationControl)
        }
        $excerpt = @($output | Select-Object -Last $OutputExcerptLines) -join "`n"
        throw "$Label failed with exit code $exitCode`n$excerpt"
    }
    return @($output)
}

function Invoke-VerifyCargoCompile(
    [string[]]$Arguments,
    [string]$Label
) {
    Assert-ClearraTrustedExecutionSurface $ExecutionSurface $Label
    return Invoke-VerifyNative $CargoPath $Arguments $Label
}

function Assert-ProductContractSourceSurface {
    $contract = Join-Path $Root "crates/clearra-cli/tests/product_contract_e2e.rs"
    if (-not (Test-Path -LiteralPath $contract -PathType Leaf)) {
        throw "product contract target is missing: $contract"
    }
    $text = Get-Content -LiteralPath $contract -Raw
    foreach ($marker in @(
        "run_with_args",
        "library_route_product_e2e_opening_2l_empty_matches_golden",
        "product_gpu_no_fallback_returns_error_when_unavailable",
        "product_gpu_allow_fallback_reports_reason"
    )) {
        if (-not $text.Contains($marker)) {
            throw "product contract source is missing '$marker'"
        }
    }
}

function Write-VerifyReport(
    [string]$Status,
    [object]$CoreResult,
    [string]$ArchitectureStatus,
    [string]$RustMode,
    [string]$WasmMode,
    [string]$ProductRoute
) {
    if ([string]::IsNullOrWhiteSpace($ReportPath)) {
        return
    }
    $resolved = Resolve-ClearraReportPath $ReportPath $Root
    $directory = Split-Path -Parent $resolved
    if (-not (Test-Path -LiteralPath $directory)) {
        New-Item -ItemType Directory -Force -Path $directory | Out-Null
    }
    [pscustomobject]@{
        status = $Status
        execution_surface = $ExecutionSurface
        rust_test_execution = $RustMode
        wasm_exact_execution = $WasmMode
        c_core_test_execution = if ($CoreResult.TestExecuted) { "launched" } else { "not-built" }
        product_e2e_route = $ProductRoute
        architecture_validation = $ArchitectureStatus
        application_control = $applicationControl.user_mode_code_integrity_policy
        policy_fallback_used = $false
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $resolved -Encoding UTF8
    Write-Output "verification_report: $resolved"
}

New-Item -ItemType Directory -Force -Path $cargoTargetDir, $coreBuildDir | Out-Null
if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
    $env:CARGO_TARGET_DIR = $cargoTargetDir
    $setCargoTargetDir = $true
}

Push-Location $Root
try {
    $progress = New-ClearraProgressScope `
        -Name "verify" `
        -Total 8 `
        -Workers 1 `
        -VerboseLog:$VerboseLog.IsPresent

    Invoke-ClearraProgressCase -Scope $progress -Name "cargo fmt" -Body {
        Invoke-VerifyNative $CargoPath @("fmt", "--all", "--check") "cargo fmt"
    }
    Invoke-ClearraProgressCase -Scope $progress -Name "Rust workspace" -Body {
        if ($trustedExecution) {
            Invoke-VerifyCargoCompile @("check", "--workspace") "cargo check --workspace"
        } else {
            Invoke-VerifyNative $CargoPath @(
                "metadata", "--no-deps", "--format-version", "1"
            ) "cargo metadata"
            Write-Output "[verify] ManagedLocal validated Cargo metadata without compiling source artifacts"
        }
    }
    Invoke-ClearraProgressCase -Scope $progress -Name "Rust test surface" -Body {
        if ($trustedExecution) {
            Invoke-VerifyCargoCompile @("check", "--workspace", "--tests") "cargo check --workspace --tests"
        } else {
            Write-Output "[verify] ManagedLocal does not compile Rust tests because Cargo build helpers are generated executables"
        }
    }
    Invoke-ClearraProgressCase -Scope $progress -Name "WebGPU test surface" -Body {
        if ($trustedExecution) {
            Invoke-VerifyCargoCompile @(
                "check", "-p", "clearra-core-executor", "--features", "native-c-core,webgpu-search", "--tests"
            ) "cargo check clearra-core-executor WebGPU tests"
        } else {
            Write-Output "[verify] ManagedLocal leaves WebGPU compilation to Trusted execution"
        }
    }

    $script:VerifyCoreResult = $null
    Invoke-ClearraProgressCase -Scope $progress -Name "C core" -PreserveOutput -Body {
        Complete-ClearraProgressLine $progress
        $script:VerifyCoreResult = Invoke-CoreCTest `
            -BuildDir $coreBuildDir `
            -Configuration "Debug" `
            -BuildOnly:(-not $trustedExecution) `
            -Workers $Workers
        if ($script:VerifyCoreResult.Status -eq "Failed") {
            throw "C core verification failed"
        }
    }

    Invoke-ClearraProgressCase -Scope $progress -Name "product contract" -Body {
        Assert-ProductContractSourceSurface
        if ($nativeTestExecutionAllowed) {
            Invoke-VerifyCargoCompile @(
                "test", "-p", "clearra-cli", "--features", "native-c-core,webgpu-search",
                "--lib", "product_contract_e2e::",
                "--", "--test-threads=1"
            ) "product contract E2E"
        } else {
            Write-Output "[verify] ManagedLocal validated the product contract source without compiling a test harness"
        }
    }

    Invoke-ClearraProgressCase -Scope $progress -Name "Rust exact execution" -Body {
        $script:VerifyRustMode = "not-built"
        $script:VerifyWasmMode = "not-run"
        if ($nativeTestExecutionAllowed) {
            Invoke-VerifyCargoCompile @(
                "test", "--workspace", "--", "--test-threads=1"
            ) "cargo test --workspace"
            $script:VerifyRustMode = "launched"
        } else {
            Write-Output "[verify] Rust test targets were neither compiled nor launched on ManagedLocal"
        }
        if ($RunVerifySecurity.IsPresent) {
            if (-not $trustedExecution) {
                throw "runner security execution requires -ExecutionSurface Trusted"
            }
            Invoke-VerifyNative $PowerShellPath @(
                "-NoProfile", "-File", (Join-Path $PSScriptRoot "test_verify_security.ps1")
            ) "runner security"
        }
    }

    $script:VerifyArchitectureStatus = "failed"
    Invoke-ClearraProgressCase -Scope $progress -Name "architecture validation" -PreserveOutput -Body {
        Complete-ClearraProgressLine $progress
        $result = Invoke-ArchitectureValidation `
            -Workers $Workers `
            -ShowWarnings:($ShowWarnings.IsPresent -or $VerboseLog.IsPresent) `
            -WarningDetailLimit $WarningDetailLimit
        if ($result.Status -eq "Failed") {
            throw "architecture validation failed with $($result.ErrorCount) error(s)"
        }
        $script:VerifyArchitectureStatus = "passed"
    }
    Complete-ClearraProgressLine $progress

    $cMode = if ($script:VerifyCoreResult.TestExecuted) { "launched" } else { "not-built" }
    $productRoute = if ($nativeTestExecutionAllowed) {
        "library"
    } else {
        "source-contract"
    }
    Write-Output "[verify] gate summary | execution_surface=$ExecutionSurface | rust_test_execution=$script:VerifyRustMode | wasm_exact_execution=$script:VerifyWasmMode | c_core_test_execution=$cMode | product_e2e_route=$productRoute | architecture_validation=$script:VerifyArchitectureStatus | application_control=$($applicationControl.user_mode_code_integrity_policy) | policy_fallback_used=false"
    Write-VerifyReport `
        "passed" `
        $script:VerifyCoreResult `
        $script:VerifyArchitectureStatus `
        $script:VerifyRustMode `
        $script:VerifyWasmMode `
        $productRoute
} finally {
    Pop-Location
    if ($setCargoTargetDir) {
        Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    }
}
