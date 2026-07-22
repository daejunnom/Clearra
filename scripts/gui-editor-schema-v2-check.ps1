param(
    [string]$CargoPath = "cargo",
    [int]$Workers = 1,
    [switch]$VerboseLog
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
. (Join-Path $PSScriptRoot "lib/progress.ps1")

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
    -Name "gui-schema-v2" `
    -Total 4 `
    -Workers ([Math]::Max(1, $Workers)) `
    -VerboseLog:$VerboseLog.IsPresent

Push-Location $Root
try {
    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-ui-schema --tests" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-ui-schema", "--tests")

    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-gui-host --tests" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-gui-host", "--tests")

    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-app --tests" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-app", "--tests")

    Invoke-CheckedCommand `
        -Label "architecture X8 GUI Editor Schema v2" `
        -FileName "powershell" `
        -Arguments @(
            "-NoProfile",
            "-File", "scripts/validate_architecture.ps1",
            "-TaskName", "X8 GUI Editor Schema v2"
        )

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[gui-schema-v2] passed | execution_surface=compile-and-architecture-only | test_executable_launched=false"
}
finally {
    Pop-Location
}
