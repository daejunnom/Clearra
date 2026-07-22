param(
    [string]$CargoPath = "cargo",
    [int]$Workers = 1,
    [int]$OutputExcerptLines = 60,
    [switch]$StaticProductContractOnly,
    [string]$ExecutionSurface = "",
    [switch]$VerboseLog,
    [switch]$ShowWarnings
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
. (Join-Path $PSScriptRoot "lib/progress.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-execution-surface.ps1")
Assert-ClearraTrustedExecutionSurface $ExecutionSurface "MVP3 acceptance"

function Invoke-Mvp3AcceptanceStep {
    param(
        [Parameter(Mandatory)]
        [string]$Name,

        [Parameter(Mandatory)]
        [string]$ScriptPath,

        [string[]]$Arguments = @()
    )

    Invoke-ClearraProgressCase `
        -Scope $script:Scope `
        -Name $Name `
        -Body {
            $result = Invoke-NativeWithProgress `
                -Scope $script:Scope `
                -Label $Name `
                -FileName "powershell" `
                -Arguments (@(
                        "-NoProfile",
                        "-File", $ScriptPath
                    ) + $Arguments)

            if ($VerboseLog.IsPresent -and -not [string]::IsNullOrWhiteSpace($result.Output)) {
                Complete-ClearraProgressLine $script:Scope
                Write-Output $result.Output
            }

            if ($result.ExitCode -ne 0) {
                throw "$Name failed with exit $($result.ExitCode)`n$($result.Output)"
            }
        }
}

$script:Scope = New-ClearraProgressScope `
    -Name "mvp3-acceptance" `
    -Total 14 `
    -Workers 1 `
    -VerboseLog:$VerboseLog.IsPresent

Push-Location $Root
try {
    # standard_fast_path_unchanged_under_mvp3
    # MVP3 additions must not alter MVP1 PC/product health.
    $productArgs = @("-OutputExcerptLines", [string]$OutputExcerptLines)
    if ($StaticProductContractOnly.IsPresent) {
        $productArgs += "-StaticFixtureOnly"
    }
    if ($VerboseLog.IsPresent) {
        $productArgs += "-VerboseLog"
    }
    Invoke-Mvp3AcceptanceStep `
        -Name "MVP1 ProductE2E" `
        -ScriptPath "scripts/product-e2e.ps1" `
        -Arguments $productArgs

    $mvp2Args = @(
        "-CargoPath", $CargoPath,
        "-Workers", [string]$Workers,
        "-OutputExcerptLines", [string]$OutputExcerptLines
    )
    if ($StaticProductContractOnly.IsPresent) {
        $mvp2Args += "-StaticProductContractOnly"
    }
    if ($ShowWarnings.IsPresent -or $VerboseLog.IsPresent) {
        $mvp2Args += "-ShowWarnings"
    }
    if ($VerboseLog.IsPresent) {
        $mvp2Args += "-VerboseLog"
    }
    Invoke-Mvp3AcceptanceStep `
        -Name "MVP2 Acceptance" `
        -ScriptPath "scripts/mvp2-acceptance.ps1" `
        -Arguments $mvp2Args

    $mvp3GuardSteps = @(
        [pscustomobject]@{ Name = "MVP3 Scope Gate"; ScriptPath = "scripts/mvp3-scope-gate-check.ps1" }
        [pscustomobject]@{ Name = "Custom piece schema tests"; ScriptPath = "scripts/custom-piece-domain-check.ps1" }
        [pscustomobject]@{ Name = "Mixed bag schema tests"; ScriptPath = "scripts/mixed-supply-generalization-check.ps1" }
        [pscustomobject]@{ Name = "Board128/Wide descriptor tests"; ScriptPath = "scripts/board128-wide-runtime-check.ps1" }
        [pscustomobject]@{ Name = "Generic operation guard tests"; ScriptPath = "scripts/generic-operation-candidate-reachability-check.ps1" }
        [pscustomobject]@{ Name = "Area multiset feasibility tests"; ScriptPath = "scripts/area-multiset-feasibility-check.ps1" }
        [pscustomobject]@{ Name = "DLX tests"; ScriptPath = "scripts/generic-exact-cover-dlx-check.ps1" }
        [pscustomobject]@{ Name = "Unsupported runtime guard tests"; ScriptPath = "scripts/generic-buildup-check.ps1" }
        [pscustomobject]@{ Name = "Custom rule editor validation tests"; ScriptPath = "scripts/custom-rule-editor-check.ps1" }
        [pscustomobject]@{ Name = "Generic GPU descriptor tests"; ScriptPath = "scripts/generic-gpu-descriptor-check.ps1" }
        [pscustomobject]@{ Name = "Custom skin/theme editor tests"; ScriptPath = "scripts/custom-skin-theme-editor-check.ps1" }
    )

    foreach ($step in $mvp3GuardSteps) {
        Invoke-Mvp3AcceptanceStep `
            -Name $step.Name `
            -ScriptPath $step.ScriptPath `
            -Arguments @("-CargoPath", $CargoPath, "-Workers", [string]$Workers)
    }

    $architectureArgs = @("-Workers", [string]$Workers)
    if ($ShowWarnings.IsPresent -or $VerboseLog.IsPresent) {
        $architectureArgs += "-ShowWarnings"
    }
    Invoke-Mvp3AcceptanceStep `
        -Name "Architecture validation" `
        -ScriptPath "scripts/validate_architecture.ps1" `
        -Arguments $architectureArgs

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[mvp3-acceptance] passed | standard_fast_path_unchanged_under_mvp3=true | custom_features_guarded_until_runtime_connected=true | no_silent_fallback_to_standard_path=true | generic_cache_keys_include_piece_board_rule_supply_identity=true"
}
finally {
    Pop-Location
}
