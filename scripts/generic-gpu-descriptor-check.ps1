param(
    [string]$CargoPath = "cargo",
    [int]$Workers = 1,
    [switch]$VerboseLog
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$ScriptRoot = Split-Path -Parent $PSCommandPath
$Root = Resolve-Path -LiteralPath (Join-Path $ScriptRoot "..")
. (Join-Path $ScriptRoot "lib/progress.ps1")

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
    -Name "generic-gpu-unsupported" `
    -Total 3 `
    -Workers ([Math]::Max(1, $Workers)) `
    -VerboseLog:$VerboseLog.IsPresent

Push-Location $Root
try {
    Invoke-CheckedCommand `
        -Label "cargo check default GPU surfaces" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-core-ffi", "-p", "clearra-webgpu")

    Invoke-CheckedCommand `
        -Label "core-c COnlySplit" `
        -FileName "powershell" `
        -Arguments @(
            "-NoProfile",
            "-File", "scripts/clearra.ps1",
            "-Task", "COnlySplit",
            "-Workers", "$Workers"
        )

    Invoke-CheckedCommand `
        -Label "architecture G9 Generic GPU Unsupported" `
        -FileName "powershell" `
        -Arguments @(
            "-NoProfile",
            "-File", "scripts/validate_architecture.ps1",
            "-TaskName", "G9 Generic GPU Unsupported"
        )

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[generic-gpu-unsupported] passed | default_runtime=absent | capability=unsupported"
}
finally {
    Pop-Location
}
