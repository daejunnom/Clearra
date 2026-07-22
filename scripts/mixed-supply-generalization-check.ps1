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
    -Name "mixed-supply" `
    -Total 5 `
    -Workers ([Math]::Max(1, $Workers)) `
    -VerboseLog:$VerboseLog.IsPresent

Push-Location $Root
try {
    foreach ($crate in @(
            "clearra-supply",
            "clearra-profiles",
            "clearra-validation",
            "clearra-core-ffi"
        )) {
        Invoke-CheckedCommand `
            -Label "cargo check -p $crate --tests" `
            -FileName $CargoPath `
            -Arguments @("check", "-p", $crate, "--tests")
    }

    Invoke-CheckedCommand `
        -Label "architecture G2 Mixed Supply Generalization" `
        -FileName "powershell" `
        -Arguments @(
            "-NoProfile",
            "-File", "scripts/validate_architecture.ps1",
            "-TaskName", "G2 Mixed Supply Generalization"
        )

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[mixed-supply] passed | execution_surface=compile-and-architecture-only | test_executable_launched=false"
}
finally {
    Pop-Location
}
