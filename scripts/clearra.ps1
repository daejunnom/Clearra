param(
    [Alias("Mode")]
    [string[]]$Task = @("Local"),
    [int]$Workers = [Math]::Max(1, [Environment]::ProcessorCount),
    [switch]$VerboseLog,
    [switch]$ShowWarnings,
    [switch]$Json,
    [int]$OutputExcerptLines = 60,
    [int]$WarningDetailLimit = 5,
    [switch]$KeepBuildCache,
    [string]$CoreCBuildDir,
    [string]$CMakeBuildType,
    [string]$ReportDir,
    [string]$ReportPath,
    [int]$Minutes = 60,
    [ValidateSet("ManagedLocal", "Trusted")]
    [string]$ExecutionSurface = "ManagedLocal",
    [ValidateSet(
        "Full",
        "Foundation",
        "FoundationNoProductDebt",
        "FoundationAdversarialCorrectness",
        "FoundationDesktopHost",
        "Sanitizer",
        "Rust",
        "Pages"
    )]
    [string]$ReleaseAcceptanceShard = "Full",
    [ValidateSet("auto", "windows", "wsl", "wasm")]
    [string]$RuntimeEnvironment = "auto",
    [string]$WslDistribution = "Ubuntu",
    [string]$CargoPath = "cargo",
    [string]$PowerShellPath = "powershell"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot "lib/core-c-tests.ps1")
. (Join-Path $PSScriptRoot "lib/adversarial-correctness.ps1")
. (Join-Path $PSScriptRoot "lib/no-product-debt.ps1")
. (Join-Path $PSScriptRoot "lib/c-sanitizer-gate.ps1")
. (Join-Path $PSScriptRoot "lib/rust-exact-tests.ps1")
. (Join-Path $PSScriptRoot "lib/wasm-release-gate.ps1")
. (Join-Path $PSScriptRoot "lib/render-golden-gate.ps1")
. (Join-Path $PSScriptRoot "lib/product-e2e-library-gate.ps1")
. (Join-Path $PSScriptRoot "lib/architecture-validation.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-execution-surface.ps1")

$script:ClearraAllowedTasks = @(
    "Quick",
    "All",
    "COnly",
    "COnlySplit",
    "COnlyAsan",
    "COnlyUbsan",
    "UXSmoke",
    "DesktopHost",
    "WorkerE2E",
    "WorkerE2EStress",
    "WorkerAcceptance",
    "WorkerRelease",
    "ProductE2E",
    "ProductE2EBuilt",
    "Acceptance",
    "ReleaseAcceptance",
    "NoProductDebt",
    "AdversarialCorrectness",
    "CSanitizer",
    "RustExactTests",
    "WasmBuildProducer",
    "WasmBuildTest",
    "RenderGolden",
    "GpuWorkerAcceptance",
    "GpuWorkerNative",
    "GpuWorkerRelease",
    "Mvp2Acceptance",
    "Mvp3Acceptance",
    "Validate",
    "Local",
    "Strict",
    "Security",
    "SecurityFull",
    "NativeLocal",
    "DiagnoseCArtifacts",
    "Events",
    "CollectWindowsBlockEvents"
)

$script:ClearraDeveloperEntrypoints = @(
    "scripts/start-tests.ps1",
    "scripts/lib/architecture-validation.ps1",
    "scripts/diagnose-c-core-test-artifacts.ps1",
    "scripts/collect-windows-block-events.ps1"
)

$ClearraScriptRoot = $PSScriptRoot
. (Join-Path $ClearraScriptRoot "lib/clearra-start-helpers.ps1")
. (Join-Path $ClearraScriptRoot "lib/progress.ps1")
. (Join-Path $ClearraScriptRoot "lib/product-process-surface.ps1")
. (Join-Path $ClearraScriptRoot "lib/gpu-worker-tasks.ps1")
. (Join-Path $ClearraScriptRoot "lib/clearra-task-dispatch.ps1")

if ($Workers -lt 1) {
    throw "-Workers must be at least 1."
}
$Workers = [Math]::Min($Workers, [Math]::Max(1, [Environment]::ProcessorCount))
if ($OutputExcerptLines -lt 1) {
    throw "-OutputExcerptLines must be at least 1."
}
$resolvedRequestedRuntime = Resolve-ClearraRuntimeEnvironment $RuntimeEnvironment
Assert-ClearraRequestedTaskSurfaces `
    $Task `
    $ExecutionSurface `
    $resolvedRequestedRuntime

$Root = Resolve-ClearraRoot
if (-not [string]::IsNullOrWhiteSpace($ReportPath)) {
    $ReportPath = Resolve-ClearraReportPath $ReportPath $Root
}
$previousCargoTargetDir = $env:CARGO_TARGET_DIR
$previousCargoIncremental = $env:CARGO_INCREMENTAL
$previousBuildCacheSessionKey = $env:CLEARRA_BUILD_CACHE_SESSION_KEY
$previousBuildCacheOwnerPid = $env:CLEARRA_BUILD_CACHE_OWNER_PID
$previousExecutionSurface = $env:CLEARRA_EXECUTION_SURFACE
$previousRuntimeEnvironment = $env:CLEARRA_RUNTIME_ENVIRONMENT
$clearraLocationPushed = $false
try {
    $env:CLEARRA_EXECUTION_SURFACE = $ExecutionSurface
    $script:ClearraRuntimeEnvironment = $resolvedRequestedRuntime
    $script:ClearraWslDistribution = $WslDistribution
    $env:CLEARRA_RUNTIME_ENVIRONMENT = $script:ClearraRuntimeEnvironment
    # The execution surface is part of the reusable cache generation.
    Ensure-ClearraBuildArtifactCache
    if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
        $env:CARGO_TARGET_DIR = Get-ClearraCargoTargetDir
    } else {
        Assert-ClearraCanonicalCargoTargetDir $previousCargoTargetDir | Out-Null
    }

    Push-Location $Root
    $clearraLocationPushed = $true
    $tasks = @(Expand-ClearraTasks `
        -RequestedTasks $Task `
        -ReleaseAcceptanceShard $ReleaseAcceptanceShard)
    $script:ClearraReleaseAcceptanceMode = $false
    $script:ClearraReleaseAcceptanceShard = $ReleaseAcceptanceShard
    $script:ClearraNoProductDebtArchitecturePassed = $false
    if ($VerboseLog.IsPresent) {
        Write-Output "==> Clearra task start | task=$($tasks -join ',') | workers=$Workers | root=$Root"
    }

    $progressScopeName = "clearra"
    foreach ($taskValue in $Task) {
        foreach ($rawTaskName in ([string]$taskValue -split ",")) {
            switch ($rawTaskName.Trim().ToLowerInvariant()) {
                "acceptance" { $progressScopeName = "acceptance" }
                "releaseacceptance" {
                    $progressScopeName = if ($ReleaseAcceptanceShard -eq "Full") {
                        "release-acceptance"
                    } else {
                        "release-acceptance-$($ReleaseAcceptanceShard.ToLowerInvariant())"
                    }
                    $script:ClearraReleaseAcceptanceMode = $true
                }
                "gpuworkerrelease" {
                    $progressScopeName = "gpu-worker-release"
                    $script:ClearraReleaseAcceptanceMode = $true
                }
                "workeracceptance" { $progressScopeName = "worker-acceptance" }
                "workerrelease" {
                    $progressScopeName = "worker-release"
                    $script:ClearraReleaseAcceptanceMode = $true
                }
                "desktophost" { $progressScopeName = "desktop-host" }
                "mvp2acceptance" { $progressScopeName = "mvp2-acceptance" }
                "mvp3acceptance" { $progressScopeName = "mvp3-acceptance" }
            }
        }
    }

    $topLevelProgressScope = New-ClearraProgressScope `
        -Name $progressScopeName `
        -Total $tasks.Count `
        -Workers 1 `
        -VerboseLog:$VerboseLog.IsPresent
    foreach ($taskName in $tasks) {
        Invoke-ClearraProgressCase `
            -Scope $topLevelProgressScope `
            -Name $taskName `
            -PreserveOutput `
            -Body { Invoke-ClearraTask $taskName $Root }
    }

    Complete-ClearraProgressLine $topLevelProgressScope
    if ($VerboseLog.IsPresent) {
        Write-Output "==> Clearra task completed | task=$($tasks -join ',')"
    }
} finally {
    if ($clearraLocationPushed) {
        Pop-Location
    }
    Exit-ClearraBuildArtifactCacheUsage
    if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
        Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $previousCargoTargetDir
    }
    if ([string]::IsNullOrWhiteSpace($previousCargoIncremental)) {
        Remove-Item Env:\CARGO_INCREMENTAL -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_INCREMENTAL = $previousCargoIncremental
    }
    if ([string]::IsNullOrWhiteSpace($previousBuildCacheSessionKey)) {
        Remove-Item Env:\CLEARRA_BUILD_CACHE_SESSION_KEY -ErrorAction SilentlyContinue
    } else {
        $env:CLEARRA_BUILD_CACHE_SESSION_KEY = $previousBuildCacheSessionKey
    }
    if ([string]::IsNullOrWhiteSpace($previousBuildCacheOwnerPid)) {
        Remove-Item Env:\CLEARRA_BUILD_CACHE_OWNER_PID -ErrorAction SilentlyContinue
    } else {
        $env:CLEARRA_BUILD_CACHE_OWNER_PID = $previousBuildCacheOwnerPid
    }
    if ([string]::IsNullOrWhiteSpace($previousExecutionSurface)) {
        Remove-Item Env:\CLEARRA_EXECUTION_SURFACE -ErrorAction SilentlyContinue
    } else {
        $env:CLEARRA_EXECUTION_SURFACE = $previousExecutionSurface
    }
    if ([string]::IsNullOrWhiteSpace($previousRuntimeEnvironment)) {
        Remove-Item Env:\CLEARRA_RUNTIME_ENVIRONMENT -ErrorAction SilentlyContinue
    } else {
        $env:CLEARRA_RUNTIME_ENVIRONMENT = $previousRuntimeEnvironment
    }
}
