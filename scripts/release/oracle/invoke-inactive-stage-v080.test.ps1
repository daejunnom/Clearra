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

function Invoke-ExtractedWrapperFunction {
    param(
        [Parameter(Mandatory = $true)][string] $Path,
        [Parameter(Mandatory = $true)][string] $FunctionName,
        [Parameter(Mandatory = $true)][object[]] $Arguments
    )
    $tokens = $null
    $parseErrors = $null
    $ast = [Management.Automation.Language.Parser]::ParseFile(
        $Path,
        [ref]$tokens,
        [ref]$parseErrors
    )
    if ($parseErrors.Count -ne 0) {
        throw "Wrapper parse failed while extracting $FunctionName."
    }
    $definitions = @($ast.FindAll({
        param($node)
        return $node -is [Management.Automation.Language.FunctionDefinitionAst] -and
            $node.Name -ceq $FunctionName
    }, $true))
    if ($definitions.Count -ne 1) {
        throw "Wrapper must define $FunctionName exactly once."
    }
    $invocation = [scriptblock]::Create(
        $definitions[0].Extent.Text + "`n& $FunctionName @args"
    )
    return & $invocation @Arguments
}

$windowsSshConfig = Invoke-ExtractedWrapperFunction `
    -Path $wrapper -FunctionName 'Get-OracleSshConfigPath' -Arguments @('windows')
$linuxSshConfig = Invoke-ExtractedWrapperFunction `
    -Path $wrapper -FunctionName 'Get-OracleSshConfigPath' -Arguments @('linux')
if ($windowsSshConfig -cne 'NUL' -or $linuxSshConfig -cne '/dev/null') {
    throw 'Inactive-stage Windows/Linux SSH config argument contract drifted.'
}
$wrapperSource = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $wrapper))
foreach ($requiredPlatformAssembly in @(
    '$hostPlatform = Get-OracleHostPlatform',
    '$sshConfigPath = Get-OracleSshConfigPath -Platform $hostPlatform',
    "'-F', `$sshConfigPath"
)) {
    if ($wrapperSource.IndexOf($requiredPlatformAssembly, [StringComparison]::Ordinal) -lt 0) {
        throw "Inactive-stage SSH platform assembly is missing: $requiredPlatformAssembly"
    }
}
foreach ($requiredRemoteContract in @(
    "'--remote-overlay-archive', `$RemoteOverlayArchive",
    "'--remote-overlay-sha256', `$RemoteOverlaySha256"
)) {
    if ($wrapperSource.IndexOf($requiredRemoteContract, [StringComparison]::Ordinal) -lt 0) {
        throw "Inactive-stage remote root-helper argument contract is missing: $requiredRemoteContract"
    }
}
foreach ($forbiddenRemoteRead in @(
    'function Copy-RemoteSealedOverlay',
    "'-o', '1001', '-g', '1001'",
    "'sudo', '-n', '/usr/bin/sha256sum', '--', `$RemoteOverlayArchive"
)) {
    if ($wrapperSource.IndexOf($forbiddenRemoteRead, [StringComparison]::Ordinal) -ge 0) {
        throw "Inactive-stage wrapper regained private-overlay read authority: $forbiddenRemoteRead"
    }
}
$stageTemplateSource = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'clearra-oracle-inactive-stage-v080.template'))
foreach ($sealedCopyMarker in @(
    'os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW',
    'metadata.st_nlink != 1 or metadata.st_size <= 0',
    'stat.S_IMODE(metadata.st_mode) & 0o022',
    'os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC | os.O_NOFOLLOW',
    'os.fsync(destination_fd)',
    'os.unlink(destination)',
    'upload_expected_count=6',
    'upload_expected_count=7'
)) {
    if ($stageTemplateSource.IndexOf($sealedCopyMarker, [StringComparison]::Ordinal) -lt 0) {
        throw "Inactive-stage sealed-overlay fail-closed contract drifted: $sealedCopyMarker"
    }
}
$cleanupStart = $stageTemplateSource.IndexOf(
    'if [ "$cleanup_only" -eq 1 ]; then',
    [StringComparison]::Ordinal
)
$cleanupEnd = $stageTemplateSource.IndexOf(
    "`nfi`n`ncapture_baseline",
    $cleanupStart,
    [StringComparison]::Ordinal
)
if ($cleanupStart -lt 0 -or $cleanupEnd -lt 0) {
    throw 'Inactive-stage CleanupOnly branch boundary is unavailable.'
}
$cleanupBranch = $stageTemplateSource.Substring($cleanupStart, $cleanupEnd - $cleanupStart)
foreach ($cleanupDigesterMarker in @(
    'validate_digester=$2',
    'require_exact_hash "$validate_digester" "$expected_digester_sha256"',
    'cleanup_digester=$upload_root/clearra-release-tree-digest.py',
    'validate_candidate "$candidate_path" "$cleanup_digester"'
)) {
    if ($stageTemplateSource.IndexOf($cleanupDigesterMarker, [StringComparison]::Ordinal) -lt 0) {
        throw "Inactive-stage explicit digester contract drifted: $cleanupDigesterMarker"
    }
}
if ($cleanupBranch.IndexOf(
    '$input_root/clearra-release-tree-digest.py',
    [StringComparison]::Ordinal
) -ge 0) {
    throw 'Candidate-present CleanupOnly depends on the unmaterialized input-root digester.'
}
if ($stageTemplateSource.IndexOf(
    'validate_candidate "$candidate_path" "$input_root/clearra-release-tree-digest.py"',
    [StringComparison]::Ordinal
) -lt 0) {
    throw 'Normal inactive staging lost the root-owned input digester contract.'
}

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
    # Windows PowerShell 5.1 and PowerShell 7 indent ConvertTo-Json output
    # differently. The production parser deliberately accepts only Node's
    # two-space canonical JSON, so make the cross-host test fixture canonical
    # with the same required Node runtime before invoking the wrapper.
    $manifestJson = $manifest | ConvertTo-Json -Depth 10 -Compress
    [IO.File]::WriteAllText(
        $manifestPath,
        $manifestJson,
        [Text.UTF8Encoding]::new($false)
    )
    $canonicalizerPath = Join-Path $temporaryRoot 'canonicalize-json.cjs'
    $canonicalizeManifest = 'const fs=require("node:fs");const path=process.argv[2];const value=JSON.parse(fs.readFileSync(path,"utf8"));fs.writeFileSync(path,JSON.stringify(value,null,2)+"\n");'
    [IO.File]::WriteAllText(
        $canonicalizerPath,
        $canonicalizeManifest,
        [Text.UTF8Encoding]::new($false)
    )
    & node $canonicalizerPath $manifestPath
    if ($LASTEXITCODE -ne 0) {
        throw 'Oracle stage manifest fixture canonicalization failed.'
    }

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

    $remoteOverlaySha256 = Get-Sha256 $overlay
    $remoteOverlayArchive = "/opt/clearra/sealed-release-inputs/private-overlay-no-config-$remoteOverlaySha256.tar"
    $remoteAudit = @(& $wrapper `
        -ManifestPath $manifestPath `
        -SourceArchive $source `
        -RemoteOverlayArchive $remoteOverlayArchive `
        -RemoteOverlaySha256 $remoteOverlaySha256 `
        -Ctk3DistArchive $dist `
        -DependenciesArchive $dependencies `
        -AuditOnly)
    if ($remoteAudit.Count -ne 6 -or
        $remoteAudit[0] -cne 'oracle_inactive_stage_invoker=audit-ok') {
        throw 'Remote-overlay AuditOnly did not remain a typed local-only audit.'
    }

    $mismatchedRemoteSha256 = 'f' * 64
    $rejected = $false
    try {
        [void]@(& $wrapper `
            -ManifestPath $manifestPath `
            -SourceArchive $source `
            -RemoteOverlayArchive "/opt/clearra/sealed-release-inputs/private-overlay-no-config-$mismatchedRemoteSha256.tar" `
            -RemoteOverlaySha256 $mismatchedRemoteSha256 `
            -Ctk3DistArchive $dist `
            -DependenciesArchive $dependencies `
            -AuditOnly)
    } catch {
        $rejected = $_.Exception.Message -like '*does not match the frozen manifest*'
    }
    if (-not $rejected) {
        throw 'Remote-overlay AuditOnly accepted a manifest/hash authority mismatch.'
    }

    $rejected = $false
    try {
        [void]@(& $wrapper `
            -ManifestPath $manifestPath `
            -SourceArchive $source `
            -OverlayArchive $overlay `
            -RemoteOverlayArchive $remoteOverlayArchive `
            -RemoteOverlaySha256 $remoteOverlaySha256 `
            -Ctk3DistArchive $dist `
            -DependenciesArchive $dependencies `
            -AuditOnly)
    } catch {
        $rejected = $true
    }
    if (-not $rejected) {
        throw 'Inactive-stage wrapper accepted local and remote overlay inputs together.'
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
