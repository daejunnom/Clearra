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
    -Name "ppc-cli" `
    -Total 5 `
    -Workers ([Math]::Max(1, $Workers)) `
    -VerboseLog:$VerboseLog.IsPresent

Push-Location $Root
try {
    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-core-executor --tests" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-core-executor", "--tests")

    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-app --tests" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-app", "--tests")

    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-cli --tests" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-cli", "--tests")

    Invoke-CheckedCommand `
        -Label "cargo check -p clearra-build-coverage --tests" `
        -FileName $CargoPath `
        -Arguments @("check", "-p", "clearra-build-coverage", "--tests")

    Invoke-CheckedCommand `
        -Label "architecture X6 Path Percent Cover CLI" `
        -FileName "powershell" `
        -Arguments @(
            "-NoProfile",
            "-File", "scripts/validate_architecture.ps1",
            "-TaskName", "X6 Path Percent Cover CLI"
        )

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[ppc-cli] passed | execution_surface=compile-and-architecture-only | test_executable_launched=false"
}
finally {
    Pop-Location
}
