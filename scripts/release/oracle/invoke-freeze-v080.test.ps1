[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$wrapper = Join-Path $PSScriptRoot 'invoke-freeze-v080.ps1'
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) (
    'clearra-oracle-freeze-wrapper-test-' + [guid]::NewGuid().ToString('N')
)

try {
    [void][IO.Directory]::CreateDirectory($temporaryRoot)
    $source = Join-Path $temporaryRoot 'source.tar.gz'
    $overlay = Join-Path $temporaryRoot 'private-overlay-no-config.tar'
    $dist = Join-Path $temporaryRoot 'ctk3-dist.tar'
    $dependencies = Join-Path $temporaryRoot 'node_modules.tar'
    $manifest = Join-Path $temporaryRoot 'manifest.json'
    [IO.File]::WriteAllBytes($source, [byte[]](1, 2, 3, 4))
    [IO.File]::WriteAllBytes($overlay, [byte[]](5, 6, 7))
    [IO.File]::WriteAllBytes($dist, [byte[]](8, 9))
    [IO.File]::WriteAllBytes($dependencies, [byte[]](10, 11, 12, 13, 14))

    $arguments = @{
        SourceCommit = '0123456789abcdef0123456789abcdef01234567'
        SourceArchive = $source
        OverlayArchive = $overlay
        Ctk3DistArchive = $dist
        DependenciesArchive = $dependencies
        ManifestOutput = $manifest
        IdentityFile = Join-Path $temporaryRoot 'identity-must-not-be-read'
        AuditOnly = $true
    }
    $audit = @(& $wrapper @arguments)
    if ($audit.Count -ne 4 -or
        $audit[0] -cne 'oracle_freeze_invoker=audit-ok' -or
        $audit[1] -cne 'oracle_source_commit=0123456789abcdef0123456789abcdef01234567' -or
        $audit[2] -cne 'oracle_release_id=v0.8.0-0123456' -or
        $audit[3] -cnotmatch '^oracle_freeze_helper_sha256=[0-9a-f]{64}$') {
        throw 'AuditOnly attestation did not match.'
    }
    if (Test-Path -LiteralPath $manifest) {
        throw 'AuditOnly created a manifest output.'
    }

    [IO.File]::WriteAllText($manifest, "occupied`n", [Text.UTF8Encoding]::new($false))
    $rejected = $false
    try {
        [void]@(& $wrapper @arguments)
    } catch {
        $rejected = $_.Exception.Message -like '*manifest output already exists*'
    }
    if (-not $rejected) {
        throw 'AuditOnly accepted an occupied manifest output path.'
    }

    'oracle_freeze_wrapper_test=pass'
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        $resolved = [IO.Path]::GetFullPath($temporaryRoot)
        $expectedParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if ($resolved.StartsWith($expectedParent, [StringComparison]::OrdinalIgnoreCase) -and
            [IO.Path]::GetFileName($resolved).StartsWith('clearra-oracle-freeze-wrapper-test-', [StringComparison]::Ordinal)) {
            Remove-Item -LiteralPath $resolved -Recurse -Force
        }
    }
}
