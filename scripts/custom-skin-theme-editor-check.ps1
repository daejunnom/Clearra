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
    -Name "custom-skin-theme-editor" `
    -Total 3 `
    -Workers ([Math]::Max(1, $Workers)) `
    -VerboseLog:$VerboseLog.IsPresent

Push-Location $Root
try {
    foreach ($crate in @(
            "clearra-render",
            "clearra-ui-schema"
        )) {
        Invoke-CheckedCommand `
            -Label "cargo check -p $crate --tests" `
            -FileName $CargoPath `
            -Arguments @("check", "-p", $crate, "--tests")
    }

    Invoke-CheckedCommand `
        -Label "architecture G10 Custom Skin Theme Editor" `
        -FileName "powershell" `
        -Arguments @(
            "-NoProfile",
            "-File", "scripts/validate_architecture.ps1",
            "-TaskName", "G10 Custom Skin Theme Editor"
        )

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[custom-skin-theme-editor] passed | execution_surface=compile-rust-ui-schema-architecture-only | test_executable_launched=false"
}
finally {
    Pop-Location
}
