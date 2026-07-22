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
    -Name "generic-buildup" `
    -Total 4 `
    -Workers ([Math]::Max(1, $Workers)) `
    -VerboseLog:$VerboseLog.IsPresent

Push-Location $Root
try {
    foreach ($crate in @(
            "clearra-core-ffi",
            "clearra-core-executor"
        )) {
        Invoke-CheckedCommand `
            -Label "cargo check -p $crate --tests" `
            -FileName $CargoPath `
            -Arguments @("check", "-p", $crate, "--tests")
    }

    Invoke-CheckedCommand `
        -Label "core-c COnlySplit build-only" `
        -FileName "powershell" `
        -Arguments @(
            "-NoProfile",
            "-File", "scripts/clearra.ps1",
            "-Task", "COnlySplit",
            "-Workers", "$Workers"
        )

    Invoke-CheckedCommand `
        -Label "architecture G7 Generic BuildUp" `
        -FileName "powershell" `
        -Arguments @(
            "-NoProfile",
            "-File", "scripts/validate_architecture.ps1",
            "-TaskName", "G7 Generic BuildUp"
        )

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[generic-buildup] passed | execution_surface=compile-c-and-rust-architecture-only | test_executable_launched=false"
}
finally {
    Pop-Location
}
