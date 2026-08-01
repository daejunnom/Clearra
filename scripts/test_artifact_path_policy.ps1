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

$transientPrefix = "clearra-core-c-policy-$PID"
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

$fakeRepository = Join-Path ([System.IO.Path]::GetTempPath()) `
    "clearra-artifact-policy-$PID-$([System.Guid]::NewGuid().ToString('N'))"
try {
    $fakeLocal = Join-Path $fakeRepository '_local'
    New-Item -ItemType Directory -Force -Path $fakeLocal | Out-Null
    Set-Content -LiteralPath (Join-Path $fakeLocal 'bundle.py') -Value '# test bundle tool'
    Assert-ClearraRepositoryArtifactPolicy $fakeRepository
    Assert-ArtifactPathCondition $true 'repository_local_bundle_tool_is_allowed'

    Set-Content -LiteralPath (Join-Path $fakeLocal 'project_bundle.txt') -Value 'test bundle output'
    Assert-ClearraRepositoryArtifactPolicy $fakeRepository
    Assert-ArtifactPathCondition $true 'repository_local_bundle_output_is_allowed'

    Set-Content -LiteralPath (Join-Path $fakeLocal 'unexpected-report.json') -Value '{}'
    Assert-ArtifactPathCondition `
        (Test-ArtifactPathThrows { Assert-ClearraRepositoryArtifactPolicy $fakeRepository }) `
        'release_acceptance_rejects_unexpected_local_artifact'
} finally {
    if (Test-Path -LiteralPath $fakeRepository) {
        Remove-Item -LiteralPath $fakeRepository -Recurse -Force
    }
}

$cacheTestRoot = Join-Path ([System.IO.Path]::GetTempPath()) `
    "clearra-cache-lifecycle-$PID-$([System.Guid]::NewGuid().ToString('N'))"
try {
    $cacheRepository = Join-Path $cacheTestRoot 'repository'
    $cacheArtifactRoot = Join-Path $cacheTestRoot 'artifacts/build'
    $sourceRoot = Join-Path $cacheRepository 'crates/cache-probe/src'
    New-Item -ItemType Directory -Force -Path $sourceRoot | Out-Null
    Set-Content -LiteralPath (Join-Path $cacheRepository 'Cargo.toml') `
        -Value '[workspace]'
    $sourceFile = Join-Path $sourceRoot 'lib.rs'
    Set-Content -LiteralPath $sourceFile -Value 'pub const CACHE_PROBE: u8 = 1;'

    $firstGeneration = Initialize-ClearraBuildArtifactCache `
        -RepositoryRoot $cacheRepository `
        -ArtifactRoot $cacheArtifactRoot `
        -MaxBytes 1MB
    Assert-ArtifactPathCondition `
        ($firstGeneration.action -eq 'fresh') `
        'first_cache_generation_is_fresh'

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
} finally {
    if (Test-Path -LiteralPath $cacheTestRoot) {
        Remove-Item -LiteralPath $cacheTestRoot -Recurse -Force
    }
}
