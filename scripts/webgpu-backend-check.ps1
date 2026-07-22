param(
    [string]$CargoPath = "cargo",
    [int]$Workers = 1,
    [string]$ExecutionSurface = "",
    [switch]$VerboseLog
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
. (Join-Path $PSScriptRoot "lib/progress.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-execution-surface.ps1")
Assert-ClearraTrustedExecutionSurface $ExecutionSurface "WebGPU backend check"

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory)]
        [string]$Label,

        [Parameter(Mandatory)]
        [string]$FileName,

        [Parameter(Mandatory)]
        [string[]]$Arguments
    )

    Invoke-ClearraProgressCase `
        -Scope $script:Scope `
        -Name $Label `
        -Body {
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
                throw "$Label failed with exit $($result.ExitCode)`n$result.Output"
            }
        }
}

$script:Scope = New-ClearraProgressScope `
    -Name "webgpu" `
    -Total 4 `
    -Workers ([Math]::Max(1, $Workers)) `
    -VerboseLog:$VerboseLog.IsPresent

Push-Location $Root
try {
    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-wasm" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-wasm")

    Invoke-CheckedCommand `
        -Label "cargo test clearra-wasm host contract" `
        -FileName $CargoPath `
        -Arguments @("test", "-p", "clearra-wasm", "--test", "wasm_host_contract")

    Invoke-CheckedCommand `
        -Label "cargo test clearra-webgpu real batch" `
        -FileName $CargoPath `
        -Arguments @(
            "test", "-p", "clearra-webgpu",
            "tests::webgpu_backend_runs_real_batch", "--", "--exact"
        )

    Invoke-CheckedCommand `
        -Label "architecture U8 WebGPU Backend" `
        -FileName "powershell" `
        -Arguments @(
            "-NoProfile",
            "-File", "scripts/validate_architecture.ps1",
            "-TaskName", "U8 WebGPU Backend"
        )

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[webgpu] passed | real_batch=passed | wasm_host_contract=passed | architecture_validation=passed"
}
finally {
    Pop-Location
}
