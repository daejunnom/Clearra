[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$wrapper = Join-Path $PSScriptRoot 'invoke-inactive-stage-v080.ps1'
$launcher = Join-Path $PSScriptRoot 'clearra-oracle-release-deploy-v080'
$digester = Join-Path $PSScriptRoot 'clearra-release-tree-digest.py'
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) (
    'clearra-oracle-wrapper-test-' + [guid]::NewGuid().ToString('N')
)

function Get-Sha256([string] $Path) {
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

try {
    [void](New-Item -ItemType Directory -Path $temporaryRoot)
    $source = Join-Path $temporaryRoot 'source.tar.gz'
    $overlay = Join-Path $temporaryRoot 'private-overlay-no-config.tar'
    $dist = Join-Path $temporaryRoot 'ctk3-dist.tar'
    $dependencies = Join-Path $temporaryRoot 'node_modules.tar'
    [IO.File]::WriteAllBytes($source, [byte[]](1, 2, 3, 4))
    [IO.File]::WriteAllBytes($overlay, [byte[]](5, 6, 7))
    [IO.File]::WriteAllBytes($dist, [byte[]](8, 9))
    [IO.File]::WriteAllBytes($dependencies, [byte[]](10, 11, 12, 13, 14))

    $manifest = [ordered]@{
        schemaVersion = 'clearra.oracle.inactive-stage.v080.v1'
        sourceCommit = '0123456789abcdef0123456789abcdef01234567'
        releaseId = 'v0.8.0-0123456'
        active = [ordered]@{
            releasePath = '/opt/clearra/releases/v0.7.5-042ec21'
            treeSha256 = 'a' * 64
            settingsSha256 = 'b' * 64
            settingsSize = 367
            configSha256 = 'c' * 64
            configSize = 3432
        }
        candidate = [ordered]@{
            treeSha256 = 'd' * 64
            counts = [ordered]@{
                directories = 500
                files0644 = 3200
                files0755 = 8
                symlinks = 2
            }
        }
        layers = [ordered]@{
            source = [ordered]@{
                sha256 = Get-Sha256 $source
                size = (Get-Item -LiteralPath $source).Length
                counts = [ordered]@{ files = 1; directories = 0; symlinks = 0 }
            }
            overlay = [ordered]@{
                sha256 = Get-Sha256 $overlay
                size = (Get-Item -LiteralPath $overlay).Length
                counts = [ordered]@{ files = 1; directories = 0; symlinks = 0 }
            }
            ctk3Dist = [ordered]@{
                sha256 = Get-Sha256 $dist
                size = (Get-Item -LiteralPath $dist).Length
                counts = [ordered]@{ files = 1; directories = 0; symlinks = 0 }
            }
            dependencies = [ordered]@{
                sha256 = Get-Sha256 $dependencies
                size = (Get-Item -LiteralPath $dependencies).Length
                counts = [ordered]@{ files = 1; directories = 1; symlinks = 2 }
            }
        }
        tools = [ordered]@{
            launcher = [ordered]@{
                sha256 = Get-Sha256 $launcher
                size = (Get-Item -LiteralPath $launcher).Length
                prior = [ordered]@{ sha256 = 'e' * 64; size = 1 }
            }
            digester = [ordered]@{
                sha256 = Get-Sha256 $digester
                size = (Get-Item -LiteralPath $digester).Length
                prior = $null
            }
        }
    }
    $manifestPath = Join-Path $temporaryRoot 'manifest.json'
    $manifestJson = $manifest | ConvertTo-Json -Depth 10
    [IO.File]::WriteAllText(
        $manifestPath,
        $manifestJson.Replace("`r`n", "`n") + "`n",
        [Text.UTF8Encoding]::new($false)
    )

    $audit = @(& $wrapper `
        -ManifestPath $manifestPath `
        -SourceArchive $source `
        -OverlayArchive $overlay `
        -Ctk3DistArchive $dist `
        -DependenciesArchive $dependencies `
        -AuditOnly)
    if ($audit.Count -ne 6 -or
        $audit[0] -cne 'oracle_inactive_stage_invoker=audit-ok' -or
        $audit[1] -cne 'oracle_source_commit=0123456789abcdef0123456789abcdef01234567' -or
        $audit[2] -cne 'oracle_release_id=v0.8.0-0123456') {
        throw 'AuditOnly attestation did not match.'
    }

    [IO.File]::AppendAllText($source, 'tampered', [Text.UTF8Encoding]::new($false))
    $rejected = $false
    try {
        [void]@(& $wrapper `
            -ManifestPath $manifestPath `
            -SourceArchive $source `
            -OverlayArchive $overlay `
            -Ctk3DistArchive $dist `
            -DependenciesArchive $dependencies `
            -AuditOnly)
    } catch {
        $rejected = $_.Exception.Message -like '*Frozen input does not match: source.tar.gz*'
    }
    if (-not $rejected) {
        throw 'AuditOnly accepted a frozen-input digest drift.'
    }

    'oracle_inactive_stage_wrapper_test=pass'
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force
    }
}
