param(
    [string]$RepositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot 'lib/clearra-path-helpers.ps1')

function Assert-ArtifactPathCondition([bool]$Condition, [string]$CaseName) {
    if (-not $Condition) {
        throw "artifact path policy test failed: $CaseName"
    }
    Write-Output "artifact_path_policy_test=$CaseName status=passed"
}

function Test-ArtifactPathThrows([scriptblock]$Body) {
    try {
        & $Body | Out-Null
        return $false
    } catch {
        return $true
    }
}

$repository = [System.IO.Path]::GetFullPath($RepositoryRoot)
$reportRoot = [System.IO.Path]::GetFullPath((Get-ClearraReportRoot))
$relativeReport = Resolve-ClearraReportPath 'policy/relative-report.json' $repository
Assert-ArtifactPathCondition `
    ($relativeReport.StartsWith($reportRoot, [System.StringComparison]::OrdinalIgnoreCase)) `
    'relative_report_path_resolves_to_report_root'

$repoReport = Join-Path $repository 'reports/forbidden.json'
Assert-ArtifactPathCondition `
    (Test-ArtifactPathThrows { Resolve-ClearraReportPath $repoReport $repository }) `
    'explicit_repo_local_report_path_is_rejected'

$localDirectory = Join-Path $repository '_local/report.json'
Assert-ArtifactPathCondition `
    (Test-ArtifactPathThrows { Resolve-ClearraReportPath $localDirectory $repository }) `
    'explicit_local_directory_is_rejected'

Assert-ClearraRepositoryArtifactPolicy $repository
Assert-ArtifactPathCondition $true 'default_tasks_do_not_create_repo_local_artifacts'
Assert-ArtifactPathCondition `
    ((Get-Content -LiteralPath (Join-Path $repository '.dockerignore') -Raw) -match '(?m)^/?_local/?\s*$') `
    'raw_docker_context_excludes_nonproduct_diagnostics'

$transientPrefix = 'clearra-generic-policy'
$firstTransient = New-TransientBuildDir $transientPrefix
try {
    Set-Content -LiteralPath (Join-Path $firstTransient 'stale.txt') -Value 'stale'
} finally {
    Remove-TransientBuildDir $firstTransient
}
New-Item -ItemType Directory -Force -Path $firstTransient | Out-Null
Set-Content -LiteralPath (Join-Path $firstTransient 'stale.txt') -Value 'stale'
$secondTransient = New-TransientBuildDir $transientPrefix
try {
    Assert-ArtifactPathCondition `
        ($firstTransient -eq $secondTransient) `
        'transient_build_reuses_stable_slot'
    Assert-ArtifactPathCondition `
        (-not (Test-Path -LiteralPath (Join-Path $secondTransient 'stale.txt'))) `
        'transient_build_overwrites_previous_slot_contents'
} finally {
    Remove-TransientBuildDir $secondTransient
}

$fakeRepository = New-TransientBuildDir 'clearra-artifact-policy-test'
try {
    $fakeLocal = Join-Path $fakeRepository '_local'
    New-Item -ItemType Directory -Force -Path $fakeLocal | Out-Null
    & git -C $fakeRepository init --quiet
    if ($LASTEXITCODE -ne 0) { throw 'Could not initialize isolated artifact policy fixture' }
    Set-Content -LiteralPath (Join-Path $fakeRepository '.gitignore') -Value '/_local/'
    Set-Content -LiteralPath (Join-Path $fakeLocal 'measurement.json') -Value '{}'
    Assert-ClearraRepositoryArtifactPolicy $fakeRepository
    Assert-ArtifactPathCondition $true 'ignored_nonproduct_diagnostics_are_not_release_inputs'

    & git -C $fakeRepository add --force -- _local/measurement.json
    if ($LASTEXITCODE -ne 0) { throw 'Could not stage isolated negative artifact fixture' }
    Assert-ArtifactPathCondition `
        (Test-ArtifactPathThrows { Assert-ClearraRepositoryArtifactPolicy $fakeRepository }) `
        'release_acceptance_rejects_tracked_diagnostics'
    Remove-Item -LiteralPath (Join-Path $fakeLocal 'measurement.json')
    Assert-ClearraRepositoryArtifactPolicy $fakeRepository
    Assert-ArtifactPathCondition $true 'intentional_tracked_tool_deletion_needs_no_restoration'
    Set-Content -LiteralPath (Join-Path $fakeRepository '.gitignore') -Value '# missing boundary'
    Assert-ArtifactPathCondition `
        (Test-ArtifactPathThrows { Assert-ClearraRepositoryArtifactPolicy $fakeRepository }) `
        'diagnostics_require_explicit_ignore_boundary'
    foreach ($reference in @('import "../../_local/probe.mjs"', 'path = "../_local/probe"')) {
        $source = [pscustomobject]@{ RelativePath = 'product/input'; Text = $reference }
        Assert-ArtifactPathCondition `
            (Test-ArtifactPathThrows { Assert-ClearraProductExcludesLocalDiagnostics @($source) }) `
            'product_import_or_manifest_cannot_include_diagnostics'
    }
    Assert-ClearraProductExcludesLocalDiagnostics @(
        [pscustomobject]@{ RelativePath = 'product/input'; Text = 'use clearra_app::AppRequest;' }
    )
    Assert-ArtifactPathCondition $true 'ordinary_product_inputs_remain_valid'
    foreach ($renderedName in @('_local/measurement with spaces.json', '"_local/quoted\tmeasurement.json"')) {
        Assert-ArtifactPathCondition `
            (Test-ArtifactPathThrows { Assert-ClearraLocalGitOwnership @($renderedName) @() }) `
            'git_rendered_special_name_is_not_misclassified_as_deleted'
        Assert-ClearraLocalGitOwnership @($renderedName) @($renderedName)
        Assert-ArtifactPathCondition $true 'git_rendered_exact_deleted_name_is_preserved'
    }
} finally {
    Remove-TransientBuildDir $fakeRepository
}

$cacheTestRoot = New-TransientBuildDir 'clearra-cache-lifecycle-test'
try {
    $cacheRepository = Join-Path $cacheTestRoot 'repository'
    $cacheArtifactRoot = Join-Path $cacheTestRoot 'artifacts/build'
    $sourceRoot = Join-Path $cacheRepository 'crates/cache-probe/src'
    New-Item -ItemType Directory -Force -Path $sourceRoot | Out-Null
    Set-Content -LiteralPath (Join-Path $cacheRepository 'Cargo.toml') `
        -Value '[workspace]'
    $sourceFile = Join-Path $sourceRoot 'lib.rs'
    Set-Content -LiteralPath $sourceFile -Value 'pub const CACHE_PROBE: u8 = 1;'
    $orphanedTempFile = Join-Path ([System.IO.Path]::GetTempPath()) `
        "clearra-orphaned-artifact-policy-$PID.tmp"
    Set-Content -LiteralPath $orphanedTempFile -Value 'stale'

    $firstGeneration = Initialize-ClearraBuildArtifactCache `
        -RepositoryRoot $cacheRepository `
        -ArtifactRoot $cacheArtifactRoot `
        -MaxBytes 1MB
    Assert-ArtifactPathCondition `
        ($firstGeneration.action -eq 'fresh') `
        'first_cache_generation_is_fresh'
    Assert-ArtifactPathCondition `
        (-not (Test-Path -LiteralPath $orphanedTempFile)) `
        'next_cache_session_removes_orphaned_clearra_temp_files'

    $sentinel = Join-Path $cacheArtifactRoot 'cargo-target/reuse-sentinel.txt'
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $sentinel) | Out-Null
    Set-Content -LiteralPath $sentinel -Value 'preserve across incremental source changes'
    $sameGeneration = Initialize-ClearraBuildArtifactCache `
        -RepositoryRoot $cacheRepository `
        -ArtifactRoot $cacheArtifactRoot `
        -MaxBytes 1MB
    Assert-ArtifactPathCondition `
        ($sameGeneration.action -eq 'reuse' -and (Test-Path -LiteralPath $sentinel)) `
        'unchanged_inputs_reuse_existing_cache'

    Set-Content -LiteralPath $sourceFile `
        -Value 'pub const CACHE_PROBE_CHANGED: u16 = 200;'
    $changedGeneration = Initialize-ClearraBuildArtifactCache `
        -RepositoryRoot $cacheRepository `
        -ArtifactRoot $cacheArtifactRoot `
        -MaxBytes 1MB
    Assert-ArtifactPathCondition `
        ($changedGeneration.action -eq 'input-change-reuse' -and
            (Test-Path -LiteralPath $sentinel)) `
        'changed_inputs_reuse_incremental_build_cache'

    $oversizedFile = Join-Path $cacheArtifactRoot 'cargo-target/oversized.bin'
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $oversizedFile) | Out-Null
    [System.IO.File]::WriteAllBytes($oversizedFile, [byte[]]::new(4096))
    $budgetReset = Initialize-ClearraBuildArtifactCache `
        -RepositoryRoot $cacheRepository `
        -ArtifactRoot $cacheArtifactRoot `
        -MaxBytes 1024
    Assert-ArtifactPathCondition `
        ($budgetReset.action -eq 'budget-reset' -and
            -not (Test-Path -LiteralPath $oversizedFile)) `
        'oversized_cache_is_reset_before_reuse'

    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $oversizedFile) | Out-Null
    [System.IO.File]::WriteAllBytes($oversizedFile, [byte[]]::new(4096))
    $postRunReset = Invoke-ClearraBuildArtifactCacheRetention `
        -RepositoryRoot $cacheRepository `
        -ArtifactRoot $cacheArtifactRoot `
        -MaxBytes 1024
    Assert-ArtifactPathCondition `
        ($postRunReset.action -eq 'post-run-budget-reset' -and
            -not (Test-Path -LiteralPath $cacheArtifactRoot)) `
        'oversized_cache_is_reset_after_the_active_run'
} finally {
    Remove-TransientBuildDir $cacheTestRoot
}
