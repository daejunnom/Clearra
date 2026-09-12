param(
    [string]$RepositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/clearra-path-helpers.ps1')

function Assert-ArtifactPathCondition([bool]$Condition, [string]$CaseName) {
    if (-not $Condition) { throw "artifact path policy test failed: $CaseName" }
    Write-Output "artifact_path_policy_test=$CaseName status=passed"
}
function Test-ArtifactPathThrows([scriptblock]$Body) {
    try { & $Body | Out-Null; return $false } catch { return $true }
}

# All mutation lives below this test-owned temporary fixture. Never initialize
# the real user's cache or sweep Temp while running a policy test.
$testTempParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\','/')
$fixtureRoot = Join-Path $testTempParent ('clearra-build-policy-' + [Guid]::NewGuid().ToString('N'))
$fixtureEnvironmentNames = @($script:ClearraBuildTransactionEnvironmentNames) + @(
    'LOCALAPPDATA','XDG_CACHE_HOME','WSL_DISTRO_NAME','WSL_INTEROP',
    'CARGO_BUILD_TARGET_DIR','CARGO_BUILD_BUILD_DIR','CARGO_BUILD_RUSTC_WRAPPER',
    'CLEARRA_WSL_CARGO_TARGET_DIR','CLEARRA_RELEASE_BUILD_ROOT','CLEARRA_WSL_NATIVE_BUILD_ROOT',
    'CLEARRA_CORE_C_BUILD_DIR','RUSTC_WORKSPACE_WRAPPER'
)
$savedFixtureEnvironment = @{}
foreach ($name in $fixtureEnvironmentNames) {
    $savedFixtureEnvironment[$name] = [Environment]::GetEnvironmentVariable($name,'Process')
    [Environment]::SetEnvironmentVariable($name,$null,'Process')
}
try {
    New-Item -ItemType Directory -Path $fixtureRoot | Out-Null
    $fixtureBase = Join-Path $fixtureRoot 'cache-home'
    $env:LOCALAPPDATA = $fixtureBase
    $env:XDG_CACHE_HOME = $fixtureBase
    $fixtureSource = Join-Path $fixtureRoot 'source'
    $otherSource = Join-Path $fixtureRoot 'other-source'
    New-Item -ItemType Directory -Path $fixtureSource,$otherSource | Out-Null

    $repository = [IO.Path]::GetFullPath($RepositoryRoot)
    $reportRoot = [IO.Path]::GetFullPath((Get-ClearraReportRoot))
    $relativeReport = Resolve-ClearraReportPath 'policy/relative-report.json' $repository
    Assert-ArtifactPathCondition ($relativeReport.StartsWith($reportRoot, (Get-ClearraBuildPathComparison))) 'relative_report_uses_report_root'
    Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Resolve-ClearraReportPath (Join-Path $repository 'reports/forbidden.json') $repository }) 'repository_report_rejected'
    Assert-ClearraRepositoryArtifactPolicy $repository
    Assert-ArtifactPathCondition ((Get-Content -LiteralPath (Join-Path $repository '.dockerignore') -Raw) -match '(?m)^/?_local/?\s*$') 'docker_excludes_diagnostics'

    . (Join-Path $PSScriptRoot 'lib/clearra-build-entrypoints.test.ps1')
    . (Join-Path $PSScriptRoot 'lib/clearra-build-product-catalog.test.ps1')
    . (Join-Path $PSScriptRoot 'lib/clearra-build-lifecycle.test.ps1')
    foreach ($utility in @('Get-ClearraBuildInputFiles','Get-ClearraWorkspaceBuildSignature',
            'Get-ClearraCommandMetadata','Get-ClearraCommandVersionMetadata','Get-ClearraDirectorySizeBytes')) {
        Assert-ArtifactPathCondition ($null -ne (Get-Command $utility -ErrorAction SilentlyContinue)) "source_utility_API_preserved_$utility"
    }

    # Preserve the local-diagnostics ownership contract without using a build
    # slot as a source tree or writing to the real worktree.
    $localFixture = Join-Path $fixtureRoot 'diagnostics-source'
    New-Item -ItemType Directory -Path (Join-Path $localFixture '_local') -Force | Out-Null
    & git -C $localFixture init --quiet
    if ($LASTEXITCODE -ne 0) { throw 'Could not initialize isolated diagnostics fixture.' }
    [IO.File]::WriteAllText((Join-Path $localFixture '.gitignore'), "/_local/" + [Environment]::NewLine)
    [IO.File]::WriteAllText((Join-Path $localFixture '_local/measurement.json'), '{}')
    Assert-ClearraRepositoryArtifactPolicy $localFixture
    Assert-ArtifactPathCondition $true 'ignored_nonproduct_diagnostics_are_not_release_inputs'
    & git -C $localFixture add --force -- _local/measurement.json
    if ($LASTEXITCODE -ne 0) { throw 'Could not stage isolated diagnostics fixture.' }
    Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Assert-ClearraRepositoryArtifactPolicy $localFixture }) 'tracked_diagnostics_rejected'
    Remove-Item -LiteralPath (Join-Path $localFixture '_local/measurement.json')
    Assert-ClearraRepositoryArtifactPolicy $localFixture
    Assert-ArtifactPathCondition $true 'intentional_tracked_diagnostic_deletion_is_valid'
    foreach ($reference in @('import "../../_local/probe.mjs"', 'path = "../_local/probe"')) {
        $inputFile = [pscustomobject]@{ RelativePath='product/input'; Text=$reference }
        Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Assert-ClearraProductExcludesLocalDiagnostics @($inputFile) }) 'product_cannot_import_diagnostics'
    }
    foreach ($renderedName in @('_local/measurement with spaces.json','"_local/quoted\tmeasurement.json"')) {
        Assert-ArtifactPathCondition (Test-ArtifactPathThrows { Assert-ClearraLocalGitOwnership @($renderedName) @() }) 'special_git_name_not_misclassified_as_deleted'
        Assert-ClearraLocalGitOwnership @($renderedName) @($renderedName)
    }
} finally {
    if ($null -ne $script:ClearraBuildTransaction) {
        try { Exit-ClearraBuildArtifactCacheUsage } catch { Write-Warning $_.Exception.Message }
    }
    foreach ($name in $fixtureEnvironmentNames) {
        [Environment]::SetEnvironmentVariable($name,$savedFixtureEnvironment[$name],'Process')
    }
    $resolvedFixture = [IO.Path]::GetFullPath($fixtureRoot)
    if (-not ([IO.Path]::GetDirectoryName($resolvedFixture)).Equals($testTempParent, (Get-ClearraBuildPathComparison)) -or
        [IO.Path]::GetFileName($resolvedFixture) -notmatch '^clearra-build-policy-[0-9a-f]{32}$') {
        throw 'Refusing an unexpected fixture cleanup path.'
    }
    if (Test-Path -LiteralPath $resolvedFixture) {
        Assert-ClearraBuildTreeNoReparse $resolvedFixture
        Remove-Item -LiteralPath $resolvedFixture -Recurse -Force
    }
}
