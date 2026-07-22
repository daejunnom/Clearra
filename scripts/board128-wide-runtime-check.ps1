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
    -Name "board-backend" `
    -Total 5 `
    -Workers ([Math]::Max(1, $Workers)) `
    -VerboseLog:$VerboseLog.IsPresent

Push-Location $Root
try {
    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-geometry --tests" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-geometry", "--tests")

    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-core-ffi --tests" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-core-ffi", "--tests")

    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-validation --tests" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-validation", "--tests")

    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-invariant-tests --tests" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-invariant-tests", "--tests")

    Invoke-CheckedCommand `
        -Label "architecture G3 Board128 Wide Runtime" `
        -FileName "powershell" `
        -Arguments @(
            "-NoProfile",
            "-File", "scripts/validate_architecture.ps1",
            "-TaskName", "G3 Board128 Wide Runtime"
        )

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[board-backend] passed | execution_surface=compile-and-architecture-only | test_executable_launched=false"
}
finally {
    Pop-Location
}
