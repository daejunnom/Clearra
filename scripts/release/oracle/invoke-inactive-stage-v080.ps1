[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)]
    [string] $ManifestPath,

    [Parameter(Mandatory = $true)]
    [string] $SourceArchive,

    [Parameter(Mandatory = $true)]
    [string] $OverlayArchive,

    [Parameter(Mandatory = $true)]
    [string] $Ctk3DistArchive,

    [Parameter(Mandatory = $true)]
    [string] $DependenciesArchive,

    [string] $IdentityFile,

    [switch] $AuditOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$expectedFingerprint = 'SHA256:mdw7bdzZOBrd6sCebPmMVuTaps+ct2OaOle/gaZMBKU'
$expectedKnownHostsSha256 = '2f7f658642c2dec4f9ad9e34d959b0215bdcf877e5636daebb003888434a8fd0'
$expectedKnownHostsRecord = '157.151.254.175 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIL/HJCUimGyR+oxmnoyYb10MT28zy4bh7SPwhj6ZJUz+'
$hostName = '157.151.254.175'
$userName = 'ubuntu'
$knownHostsPath = Join-Path $PSScriptRoot 'clearra-oracle-known-hosts'
$generatorPath = Join-Path $PSScriptRoot 'create-inactive-stage-v080.mjs'
$launcherPath = Join-Path $PSScriptRoot 'clearra-oracle-release-deploy-v080'
$digesterPath = Join-Path $PSScriptRoot 'clearra-release-tree-digest.py'

function Get-ExactLeaf {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,

        [Parameter(Mandatory = $true)]
        [string] $Label
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Label is unavailable."
    }
    $item = Get-Item -LiteralPath $Path -Force
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "$Label must not be a reparse point."
    }
    return $item
}

function Get-ExactSha256 {
    param([Parameter(Mandatory = $true)][string] $Path)
    return (Get-FileHash -Algorithm SHA256 -LiteralPath $Path).Hash.ToLowerInvariant()
}

function Assert-FrozenInput {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject] $FrozenInput
    )

    $item = Get-ExactLeaf -Path $FrozenInput.Local -Label "Frozen input $($FrozenInput.RemoteName)"
    if ($item.Length -ne [long]$FrozenInput.Length -or
        (Get-ExactSha256 -Path $FrozenInput.Local) -cne [string]$FrozenInput.Sha256) {
        throw "Frozen input does not match: $($FrozenInput.RemoteName)"
    }
}

foreach ($requiredTool in @(
    @{ Path = $knownHostsPath; Label = 'Pinned Oracle host-key file' },
    @{ Path = $generatorPath; Label = 'Tracked Oracle stage generator' },
    @{ Path = $launcherPath; Label = 'Tracked Oracle deployment launcher' },
    @{ Path = $digesterPath; Label = 'Tracked Oracle tree digester' },
    @{ Path = $ManifestPath; Label = 'Oracle stage manifest' }
)) {
    [void](Get-ExactLeaf -Path $requiredTool.Path -Label $requiredTool.Label)
}

$knownHostsSha256 = Get-ExactSha256 -Path $knownHostsPath
if ($knownHostsSha256 -cne $expectedKnownHostsSha256) {
    throw 'Pinned Oracle host-key file digest does not match.'
}
$knownHostsRecords = @(Get-Content -LiteralPath $knownHostsPath | Where-Object { $_.Length -gt 0 })
if ($knownHostsRecords.Count -ne 1 -or $knownHostsRecords[0] -cne $expectedKnownHostsRecord) {
    throw 'Pinned Oracle host-key file must contain exactly the approved record.'
}
$fingerprint = @(& ssh-keygen -lf $knownHostsPath -E sha256)
$expectedFingerprintLine = "256 $expectedFingerprint 157.151.254.175 (ED25519)"
if ($LASTEXITCODE -ne 0 -or $fingerprint.Count -ne 1 -or $fingerprint[0] -cne $expectedFingerprintLine) {
    throw 'Pinned Oracle host-key fingerprint does not match.'
}

$generatorAudit = @(& node $generatorPath --manifest $ManifestPath --audit)
if ($LASTEXITCODE -ne 0 -or $generatorAudit.Count -ne 7) {
    throw 'Oracle stage manifest or bootstrap template audit failed.'
}
$auditValues = [ordered]@{}
foreach ($line in $generatorAudit) {
    if ($line -cnotmatch '^([a-z0-9_]+)=([A-Za-z0-9._-]+)$' -or $auditValues.Contains($Matches[1])) {
        throw 'Oracle stage generator returned an invalid attestation.'
    }
    $auditValues[$Matches[1]] = $Matches[2]
}
foreach ($requiredKey in @(
    'oracle_stage_manifest',
    'oracle_stage_mode',
    'oracle_source_commit',
    'oracle_release_id',
    'oracle_release_sha256',
    'oracle_bootstrap_sha256',
    'oracle_bootstrap_size'
)) {
    if (-not $auditValues.Contains($requiredKey)) {
        throw "Oracle stage generator omitted $requiredKey."
    }
}
if ($auditValues.oracle_stage_manifest -cne 'ok' -or
    $auditValues.oracle_stage_mode -cne 'audit' -or
    $auditValues.oracle_source_commit -cnotmatch '^[0-9a-f]{40}$' -or
    $auditValues.oracle_release_id -cne "v0.8.0-$($auditValues.oracle_source_commit.Substring(0, 7))" -or
    $auditValues.oracle_release_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
    $auditValues.oracle_bootstrap_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
    $auditValues.oracle_bootstrap_size -cnotmatch '^[1-9][0-9]{0,8}$') {
    throw 'Oracle stage generator attestation is not canonical.'
}

$manifestText = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $ManifestPath))
$manifest = $manifestText | ConvertFrom-Json
$uploads = @(
    [pscustomobject]@{
        Local = $SourceArchive
        RemoteName = 'source.tar.gz'
        Sha256 = [string]$manifest.layers.source.sha256
        Length = [long]$manifest.layers.source.size
        Mode = '600'
    }
    [pscustomobject]@{
        Local = $OverlayArchive
        RemoteName = 'private-overlay-no-config.tar'
        Sha256 = [string]$manifest.layers.overlay.sha256
        Length = [long]$manifest.layers.overlay.size
        Mode = '600'
    }
    [pscustomobject]@{
        Local = $Ctk3DistArchive
        RemoteName = 'ctk3-dist.tar'
        Sha256 = [string]$manifest.layers.ctk3Dist.sha256
        Length = [long]$manifest.layers.ctk3Dist.size
        Mode = '600'
    }
    [pscustomobject]@{
        Local = $DependenciesArchive
        RemoteName = 'node_modules.tar'
        Sha256 = [string]$manifest.layers.dependencies.sha256
        Length = [long]$manifest.layers.dependencies.size
        Mode = '600'
    }
    [pscustomobject]@{
        Local = $launcherPath
        RemoteName = 'clearra-oracle-release-deploy'
        Sha256 = [string]$manifest.tools.launcher.sha256
        Length = [long]$manifest.tools.launcher.size
        Mode = '600'
    }
    [pscustomobject]@{
        Local = $digesterPath
        RemoteName = 'clearra-release-tree-digest.py'
        Sha256 = [string]$manifest.tools.digester.sha256
        Length = [long]$manifest.tools.digester.size
        Mode = '600'
    }
)
foreach ($upload in $uploads) {
    Assert-FrozenInput -FrozenInput $upload
}

if ($AuditOnly) {
    'oracle_inactive_stage_invoker=audit-ok'
    "oracle_source_commit=$($auditValues.oracle_source_commit)"
    "oracle_release_id=$($auditValues.oracle_release_id)"
    "oracle_release_sha256=$($auditValues.oracle_release_sha256)"
    "oracle_bootstrap_sha256=$($auditValues.oracle_bootstrap_sha256)"
    "oracle_bootstrap_size=$($auditValues.oracle_bootstrap_size)"
    return
}

if ([string]::IsNullOrWhiteSpace($IdentityFile)) {
    throw 'An approved Oracle identity file is required outside AuditOnly.'
}
[void](Get-ExactLeaf -Path $IdentityFile -Label 'Oracle identity file')

$commonSshOptions = @(
    '-F', 'NUL',
    '-i', $IdentityFile,
    '-o', 'BatchMode=yes',
    '-o', 'IdentitiesOnly=yes',
    '-o', 'IdentityAgent=none',
    '-o', 'PreferredAuthentications=publickey',
    '-o', 'PasswordAuthentication=no',
    '-o', 'KbdInteractiveAuthentication=no',
    '-o', 'GSSAPIAuthentication=no',
    '-o', 'StrictHostKeyChecking=yes',
    '-o', "UserKnownHostsFile=$knownHostsPath",
    '-o', "GlobalKnownHostsFile=$knownHostsPath",
    '-o', 'HostKeyAlgorithms=ssh-ed25519',
    '-o', 'KexAlgorithms=curve25519-sha256',
    '-o', 'ProxyCommand=none',
    '-o', 'ProxyJump=none',
    '-o', 'CanonicalizeHostname=no',
    '-o', 'UpdateHostKeys=no',
    '-o', 'ClearAllForwardings=yes',
    '-o', 'RequestTTY=no',
    '-o', 'NumberOfPasswordPrompts=0',
    '-o', 'ControlMaster=no',
    '-o', 'ControlPath=none',
    '-o', 'ControlPersist=no',
    '-o', 'PermitLocalCommand=no',
    '-o', 'LogLevel=ERROR',
    '-o', 'ConnectTimeout=15'
)
$sshArguments = $commonSshOptions + @("$userName@$hostName")
$scpArguments = @('-q') + $commonSshOptions

function Invoke-ExactSsh {
    param(
        [Parameter(Mandatory = $true)]
        [string[]] $RemoteArguments
    )

    foreach ($argument in $RemoteArguments) {
        if ($argument -cnotmatch '^[A-Za-z0-9_./:%=@+-]{1,2048}$') {
            throw 'Remote bootstrap argument is outside the non-secret token grammar.'
        }
    }
    $output = @(& ssh @sshArguments @RemoteArguments)
    if ($LASTEXITCODE -ne 0) {
        throw "Oracle bootstrap command failed with exit code $LASTEXITCODE."
    }
    return $output
}

$stageNonce = [Convert]::ToHexString(
    [Security.Cryptography.RandomNumberGenerator]::GetBytes(32)
).ToLowerInvariant()
if ($stageNonce -cnotmatch '^[0-9a-f]{64}$') {
    throw 'Stage nonce generation failed.'
}
$commitPrefix = $auditValues.oracle_source_commit.Substring(0, 7)
$uploadRoot = "/home/ubuntu/.clearra-v080-upload-$commitPrefix-$stageNonce"
$localStageRoot = Join-Path ([IO.Path]::GetTempPath()) "clearra-oracle-stage-v080-$stageNonce"
$bootstrapPath = Join-Path $localStageRoot 'clearra-oracle-inactive-stage-v080'

try {
    [void](New-Item -ItemType Directory -Path $localStageRoot -ErrorAction Stop)
    $generated = @(& node $generatorPath --manifest $ManifestPath --output $bootstrapPath)
    if ($LASTEXITCODE -ne 0 -or $generated.Count -ne 7 -or
        $generated[1] -cne 'oracle_stage_mode=output') {
        throw 'Oracle one-shot bootstrap generation failed.'
    }
    $bootstrapItem = Get-ExactLeaf -Path $bootstrapPath -Label 'Generated Oracle bootstrap'
    if ($bootstrapItem.Length -ne [long]$auditValues.oracle_bootstrap_size -or
        (Get-ExactSha256 -Path $bootstrapPath) -cne $auditValues.oracle_bootstrap_sha256) {
        throw 'Generated Oracle bootstrap bytes do not match the audited bytes.'
    }
    $bootstrapBytes = [IO.File]::ReadAllBytes((Resolve-Path -LiteralPath $bootstrapPath))
    if ($bootstrapBytes -contains 13) {
        throw 'Generated Oracle bootstrap must contain LF line endings only.'
    }
    $allUploads = @($uploads) + @(
        [pscustomobject]@{
            Local = $bootstrapPath
            RemoteName = 'clearra-oracle-inactive-stage-v080'
            Sha256 = $auditValues.oracle_bootstrap_sha256
            Length = [long]$auditValues.oracle_bootstrap_size
            Mode = '700'
        }
    )

    [void](Invoke-ExactSsh @('/usr/bin/mkdir', '-m', '0700', '--', $uploadRoot))
    $uploadMetadata = @(Invoke-ExactSsh @('/usr/bin/stat', '-c', '%u:%g:%a', '--', $uploadRoot))
    $uploadResolved = @(Invoke-ExactSsh @('/usr/bin/readlink', '-f', '--', $uploadRoot))
    if ($uploadMetadata.Count -ne 1 -or $uploadMetadata[0] -cne '1001:1001:700' -or
        $uploadResolved.Count -ne 1 -or $uploadResolved[0] -cne $uploadRoot) {
        throw 'Remote upload directory authority does not match.'
    }

    foreach ($upload in $allUploads) {
        $remotePath = "$uploadRoot/$($upload.RemoteName)"
        & scp @scpArguments '--' $upload.Local "${userName}@${hostName}:$remotePath"
        if ($LASTEXITCODE -ne 0) {
            throw "Oracle upload failed: $($upload.RemoteName)"
        }
        [void](Invoke-ExactSsh @('/usr/bin/chmod', $upload.Mode, '--', $remotePath))
        $metadata = @(Invoke-ExactSsh @('/usr/bin/stat', '-c', '%u:%g:%a:%s', '--', $remotePath))
        if ($metadata.Count -ne 1 -or
            $metadata[0] -cne "1001:1001:$($upload.Mode):$($upload.Length)") {
            throw "Remote upload metadata does not match: $($upload.RemoteName)"
        }
        $readback = @(Invoke-ExactSsh @('/usr/bin/sha256sum', '--', $remotePath))
        if ($readback.Count -ne 1 -or $readback[0] -cnotmatch '^([0-9a-f]{64})  /' -or
            $Matches[1] -cne $upload.Sha256) {
            throw "Remote upload digest does not match: $($upload.RemoteName)"
        }
    }

    $bootstrapRemoteOutput = @(Invoke-ExactSsh @(
        'sudo', '-n', '/usr/bin/mktemp',
        "/usr/local/sbin/.clearra-oracle-inactive-stage-v080-$commitPrefix.XXXXXXXX"
    ))
    if ($bootstrapRemoteOutput.Count -ne 1 -or
        $bootstrapRemoteOutput[0] -cnotmatch "^/usr/local/sbin/\.clearra-oracle-inactive-stage-v080-$commitPrefix\.[A-Za-z0-9]{8}$") {
        throw 'Remote one-shot bootstrap path is invalid.'
    }
    $bootstrapRemote = $bootstrapRemoteOutput[0]
    [void](Invoke-ExactSsh @(
        'sudo', '-n', '/usr/bin/install', '-o', 'root', '-g', 'root', '-m', '0755', '--',
        "$uploadRoot/clearra-oracle-inactive-stage-v080", $bootstrapRemote
    ))
    $bootstrapMetadata = @(Invoke-ExactSsh @(
        'sudo', '-n', '/usr/bin/stat', '-c', '%u:%g:%a:%s', '--', $bootstrapRemote
    ))
    if ($bootstrapMetadata.Count -ne 1 -or
        $bootstrapMetadata[0] -cne "0:0:755:$($auditValues.oracle_bootstrap_size)") {
        throw 'Root-owned one-shot bootstrap metadata does not match.'
    }
    $bootstrapReadback = @(Invoke-ExactSsh @(
        'sudo', '-n', '/usr/bin/sha256sum', '--', $bootstrapRemote
    ))
    if ($bootstrapReadback.Count -ne 1 -or
        $bootstrapReadback[0] -cnotmatch '^([0-9a-f]{64})  /' -or
        $Matches[1] -cne $auditValues.oracle_bootstrap_sha256) {
        throw 'Root-owned one-shot bootstrap digest does not match.'
    }

    $attestation = @(Invoke-ExactSsh @(
        'sudo', '-n', $bootstrapRemote,
        '--nonce', $stageNonce,
        '--self-sha256', $auditValues.oracle_bootstrap_sha256,
        '--self-path', $bootstrapRemote
    ))
    $expectedAttestation = @(
        'oracle_inactive_stage=ready',
        "oracle_source_commit=$($auditValues.oracle_source_commit)",
        "oracle_release_id=$($auditValues.oracle_release_id)",
        "oracle_release_sha256=$($auditValues.oracle_release_sha256)",
        "oracle_launcher_sha256=$([string]$manifest.tools.launcher.sha256)",
        "oracle_tree_digester_sha256=$([string]$manifest.tools.digester.sha256)",
        "oracle_stage_nonce=$stageNonce"
    )
    if ($attestation.Count -ne $expectedAttestation.Count -or
        (Compare-Object -CaseSensitive -SyncWindow 0 $expectedAttestation $attestation)) {
        throw 'Oracle inactive-stage attestation does not match.'
    }

    $bootstrapBasename = [IO.Path]::GetFileName($bootstrapRemote)
    $bootstrapResidue = @(Invoke-ExactSsh @(
        'sudo', '-n', '/usr/bin/find', '/usr/local/sbin', '-maxdepth', '1',
        '-name', $bootstrapBasename, '-print'
    ))
    $uploadBasename = [IO.Path]::GetFileName($uploadRoot)
    $uploadResidue = @(Invoke-ExactSsh @(
        '/usr/bin/find', '/home/ubuntu', '-maxdepth', '1', '-name', $uploadBasename, '-print'
    ))
    if ($bootstrapResidue.Count -ne 0 -or $uploadResidue.Count -ne 0) {
        throw 'Successful Oracle staging left upload or bootstrap residue.'
    }

    $attestation
} catch {
    throw "Oracle inactive staging failed; preserve nonce $stageNonce for exact audit. $($_.Exception.Message)"
} finally {
    if (Test-Path -LiteralPath $localStageRoot) {
        $resolvedTemporaryRoot = [IO.Path]::GetFullPath($localStageRoot)
        $expectedTemporaryParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if ($resolvedTemporaryRoot.StartsWith($expectedTemporaryParent, [StringComparison]::OrdinalIgnoreCase) -and
            [IO.Path]::GetFileName($resolvedTemporaryRoot) -ceq "clearra-oracle-stage-v080-$stageNonce") {
            Remove-Item -LiteralPath $resolvedTemporaryRoot -Recurse -Force
        }
    }
}
