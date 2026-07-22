param(
    [switch]$UseBuiltBinary,
    [string]$ExePath = "",
    [string]$ReportPath = "",
    [int]$OutputExcerptLines = 60,
    [switch]$StaticFixtureOnly,
    [string]$ExecutionSurface = "",
    [switch]$VerboseLog
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
$ProductResults = New-Object System.Collections.Generic.List[object]
if (-not $PSBoundParameters.ContainsKey("ReportPath")) {
    $ReportPath = ""
}
. (Join-Path $PSScriptRoot "lib/progress.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-path-helpers.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-execution-surface.ps1")
if (-not $StaticFixtureOnly.IsPresent) {
    Assert-ClearraTrustedExecutionSurface $ExecutionSurface "process ProductE2E"
}
. (Join-Path $PSScriptRoot "lib/product-e2e-build.ps1")
. (Join-Path $PSScriptRoot "lib/product-e2e-typed-assertions.ps1")
. (Join-Path $PSScriptRoot "lib/product-e2e-assertions.ps1")
. (Join-Path $PSScriptRoot "lib/product-e2e-t4-golden-cases.ps1")
. (Join-Path $PSScriptRoot "lib/product-e2e-run.ps1")
. (Join-Path $PSScriptRoot "lib/product-e2e-report.ps1")

if (-not [string]::IsNullOrWhiteSpace($ReportPath)) {
    $ReportPath = Resolve-ClearraReportPath $ReportPath $Root
}

if ($OutputExcerptLines -lt 1) {
    throw "-OutputExcerptLines must be at least 1."
}

Remove-StaleProductE2EClearraCliBinary

$script:ProductE2ECurrentCaseName = ""
$productE2ETotal = if ($StaticFixtureOnly.IsPresent) { 2 } else { 16 }
$script:ProductE2EProgressScope = New-ClearraProgressScope `
    -Name "product-e2e" `
    -Total $productE2ETotal `
    -Workers 1 `
    -VerboseLog:$VerboseLog.IsPresent

try {
    if (-not $StaticFixtureOnly.IsPresent) {
        Invoke-ProductE2ECommandCase `
            -Name "opening 2L product pipeline" `
            -FixturePath "tests/fixtures/pc/opening_2l_empty.json" `
            -GoldenPath "tests/golden/pc/opening_2l_empty.json" `
            -CommandArgs (Get-FixtureCommandArgs "tests/fixtures/pc/opening_2l_empty.json")

        Invoke-ProductE2ECommandCase `
            -Name "scenario 4L simple product pipeline" `
            -FixturePath "tests/fixtures/pc/scenario_simple_4l.json" `
            -GoldenPath "tests/golden/pc/scenario_simple_4l.json" `
            -CommandArgs @("--format", "json", "pc-scenario", "--fixture", "tests/fixtures/pc/scenario_simple_4l.json", "--verify-expected")

        Invoke-ProductE2ET4GoldenCases

        Invoke-ProductE2ECommandCase `
            -Name "unsupported 180 scenario reports capability reason" `
            -FixturePath "tests/fixtures/pc/requires_180_unsupported.json" `
            -GoldenPath "tests/golden/pc/requires_180_unsupported.json" `
            -CommandArgs @("--format", "json", "pc-scenario", "--fixture", "tests/fixtures/pc/requires_180_unsupported.json", "--verify-expected")

        Invoke-ProductE2ECommandCase `
            -Name "post-PC continuation token contract" `
            -FixturePath "tests/fixtures/continuation/pc_then_next_pc_available.json" `
            -GoldenPath "tests/golden/continuation/next_pc_available.json" `
            -CommandArgs (Get-FixtureCommandArgs "tests/fixtures/continuation/pc_then_next_pc_available.json")

        Invoke-ProductE2EOpening2LBackendEquivalenceCase

        Invoke-ProductE2EBackendCapabilityReportCase

        Invoke-ProductE2EScenario4LBackendEquivalenceCase

        Invoke-ProductE2EGpuNoFallbackUnavailableCase

        Invoke-ProductE2EGpuAllowFallbackReasonCase

        Invoke-ProductE2EGpuBackendTrustStateCase
    }

    Invoke-ProductE2EFixtureCase `
        -Name "coverage overlap uses PatternBitSet union" `
        -FixturePath "tests/fixtures/coverage/overlap_two_variants_one_pattern.json" `
        -GoldenPath "tests/golden/coverage/overlap_union_probability.json"

    Invoke-ProductE2EFixtureCase `
        -Name "setup family probability uses PatternBitSet union" `
        -FixturePath "tests/fixtures/setup/simple_family_union.json" `
        -GoldenPath "tests/golden/setup/simple_family_probability.json"
} finally {
    Write-ProductE2EReport
}

Complete-ClearraProgressLine $script:ProductE2EProgressScope
[Console]::Out.WriteLine("[product-e2e] all product cases passed | cases=$($ProductResults.Count)")
if ($StaticFixtureOnly.IsPresent) {
    [Console]::Out.WriteLine("[product-e2e] fixture evidence | product_e2e_route=static-fixture-contract | process-launch=False | execution_complete=false")
} else {
    [Console]::Out.WriteLine("[product-e2e] gate summary | product_e2e_route=process | process-launch=True")
}
