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
Assert-ClearraTrustedExecutionSurface $ExecutionSurface "MVP2 acceptance"

function Invoke-Mvp2AcceptanceStep {
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
    -Name "mvp2-acceptance" `
    -Total 10 `
    -Workers 1 `
    -VerboseLog:$VerboseLog.IsPresent

Push-Location $Root
try {
    # mvp2_acceptance_runs_mvp1_product_e2e_first
    # MVP2 feature failure must not break MVP1 pc/path/percent product health.
    $productArgs = @("-OutputExcerptLines", [string]$OutputExcerptLines)
    if ($StaticProductContractOnly.IsPresent) {
        $productArgs += "-StaticFixtureOnly"
    }
    if ($VerboseLog.IsPresent) {
        $productArgs += "-VerboseLog"
    }
    Invoke-Mvp2AcceptanceStep `
        -Name "MVP1 ProductE2E" `
        -ScriptPath "scripts/product-e2e.ps1" `
        -Arguments $productArgs

    Invoke-Mvp2AcceptanceStep `
        -Name "MVP2 Rule/Kick tests" `
        -ScriptPath "scripts/rule-kick-expansion-check.ps1" `
        -Arguments @("-CargoPath", $CargoPath, "-Workers", [string]$Workers)

    Invoke-Mvp2AcceptanceStep `
        -Name "MVP2 Scoring tests" `
        -ScriptPath "scripts/score-profile-object-check.ps1" `
        -Arguments @("-CargoPath", $CargoPath, "-Workers", [string]$Workers)

    Invoke-Mvp2AcceptanceStep `
        -Name "MVP2 Score objective tests" `
        -ScriptPath "scripts/score-aware-objective-check.ps1" `
        -Arguments @("-CargoPath", $CargoPath, "-Workers", [string]$Workers)

    Invoke-Mvp2AcceptanceStep `
        -Name "SpinTarget coverage tests" `
        -ScriptPath "scripts/spin-target-contract-check.ps1" `
        -Arguments @("-CargoPath", $CargoPath, "-Workers", [string]$Workers)

    Invoke-Mvp2AcceptanceStep `
        -Name "Setup raw metrics tests" `
        -ScriptPath "scripts/setup-raw-metrics-v2-check.ps1" `
        -Arguments @("-CargoPath", $CargoPath, "-Workers", [string]$Workers)

    Invoke-Mvp2AcceptanceStep `
        -Name "Render/Fumen transform tests" `
        -ScriptPath "scripts/fumen-render-product-check.ps1" `
        -Arguments @("-CargoPath", $CargoPath, "-Workers", [string]$Workers)

    Invoke-Mvp2AcceptanceStep `
        -Name "GPU portable/reference tests" `
        -ScriptPath "scripts/gpu-packing-strengthening-check.ps1" `
        -Arguments @("-CargoPath", $CargoPath, "-Workers", [string]$Workers)

    Invoke-Mvp2AcceptanceStep `
        -Name "GUI schema tests" `
        -ScriptPath "scripts/gui-editor-schema-v2-check.ps1" `
        -Arguments @("-CargoPath", $CargoPath, "-Workers", [string]$Workers)

    $architectureArgs = @("-Workers", [string]$Workers)
    if ($ShowWarnings.IsPresent -or $VerboseLog.IsPresent) {
        $architectureArgs += "-ShowWarnings"
    }
    Invoke-Mvp2AcceptanceStep `
        -Name "Architecture validation" `
        -ScriptPath "scripts/validate_architecture.ps1" `
        -Arguments $architectureArgs

    Complete-ClearraProgressLine $script:Scope
    Write-Output "[mvp2-acceptance] passed | mvp1_product_e2e_first=true | mvp2_exact_claims_guarded=true | mvp2_scoring_basic_approximation_disclosed=true | mvp2_renderer_exact_only_when_supported=true | mvp2_gpu_fallback_reason_visible=true"
}
finally {
    Pop-Location
}
