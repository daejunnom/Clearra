param(
    [string]$CargoPath = "cargo",
    [string]$NodePath = "node",
    [string]$PowerShellPath = "powershell",
    [int]$Workers = 1,
    [string]$ExecutionSurface = "",
    [switch]$VerboseLog
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
$ClearraScriptRoot = $PSScriptRoot
. (Join-Path $PSScriptRoot "lib/progress.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-execution-surface.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-start-helpers.ps1")
Assert-ClearraTrustedExecutionSurface $ExecutionSurface "desktop host"

function Invoke-DesktopHostCommand {
    param(
        [Parameter(Mandatory)]
        [string]$Label,

        [Parameter(Mandatory)]
        [string]$FileName,

        [Parameter(Mandatory)]
        [string[]]$Arguments
    )

    Invoke-ClearraProgressCase -Scope $script:Scope -Name $Label -Body {
        $result = Invoke-NativeWithProgress `
            -Scope $script:Scope `
            -Label $Label `
            -FileName $FileName `
            -Arguments $Arguments
        if ($VerboseLog.IsPresent -and -not [string]::IsNullOrWhiteSpace($result.Output)) {
            Complete-ClearraProgressLine $script:Scope
            Write-Output $result.Output
        }
        if ($result.ExitCode -ne 0) {
            if (Test-ClearraApplicationControlBlockOutput $result.Output) {
                throw (New-ClearraLocalSourceBuildBlockedMessage $Label $script:ApplicationControl)
            }
            throw "$Label failed with exit $($result.ExitCode)`n$($result.Output)"
        }
    }
}

$applicationControl = Get-ClearraApplicationControlStatus
$script:ApplicationControl = $applicationControl
$stepCount = 5
$script:Scope = New-ClearraProgressScope `
    -Name "desktop-host" `
    -Total $stepCount `
    -Workers ([Math]::Max(1, $Workers)) `
    -VerboseLog:$VerboseLog.IsPresent

Invoke-ClearraProgressCase -Scope $script:Scope -Name "application control preflight" -Body {
    $blockEvidence = Get-ClearraRecentGeneratedExecutableBlockEvidence
    Write-Output (
        "[desktop-host] application-control | query={0} | ci={1} | umci={2} | local_source_build_policy={3} | evidence_only=true" -f `
            $applicationControl.query_status,
            $applicationControl.code_integrity_policy,
            $applicationControl.user_mode_code_integrity_policy,
            $applicationControl.local_source_build_policy
    )
    Write-Output (
        "[desktop-host] block-evidence | query={0} | matching_generated_artifact_events={1} | latest_event_id={2} | latest_event_time_utc={3}" -f `
            $blockEvidence.query_status,
            $blockEvidence.matched_event_count,
            $blockEvidence.latest_event_id,
            $blockEvidence.latest_event_time_utc
    )
}

Invoke-DesktopHostCommand `
    -Label "compile desktop UI in memory" `
    -FileName $NodePath `
    -Arguments @((Join-Path $Root "scripts/desktop-ui-compile-check.mjs"))

Invoke-DesktopHostCommand `
    -Label "architecture U6 Tauri Desktop Host" `
    -FileName $PowerShellPath `
    -Arguments @(
        "-NoProfile",
        "-File", (Join-Path $Root "scripts/validate_architecture.ps1"),
        "-TaskName", "U6 Tauri Svelte Desktop Host"
    )

$previousCargoTargetDir = $env:CARGO_TARGET_DIR
Push-Location $Root
try {
    if (-not [string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
        Assert-ClearraCanonicalCargoTargetDir $previousCargoTargetDir | Out-Null
    }
    $env:CARGO_TARGET_DIR = Get-ClearraCargoTargetDir

    Invoke-DesktopHostCommand `
        -Label "WASM CPU GUI host async AppRequest E2E" `
        -FileName $CargoPath `
        -Arguments @(
            "test", "-p", "clearra-gui-host",
            "--features", "wasm-cpu-runtime,webgpu-search",
            "--lib",
            "--", "--test-threads=1"
        )

    Invoke-DesktopHostCommand `
        -Label "cargo check Tauri desktop" `
        -FileName $CargoPath `
        -Arguments @(
            "check",
            "--manifest-path", "apps/clearra-desktop/src-tauri/Cargo.toml"
        )

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[desktop-host] passed | product=apps/clearra-desktop | tauri=compiled | wasm_cpu_app_request=executed | async_job_e2e=executed | frontend_source=compiled-in-memory | wsl_used=false"
}
finally {
    if ([string]::IsNullOrWhiteSpace($previousCargoTargetDir)) {
        Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue
    } else {
        $env:CARGO_TARGET_DIR = $previousCargoTargetDir
    }
    Pop-Location
}
