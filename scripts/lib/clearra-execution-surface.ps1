if ($null -eq (Get-Command Assert-ClearraWindowsGeneratedExecutionAllowed -ErrorAction SilentlyContinue)) {
    . (Join-Path $PSScriptRoot 'clearra-application-control.ps1')
}

function Resolve-ClearraExecutionSurface([AllowNull()][string]$ExecutionSurface) {
    $candidate = if (-not [string]::IsNullOrWhiteSpace($ExecutionSurface)) {
        $ExecutionSurface
    } elseif (-not [string]::IsNullOrWhiteSpace($env:CLEARRA_EXECUTION_SURFACE)) {
        $env:CLEARRA_EXECUTION_SURFACE
    } else {
        "ManagedLocal"
    }

    if ($candidate -notin @("ManagedLocal", "Trusted")) {
        throw "Unknown Clearra execution surface '$candidate'."
    }
    return $candidate
}

function Test-ClearraTrustedExecutionSurface([AllowNull()][string]$ExecutionSurface) {
    return (Resolve-ClearraExecutionSurface $ExecutionSurface) -eq "Trusted"
}

function Assert-ClearraTrustedExecutionSurface(
    [string]$ExecutionSurface,
    [string]$TaskName,
    [AllowNull()][string]$RuntimeEnvironment = $null
) {
    $resolvedSurface = Resolve-ClearraExecutionSurface $ExecutionSurface
    if ($resolvedSurface -eq "Trusted") {
        $runtime = if (-not [string]::IsNullOrWhiteSpace($RuntimeEnvironment)) {
            $RuntimeEnvironment.Trim().ToLowerInvariant()
        } elseif ([string]::IsNullOrWhiteSpace($env:CLEARRA_RUNTIME_ENVIRONMENT)) {
            'windows'
        } else {
            $env:CLEARRA_RUNTIME_ENVIRONMENT.Trim().ToLowerInvariant()
        }
        if ($runtime -notin @('windows', 'wsl', 'wasm')) {
            throw "Unknown Clearra runtime environment '$runtime'."
        }
        if ((Test-StartTestsWindows) -and $runtime -eq 'windows') {
            Assert-ClearraWindowsGeneratedExecutionAllowed $TaskName | Out-Null
        }
        $env:CLEARRA_EXECUTION_SURFACE = $resolvedSurface
        return
    }

    throw "$TaskName requires -ExecutionSurface Trusted through scripts/clearra.ps1. No generated executable was launched."
}

function Assert-ClearraRequestedTaskSurfaces(
    [string[]]$RequestedTasks,
    [string]$ExecutionSurface,
    [AllowNull()][string]$RuntimeEnvironment = $null
) {
    if (Test-ClearraTrustedExecutionSurface $ExecutionSurface) {
        return
    }

    $trustedOnly = @(
        "Acceptance",
        "ReleaseAcceptance",
        "Strict",
        "UXSmoke",
        "DesktopHost",
        "NoProductDebt",
        "AdversarialCorrectness",
        "CSanitizer",
        "RustExactTests",
        "WasmBuildTest",
        "RenderGolden",
        "WorkerE2E",
        "WorkerE2EStress",
        "ProductE2EBuilt",
        "WorkerAcceptance",
        "WorkerRelease",
        "GpuWorkerAcceptance",
        "GpuWorkerNative",
        "GpuWorkerRelease",
        "Mvp2Acceptance",
        "Mvp3Acceptance",
        "SecurityFull"
    )

    foreach ($taskValue in $RequestedTasks) {
        foreach ($rawTaskName in ([string]$taskValue -split ",")) {
            $taskName = $rawTaskName.Trim()
            if ($taskName -in $trustedOnly) {
                Assert-ClearraTrustedExecutionSurface `
                    $ExecutionSurface `
                    $taskName `
                    $RuntimeEnvironment
            }
        }
    }
}
