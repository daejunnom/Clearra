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

$script:ReleaseAcceptanceShardTestMarkers = @{
    'full-local-order' = 'release_acceptance_shard_test=full-local-order status=passed'
    'foundation-order' = 'release_acceptance_shard_test=foundation-order status=passed'
    'foundation-no-product-debt-leaf' = 'release_acceptance_shard_test=foundation-no-product-debt-leaf status=passed'
    'foundation-adversarial-correctness-leaf' = 'release_acceptance_shard_test=foundation-adversarial-correctness-leaf status=passed'
    'foundation-desktop-host-leaf' = 'release_acceptance_shard_test=foundation-desktop-host-leaf status=passed'
    'sanitizer-order' = 'release_acceptance_shard_test=sanitizer-order status=passed'
    'rust-order' = 'release_acceptance_shard_test=rust-order status=passed'
    'pages-order' = 'release_acceptance_shard_test=pages-order status=passed'
    'shard-union-equals-full' = 'release_acceptance_shard_test=shard-union-equals-full status=passed'
    'rust-package-inventory' = 'release_acceptance_shard_test=rust-package-inventory status=passed'
}

function Assert-Sequence {
    param(
        [string]$Name,
        [string[]]$Actual,
        [string[]]$Expected
    )
    if (($Actual -join '|') -ne ($Expected -join '|')) {
        throw "$Name sequence differs. actual=$($Actual -join ',') expected=$($Expected -join ',')"
    }
    if (-not $script:ReleaseAcceptanceShardTestMarkers.ContainsKey($Name)) {
        throw "Unknown ReleaseAcceptance shard regression '$Name'."
    }
    Write-Output $script:ReleaseAcceptanceShardTestMarkers[$Name]
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
Assert-Sequence 'foundation-no-product-debt-leaf' `
    @(Expand-ClearraTasks -RequestedTasks @('ReleaseAcceptance') -ReleaseAcceptanceShard FoundationNoProductDebt) `
    @('NoProductDebt')
Assert-Sequence 'foundation-adversarial-correctness-leaf' `
    @(Expand-ClearraTasks -RequestedTasks @('ReleaseAcceptance') -ReleaseAcceptanceShard FoundationAdversarialCorrectness) `
    @('AdversarialCorrectness')
Assert-Sequence 'foundation-desktop-host-leaf' `
    @(Expand-ClearraTasks -RequestedTasks @('ReleaseAcceptance') -ReleaseAcceptanceShard FoundationDesktopHost) `
    @('DesktopHost')
Assert-Sequence 'sanitizer-order' `
    @(Expand-ClearraTasks -RequestedTasks @('ReleaseAcceptance') -ReleaseAcceptanceShard Sanitizer) `
    $sanitizer
Assert-Sequence 'rust-order' `
    @(Expand-ClearraTasks -RequestedTasks @('releaseacceptance') -ReleaseAcceptanceShard Rust) `
    $rust
Assert-Sequence 'pages-order' `
    @(Expand-ClearraTasks -RequestedTasks @('ReleaseAcceptance') -ReleaseAcceptanceShard Pages) `
    $pages

$foundationLeaves = @('NoProductDebt', 'AdversarialCorrectness', 'DesktopHost')
$shardedStages = @($foundationLeaves + $sanitizer + $rust + $pages | Sort-Object)
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

$continueIndex = $rustExact.IndexOf('$arguments.Add(''--no-fail-fast'')', [System.StringComparison]::Ordinal)
$compileIndex = $rustExact.IndexOf('$arguments.Add(''--no-run'')', [System.StringComparison]::Ordinal)
$partitionFailureIndex = $rustExact.IndexOf(
    'Rust exact tests failed after collecting every partition:',
    [System.StringComparison]::Ordinal
)
if ($compileIndex -lt 0 -or $continueIndex -le $compileIndex -or $partitionFailureIndex -lt 0) {
    throw 'RustExactTests must compile all packages together and collect every executable partition failure.'
}
if ($rustExact.IndexOf('rust_exact_phase=global-resource', [System.StringComparison]::Ordinal) -lt 0 -or
    $rustExact.IndexOf('rust_exact_phase=parallel-safe', [System.StringComparison]::Ordinal) -lt 0 -or
    $rustExact.IndexOf('[Math]::Min(2, [Math]::Max(1, $Workers))', [System.StringComparison]::Ordinal) -lt 0) {
    throw 'RustExactTests must run the global-resource partition first and cap the remaining pool at two threads.'
}

. (Join-Path $repositoryRoot 'scripts/lib/rust-exact-tests.ps1')
$rustPackageSpecs = @(Get-RustExactPackageSpecs)
$expectedRustPackages = @(
    'clearra-app',
    'clearra-core-executor',
    'clearra-core-ffi',
    'clearra-webgpu',
    'clearra-core-domain',
    'clearra-coverage',
    'clearra-objectives',
    'clearra-scoring',
    'clearra-postprocess'
)
Assert-Sequence `
    -Name 'rust-package-inventory' `
    -Actual @($rustPackageSpecs.Package | Sort-Object) `
    -Expected @($expectedRustPackages | Sort-Object)
$serialWholePackages = @($rustPackageSpecs | Where-Object SerialWholePackage | ForEach-Object Package)
$serialWholePackageKey = ($serialWholePackages | Sort-Object) -join '|'
$expectedSerialWholePackageKey = (@('clearra-core-ffi', 'clearra-webgpu') | Sort-Object) -join '|'
if ($serialWholePackageKey -ne $expectedSerialWholePackageKey) {
    throw 'Only native FFI and WebGPU packages may use the whole-package global-resource partition.'
}
foreach ($package in @('clearra-app', 'clearra-core-executor')) {
    $spec = $rustPackageSpecs | Where-Object Package -eq $package
    if ($null -eq $spec -or @($spec.GlobalResourceFilters).Count -lt 1) {
        throw "RustExactTests lost the explicit global-resource filters for '$package'."
    }
}
$searchBackend = Get-Content -LiteralPath (
    Join-Path $repositoryRoot 'crates/clearra-core-executor/src/backend/wasm_cpu_search_backend.rs'
) -Raw
$pureSelectionModuleIndex = $searchBackend.LastIndexOf(
    '#[cfg(all(test, feature = "webgpu-search"))]',
    [System.StringComparison]::Ordinal
)
if ($pureSelectionModuleIndex -lt 0 -or
    $searchBackend.Substring($pureSelectionModuleIndex).Contains('score_resource_test_guard')) {
    throw 'Pure WebGPU workload-selection tests must not reserve the process-global score resource.'
}
Write-Output 'release_acceptance_shard_test=rust-global-resource-first-and-parallel-safe status=passed'

& (Join-Path $PSScriptRoot 'test_independent_gate_sequence.ps1')
