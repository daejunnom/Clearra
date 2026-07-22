param(
    [switch]$UseBuiltBinary,
    [string]$ExePath = "",
    [switch]$Extended,
    [switch]$Stress,
    [switch]$VerboseLog,
    [int]$Workers = 1,
    [int]$OutputExcerptLines = 80,
    [string]$ExecutionSurface = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Root = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot "..")
. (Join-Path $PSScriptRoot "lib/clearra-path-helpers.ps1")
. (Join-Path $PSScriptRoot "lib/progress.ps1")
. (Join-Path $PSScriptRoot "lib/clearra-execution-surface.ps1")
Assert-ClearraTrustedExecutionSurface $ExecutionSurface "worker E2E"
. (Join-Path $PSScriptRoot "lib/worker-e2e-fixture.ps1")
. (Join-Path $PSScriptRoot "lib/worker-e2e-source-registry.ps1")
. (Join-Path $PSScriptRoot "lib/worker-e2e-fumen-normalize.ps1")
. (Join-Path $PSScriptRoot "lib/worker-e2e-json-assertions.ps1")
. (Join-Path $PSScriptRoot "lib/worker-e2e-backend-assertions.ps1")
. (Join-Path $PSScriptRoot "lib/worker-e2e-solution-set-assertions.ps1")
. (Join-Path $PSScriptRoot "lib/worker-e2e-process.ps1")
. (Join-Path $PSScriptRoot "lib/worker-e2e-runner.ps1")

if ($Workers -lt 1) {
    throw "-Workers must be at least 1."
}
if ($OutputExcerptLines -lt 1) {
    throw "-OutputExcerptLines must be at least 1."
}

$script:WorkerE2ERoot = $Root
$script:WorkerE2EUseBuiltBinary = $UseBuiltBinary.IsPresent
$script:WorkerE2EExePath = $ExePath
$script:WorkerE2EWorkers = $Workers
$script:WorkerE2EOutputExcerptLines = $OutputExcerptLines
$script:WorkerE2ECurrentCaseName = ""

Remove-StaleWorkerE2EClearraCliBinary

$cases = New-Object System.Collections.Generic.List[object]
$cases.Add([pscustomobject]@{
    Name = "external PC source registry contract"
    Kind = "source-registry"
})
$cases.Add([pscustomobject]@{
    Name = "PCO I-hold 6p source metadata"
    Kind = "metadata-fixture"
    FixturePath = "tests/fixtures/external-pc/pco_i_hold_6p_second_bag_pc.json"
    GoldenPath = "tests/golden/external-pc/pco_i_hold_6p_second_bag_pc.json"
})
$tsarBackends = if ($Stress.IsPresent -or $Extended.IsPresent) {
    @("cpu", "gpu", "hybrid")
} else {
    @("cpu")
}
$cases.Add([pscustomobject]@{
    Name = "Tsar Cannon after 2-bag source 42 tilings"
    Kind = "fixture-backend"
    FixturePath = "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.json"
    GoldenPath = "tests/golden/external-pc/tsar_cannon_after_2bag_full_42.json"
    Backends = $tsarBackends
})
$cases.Add([pscustomobject]@{
    Name = "external PC fumen-like fixture contract"
    Kind = "fumen-contract"
})

$script:WorkerE2EProgressScope = New-ClearraProgressScope `
    -Name "worker-e2e" `
    -Total $cases.Count `
    -Workers 1 `
    -VerboseLog:$VerboseLog.IsPresent

foreach ($case in $cases) {
    switch ($case.Kind) {
        "source-registry" {
            Invoke-ClearraProgressCase -Scope $script:WorkerE2EProgressScope -Name $case.Name -Body {
                Assert-WorkerE2ESourceRegistryContract -Root $Root
            }
        }
        "fixture-backend" {
            Invoke-WorkerE2EBackendRunCase `
                -Name $case.Name `
                -FixturePath $case.FixturePath `
                -GoldenPath $case.GoldenPath `
                -Backends $case.Backends
        }
        "metadata-fixture" {
            Invoke-WorkerE2EMetadataFixtureCase `
                -Name $case.Name `
                -FixturePath $case.FixturePath `
                -GoldenPath $case.GoldenPath
        }
        "fumen-contract" {
            Invoke-ClearraProgressCase -Scope $script:WorkerE2EProgressScope -Name $case.Name -Body {
                Assert-WorkerE2EExternalPcFumenContracts -Root $Root
            }
        }
    }
}

Complete-ClearraProgressLine $script:WorkerE2EProgressScope
[Console]::Out.WriteLine("[worker-e2e] all external PC worker contracts passed | cases=$($cases.Count)")
