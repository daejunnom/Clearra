$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repositoryRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$Root = $repositoryRoot
$Errors = [System.Collections.Generic.List[string]]::new()
$Warnings = [System.Collections.Generic.List[string]]::new()

. (Join-Path $repositoryRoot 'scripts\lib\architecture-validation-repository.ps1')
. (Join-Path $repositoryRoot 'scripts\architecture\validate_pc4_full_solution_authority_contract.ps1')

function Assert-Contract([bool]$Condition, [string]$Message) {
    if (-not $Condition) {
        throw "PC4 authority validator contract test failed: $Message"
    }
}

function Write-ContractFixture(
    [string]$FixtureRoot,
    [string]$RelativePath,
    [string]$Contents
) {
    $path = Join-Path $FixtureRoot $RelativePath
    $directory = Split-Path -Parent $path
    [void][System.IO.Directory]::CreateDirectory($directory)
    [System.IO.File]::WriteAllText($path, $Contents)
}

$temporaryRoot = Join-Path (
    [System.IO.Path]::GetTempPath()
) "clearra-pc4-authority-contract-$([guid]::NewGuid().ToString('N'))"
[void][System.IO.Directory]::CreateDirectory($temporaryRoot)

try {
    # Exercise the real production adapter, not only the synthetic authority
    # fixtures below. Its evidence constructor is private; a raw struct-literal
    # requirement would reject the stronger source-binding boundary.
    $candidateAdapter = Get-RustProductionContents (
        Get-Content -LiteralPath (Join-Path $repositoryRoot `
            'crates/clearra-app/src/pc4_graph_candidate_adapter.rs') -Raw
    )
    $Errors.Clear()
    Assert-Pc4GraphCandidateAdapterCompleteness $candidateAdapter
    Assert-Contract ($Errors.Count -eq 0) 'real complete adapter no longer matches its authority contract'

    foreach ($removedGuard in @(
        'PcCandidateCompletenessEvidence::from_verified_complete_source(',
        'if !self.is_exhausted() {'
    )) {
        $Errors.Clear()
        Assert-Pc4GraphCandidateAdapterCompleteness ($candidateAdapter.Replace($removedGuard, ''))
        Assert-Contract (($Errors -join "`n").Contains($removedGuard)) `
            "candidate adapter mutation did not reject missing $removedGuard"
    }
    $Errors.Clear()
    Assert-Pc4GraphCandidateAdapterCompleteness ($candidateAdapter.Replace(
        'PcCandidateCompletenessEvidence::from_verified_complete_source(',
        'PcCandidateCompletenessEvidence {'
    ))
    Assert-Contract ($Errors.Count -ne 0) 'raw evidence construction was accepted as the verified constructor'

    $Root = $temporaryRoot

    $scannedFixtures = [ordered]@{
        'crates/clearra-core-executor/src/pc4_authority.rs' = @'
pub const PC4_AUTHORITY: &str = "best_action";
'@
        'packages/clearra-ui/src/lib/Pc4Authority.svelte' = @'
<script>const authority = "Krylov";</script>
'@
        'apps/clearra-web/svelte.config.js' = @'
export default { pc4Authority: "policy_asset" };
'@
        'apps/clearra-web/pc4.config.json' = @'
{"pc4Authority":"value_asset"}
'@
        'scripts/release/pc4-discovery.mjs' = @'
export const authority = "v_star";
'@
        '.github/workflows/pc4-release.yml' = @'
pc4-authority: policy-action
'@
        'apps/clearra-discord-bot/Dockerfile.pc4' = @'
RUN verify-pc4-best-transition
'@
    }
    foreach ($fixture in $scannedFixtures.GetEnumerator()) {
        Write-ContractFixture $temporaryRoot $fixture.Key $fixture.Value
    }

    Write-ContractFixture $temporaryRoot `
        'crates/clearra-app/src/pc4_inline_test_only.rs' @'
pub fn graph_only_authority() {}

#[cfg(test)]
mod tests {
    const RESEARCH_ONLY: &str = "Krylov";
}
'@
    Write-ContractFixture $temporaryRoot `
        'crates/clearra-core-executor/src/backend/queue_observation_policy.rs' @'
pub struct PolicyValue;
fn best_transition() {}
'@
    Write-ContractFixture $temporaryRoot `
        'packages/clearra-ui/test/Pc4Authority.test.mjs' 'const researchOnly = "Krylov";'
    Write-ContractFixture $temporaryRoot `
        'packages/clearra-ui/docs/pc4-authority.md' 'V* is documented but not product authority.'
    Write-ContractFixture $temporaryRoot `
        'crates/clearra-pc-next-probability/src/lib.rs' 'pub const RESEARCH_ONLY: &str = "Krylov";'
    Write-ContractFixture $temporaryRoot `
        '.github/workflows/release-cli.yml' 'tags: ["v*"]'

    $scannedPaths = @(
        Get-Pc4ProductAuthoritySourceFiles |
            ForEach-Object { Get-Pc4ProductAuthorityRelativePath $_ }
    )
    foreach ($expectedPath in $scannedFixtures.Keys) {
        Assert-Contract ($scannedPaths -contains $expectedPath) "source scan omitted $expectedPath"
    }
    foreach ($allowedPath in @(
        'packages/clearra-ui/test/Pc4Authority.test.mjs',
        'packages/clearra-ui/docs/pc4-authority.md',
        'crates/clearra-pc-next-probability/src/lib.rs'
    )) {
        Assert-Contract ($scannedPaths -notcontains $allowedPath) "source scan included allowed test, documentation, or dormant-seam path $allowedPath"
    }

    $Errors.Clear()
    Assert-Pc4ProductDecisionSourceAbsence
    $decisionErrors = $Errors -join "`n"
    foreach ($expectedPath in $scannedFixtures.Keys) {
        Assert-Contract $decisionErrors.Contains($expectedPath) "decision-source scan did not reject $expectedPath"
    }
    foreach ($allowedPath in @(
        'pc4_inline_test_only.rs',
        'queue_observation_policy.rs',
        'Pc4Authority.test.mjs',
        'pc4-authority.md',
        'clearra-pc-next-probability/src/lib.rs',
        'release-cli.yml'
    )) {
        Assert-Contract (-not $decisionErrors.Contains($allowedPath)) "decision-source scan rejected allowed path $allowedPath"
    }

    Write-ContractFixture $temporaryRoot `
        'apps/clearra-web/static/tablebase/pc4-compact-exact-v12.bin' 'legacy-fixture'
    Write-ContractFixture $temporaryRoot `
        'crates/clearra-core-executor/src/backend/wasm_cpu/pc4_tablebase.rs' `
        'const MAGIC: &[u8; 8] = b"CLR4TB12";'
    Write-ContractFixture $temporaryRoot `
        'packages/clearra-ui/test/legacy-static-beta.test.mjs' `
        'const legacyTestFixture = "pc4-compact-exact-v12.bin";'

    $Errors.Clear()
    Assert-Pc4V090LegacyStaticBetaMigration
    Assert-Contract ($Errors.Count -eq 0) 'v0.8.1 compatibility mode rejected the retained static beta'

    $migrationMarker = 'scripts/architecture/pc4-v090-online-authority.mode'
    Write-ContractFixture $temporaryRoot $migrationMarker `
        'pc4-product-authority=v0.9-online-graph-v1'
    $Errors.Clear()
    Assert-Pc4V090LegacyStaticBetaMigration
    $migrationErrors = $Errors -join "`n"
    Assert-Contract ($migrationErrors.Contains('pc4-compact-exact-v12.bin')) `
        'v0.9 migration mode did not reject the retained static asset'
    Assert-Contract ($migrationErrors.Contains('pc4_tablebase.rs')) `
        'v0.9 migration mode did not reject the live CLR4TB12 loader'
    Assert-Contract (-not $migrationErrors.Contains('legacy-static-beta.test.mjs')) `
        'v0.9 migration mode rejected a test-only legacy fixture'

    [System.IO.File]::WriteAllText(
        (Join-Path $temporaryRoot $migrationMarker),
        'pc4-product-authority=unqualified'
    )
    $Errors.Clear()
    Assert-Pc4V090LegacyStaticBetaMigration
    Assert-Contract (($Errors -join "`n").Contains('must contain exactly')) `
        'an invalid v0.9 migration marker did not fail closed'

    Write-Output 'PC4 full-solution authority validator contract tests passed.'
} finally {
    $Root = $repositoryRoot
    $resolvedTemporaryRoot = [System.IO.Path]::GetFullPath($temporaryRoot)
    $resolvedSystemTemp = [System.IO.Path]::GetFullPath(
        [System.IO.Path]::GetTempPath()
    ).TrimEnd([System.IO.Path]::DirectorySeparatorChar)
    $isBoundedTemporaryPath = $resolvedTemporaryRoot.StartsWith(
        "$resolvedSystemTemp$([System.IO.Path]::DirectorySeparatorChar)clearra-pc4-authority-contract-",
        [System.StringComparison]::OrdinalIgnoreCase
    )
    if ($isBoundedTemporaryPath -and (Test-Path -LiteralPath $resolvedTemporaryRoot)) {
        Remove-Item -LiteralPath $resolvedTemporaryRoot -Recurse -Force
    }
}
