$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')
$script:ClearraAllowedTasks = @(
    'Quick',
    'ReleaseAcceptance',
    'NoProductDebt',
    'AdversarialCorrectness',
    'CSanitizer',
    'RustExactTests',
    'ProductE2E',
    'WasmBuildTest',
    'DesktopHost',
    'RenderGolden'
)
. (Join-Path $PSScriptRoot 'lib/clearra-task-ui-helpers.ps1')

function Assert-Sequence {
    param(
        [string]$Name,
        [string[]]$Actual,
        [string[]]$Expected
    )
    if (($Actual -join '|') -ne ($Expected -join '|')) {
        throw "$Name sequence differs. actual=$($Actual -join ',') expected=$($Expected -join ',')"
    }
    Write-Output "release_acceptance_shard_test=$Name status=passed"
}

$full = @(
    'NoProductDebt',
    'AdversarialCorrectness',
    'CSanitizer',
    'RustExactTests',
    'ProductE2E',
    'WasmBuildTest',
    'DesktopHost',
    'RenderGolden'
)
$foundation = @('NoProductDebt', 'AdversarialCorrectness', 'DesktopHost')
$sanitizer = @('CSanitizer')
$rust = @('RustExactTests', 'ProductE2E', 'RenderGolden')
$pages = @('WasmBuildTest')

Assert-Sequence 'full-local-order' `
    @(Expand-ClearraTasks -RequestedTasks @('ReleaseAcceptance')) `
    $full
Assert-Sequence 'foundation-order' `
    @(Expand-ClearraTasks -RequestedTasks @('ReleaseAcceptance') -ReleaseAcceptanceShard Foundation) `
    $foundation
Assert-Sequence 'sanitizer-order' `
    @(Expand-ClearraTasks -RequestedTasks @('ReleaseAcceptance') -ReleaseAcceptanceShard Sanitizer) `
    $sanitizer
Assert-Sequence 'rust-order' `
    @(Expand-ClearraTasks -RequestedTasks @('releaseacceptance') -ReleaseAcceptanceShard Rust) `
    $rust
Assert-Sequence 'pages-order' `
    @(Expand-ClearraTasks -RequestedTasks @('ReleaseAcceptance') -ReleaseAcceptanceShard Pages) `
    $pages

$shardedStages = @($foundation + $sanitizer + $rust + $pages | Sort-Object)
Assert-Sequence 'shard-union-equals-full' $shardedStages @($full | Sort-Object)

$rejected = $false
try {
    $null = @(Expand-ClearraTasks -RequestedTasks @('Quick') -ReleaseAcceptanceShard Rust)
} catch {
    $rejected = $_.Exception.Message -match 'may only select one ReleaseAcceptance task'
}
if (-not $rejected) {
    throw 'ReleaseAcceptance shard selector accepted a different task.'
}
Write-Output 'release_acceptance_shard_test=selector-scope status=passed'

$noProductDebt = Get-Content -LiteralPath (
    Join-Path $repositoryRoot 'scripts/lib/no-product-debt.ps1'
) -Raw
$adversarial = Get-Content -LiteralPath (
    Join-Path $repositoryRoot 'scripts/lib/adversarial-correctness.ps1'
) -Raw
$desktop = Get-Content -LiteralPath (
    Join-Path $repositoryRoot 'scripts/desktop-host-check.ps1'
) -Raw
$rustExact = Get-Content -LiteralPath (
    Join-Path $repositoryRoot 'scripts/lib/rust-exact-tests.ps1'
) -Raw
$render = Get-Content -LiteralPath (
    Join-Path $repositoryRoot 'scripts/lib/render-golden-gate.ps1'
) -Raw
foreach ($contract in @(
    @($noProductDebt, 'complete_required_keeps_candidate status=deferred owner=RustExactTests'),
    @($noProductDebt, 'renderer_png_artifact status=deferred owner=RenderGolden'),
    @($noProductDebt, 'renderer_gif_artifact status=deferred owner=RenderGolden'),
    @($noProductDebt, 'desktop_real_app_request status=deferred owner=DesktopHost'),
    @($adversarial, 'adversarial_rust_tests=deferred owner=RustExactTests'),
    @($desktop, "-EvidenceId 'desktop_real_app_request'"),
    @($desktop, 'no_product_debt_evidence=$EvidenceId status=passed source=rust-test owner=DesktopHost'),
    @($rustExact, 'complete_required_keeps_candidate status=passed source=rust-test owner=RustExactTests'),
    @($render, 'renderer_png_artifact status=passed source=rust-test owner=RenderGolden'),
    @($render, 'renderer_gif_artifact status=passed source=rust-test owner=RenderGolden')
)) {
    if ($contract[0].IndexOf($contract[1], [System.StringComparison]::Ordinal) -lt 0) {
        throw "ReleaseAcceptance delegated evidence contract is missing '$($contract[1])'."
    }
}
Write-Output 'release_acceptance_shard_test=delegated-evidence-owners status=passed'
