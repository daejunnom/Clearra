[CmdletBinding(PositionalBinding = $false, DefaultParameterSetName = 'LocalOverlay')]
param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9a-f]{40}$')]
    [string] $SourceCommit,

    [Parameter(Mandatory = $true)]
    [string] $SourceArchive,

    [Parameter(Mandatory = $true, ParameterSetName = 'LocalOverlay')]
    [string] $OverlayArchive,

    [Parameter(Mandatory = $true, ParameterSetName = 'RemoteOverlay')]
    [ValidatePattern('^/opt/clearra/sealed-release-inputs/private-overlay-no-config-[0-9a-f]{64}\.tar$')]
    [string] $RemoteOverlayArchive,

    [Parameter(Mandatory = $true, ParameterSetName = 'RemoteOverlay')]
    [ValidatePattern('^[0-9a-f]{64}$')]
    [string] $RemoteOverlaySha256,

    [Parameter(Mandatory = $true)]
    [string] $Ctk3DistArchive,

    [Parameter(Mandatory = $true)]
    [string] $DependenciesArchive,

    [Parameter(Mandatory = $true)]
    [string] $ManifestOutput,

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
$freezePath = Join-Path $PSScriptRoot 'clearra-oracle-freeze-v080'
$launcherPath = Join-Path $PSScriptRoot 'clearra-oracle-release-deploy-v080'
$digesterPath = Join-Path $PSScriptRoot 'clearra-release-tree-digest.py'
$generatorPath = Join-Path $PSScriptRoot 'create-inactive-stage-v080.mjs'

function Get-ExactLeaf {
    param(
        [Parameter(Mandatory = $true)][string] $Path,
        [Parameter(Mandatory = $true)][string] $Label
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

function Get-OracleHostPlatform {
    if ([Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [Runtime.InteropServices.OSPlatform]::Windows
    )) {
        return 'windows'
    }
    if ([Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [Runtime.InteropServices.OSPlatform]::Linux
    )) {
        return 'linux'
    }
    throw 'Oracle release tooling supports only Windows and Linux hosts.'
}

function Get-OracleSshConfigPath {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet('windows', 'linux')]
        [string] $Platform
    )
    if ($Platform -ceq 'windows') {
        return 'NUL'
    }
    return '/dev/null'
}

function Get-OraclePosixSyntaxAuditContract {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet('windows', 'linux')]
        [string] $Platform,

        [Parameter(Mandatory = $true)]
        [string] $Path
    )
    if ($Platform -ceq 'windows') {
        return [pscustomobject]@{
            ProjectionCommand = 'wsl.exe'
            ProjectionArguments = [string[]]@(
                '-e', '/usr/bin/wslpath', '-a', '--', $Path
            )
            SyntaxCommand = 'wsl.exe'
            SyntaxArguments = [string[]]@(
                '-e', '/usr/bin/dash', '-n', '--'
            )
        }
    }
    return [pscustomobject]@{
        ProjectionCommand = $null
        ProjectionArguments = [string[]]@()
        SyntaxCommand = '/usr/bin/dash'
        SyntaxArguments = [string[]]@('-n', '--', $Path)
    }
}

function Invoke-OraclePosixSyntaxAudit {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet('windows', 'linux')]
        [string] $Platform,

        [Parameter(Mandatory = $true)]
        [string] $Path,

        [Parameter(Mandatory = $true)]
        [string] $ProjectionError,

        [Parameter(Mandatory = $true)]
        [string] $SyntaxError
    )
    $contract = Get-OraclePosixSyntaxAuditContract -Platform $Platform -Path $Path
    $syntaxArguments = [string[]]@($contract.SyntaxArguments)
    if ($null -ne $contract.ProjectionCommand) {
        $projectionCommand = [string]$contract.ProjectionCommand
        $projectionArguments = [string[]]@($contract.ProjectionArguments)
        $projectedPath = @(& $projectionCommand @projectionArguments)
        if ($LASTEXITCODE -ne 0 -or $projectedPath.Count -ne 1 -or
            [string]::IsNullOrWhiteSpace($projectedPath[0])) {
            throw $ProjectionError
        }
        $syntaxArguments += [string]$projectedPath[0]
    }
    $syntaxCommand = [string]$contract.SyntaxCommand
    $null = & $syntaxCommand @syntaxArguments
    if ($LASTEXITCODE -ne 0) {
        throw $SyntaxError
    }
}

function Assert-CanonicalRemoteOverlayAuthority {
    param(
        [Parameter(Mandatory = $true)][string] $Path,
        [Parameter(Mandatory = $true)][string] $Sha256
    )
    if ($Sha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $Path -cne "/opt/clearra/sealed-release-inputs/private-overlay-no-config-$Sha256.tar") {
        throw 'Remote Oracle overlay authority is not canonical.'
    }
}

$remoteOverlayMode = $PSCmdlet.ParameterSetName -ceq 'RemoteOverlay'
if ($remoteOverlayMode) {
    Assert-CanonicalRemoteOverlayAuthority `
        -Path $RemoteOverlayArchive `
        -Sha256 $RemoteOverlaySha256
}
$localInputs = @(
    [pscustomobject]@{ Local = $SourceArchive; RemoteName = 'source.tar.gz'; Mode = '600'; Label = 'Exact source archive' },
    [pscustomobject]@{ Local = $Ctk3DistArchive; RemoteName = 'ctk3-dist.tar'; Mode = '600'; Label = 'CTK3 distribution archive' },
    [pscustomobject]@{ Local = $DependenciesArchive; RemoteName = 'node_modules.tar'; Mode = '600'; Label = 'Production dependency archive' },
    [pscustomobject]@{ Local = $launcherPath; RemoteName = 'clearra-oracle-release-deploy'; Mode = '600'; Label = 'Tracked v0.8 launcher' },
    [pscustomobject]@{ Local = $digesterPath; RemoteName = 'clearra-release-tree-digest.py'; Mode = '600'; Label = 'Tracked tree digester' },
    [pscustomobject]@{ Local = $freezePath; RemoteName = 'clearra-oracle-freeze-v080'; Mode = '700'; Label = 'Tracked freeze helper' }
)
if (-not $remoteOverlayMode) {
    $localInputs = @($localInputs[0]) + @(
        [pscustomobject]@{ Local = $OverlayArchive; RemoteName = 'private-overlay-no-config.tar'; Mode = '600'; Label = 'Private overlay archive' }
    ) + @($localInputs[1..($localInputs.Count - 1)])
}
foreach ($input in $localInputs) {
    $item = Get-ExactLeaf -Path $input.Local -Label $input.Label
    $input | Add-Member -NotePropertyName Length -NotePropertyValue ([long]$item.Length)
    $input | Add-Member -NotePropertyName Sha256 -NotePropertyValue (Get-ExactSha256 -Path $input.Local)
}
[void](Get-ExactLeaf -Path $knownHostsPath -Label 'Pinned Oracle host-key file')
[void](Get-ExactLeaf -Path $generatorPath -Label 'Oracle stage manifest generator')

$manifestFullPath = [IO.Path]::GetFullPath($ManifestOutput)
if (Test-Path -LiteralPath $manifestFullPath) {
    throw 'The Oracle manifest output already exists.'
}
$manifestParent = Split-Path -Parent $manifestFullPath
if (-not (Test-Path -LiteralPath $manifestParent -PathType Container)) {
    throw 'The Oracle manifest output parent directory is unavailable.'
}
$manifestParentItem = Get-Item -LiteralPath $manifestParent -Force
if (($manifestParentItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'The Oracle manifest output parent must not be a reparse point.'
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

$hostPlatform = Get-OracleHostPlatform
$sshConfigPath = Get-OracleSshConfigPath -Platform $hostPlatform
Invoke-OraclePosixSyntaxAudit `
    -Platform $hostPlatform `
    -Path $freezePath `
    -ProjectionError 'The tracked freeze helper could not be projected into WSL.' `
    -SyntaxError 'The tracked freeze helper failed its POSIX syntax audit.'

if ($AuditOnly) {
    'oracle_freeze_invoker=audit-ok'
    "oracle_source_commit=$SourceCommit"
    "oracle_release_id=v0.8.0-$($SourceCommit.Substring(0, 7))"
    "oracle_freeze_helper_sha256=$(Get-ExactSha256 -Path $freezePath)"
    return
}

if ([string]::IsNullOrWhiteSpace($IdentityFile)) {
    throw 'An approved Oracle identity file is required outside AuditOnly.'
}
[void](Get-ExactLeaf -Path $IdentityFile -Label 'Oracle identity file')

$commonSshOptions = @(
    '-F', $sshConfigPath,
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
    param([Parameter(Mandatory = $true)][string[]] $RemoteArguments)
    foreach ($argument in $RemoteArguments) {
        if ($argument -cnotmatch '^[A-Za-z0-9_./:%=@+-]{1,2048}$') {
            throw 'Remote freeze argument is outside the non-secret token grammar.'
        }
    }
    $output = @(& ssh @sshArguments @RemoteArguments)
    if ($LASTEXITCODE -ne 0) {
        throw "Oracle freeze command failed with exit code $LASTEXITCODE."
    }
    return $output
}

$freezeNonce = [Convert]::ToHexString(
    [Security.Cryptography.RandomNumberGenerator]::GetBytes(32)
).ToLowerInvariant()
$commitPrefix = $SourceCommit.Substring(0, 7)
$uploadRoot = "/home/ubuntu/.clearra-v080-freeze-$commitPrefix-$freezeNonce"
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) "clearra-oracle-freeze-v080-$freezeNonce"
$temporaryManifest = Join-Path $temporaryRoot 'oracle-inactive-stage-v080.json'

try {
    [void](New-Item -ItemType Directory -Path $temporaryRoot -ErrorAction Stop)
    [void](Invoke-ExactSsh @('/usr/bin/mkdir', '-m', '0700', '--', $uploadRoot))
    $uploadMetadata = @(Invoke-ExactSsh @('/usr/bin/stat', '-c', '%u:%g:%a', '--', $uploadRoot))
    $uploadResolved = @(Invoke-ExactSsh @('/usr/bin/readlink', '-f', '--', $uploadRoot))
    if ($uploadMetadata.Count -ne 1 -or $uploadMetadata[0] -cne '1001:1001:700' -or
        $uploadResolved.Count -ne 1 -or $uploadResolved[0] -cne $uploadRoot) {
        throw 'Remote freeze upload directory authority does not match.'
    }

    foreach ($input in $localInputs) {
        $remotePath = "$uploadRoot/$($input.RemoteName)"
        & scp @scpArguments '--' $input.Local "${userName}@${hostName}:$remotePath"
        if ($LASTEXITCODE -ne 0) {
            throw "Oracle freeze upload failed: $($input.RemoteName)"
        }
        [void](Invoke-ExactSsh @('/usr/bin/chmod', $input.Mode, '--', $remotePath))
        $metadata = @(Invoke-ExactSsh @('/usr/bin/stat', '-c', '%u:%g:%a:%s', '--', $remotePath))
        if ($metadata.Count -ne 1 -or $metadata[0] -cne "1001:1001:$($input.Mode):$($input.Length)") {
            throw "Remote freeze upload metadata does not match: $($input.RemoteName)"
        }
        $readback = @(Invoke-ExactSsh @('/usr/bin/sha256sum', '--', $remotePath))
        if ($readback.Count -ne 1 -or $readback[0] -cnotmatch '^([0-9a-f]{64})  /' -or
            $Matches[1] -cne $input.Sha256) {
            throw "Remote freeze upload digest does not match: $($input.RemoteName)"
        }
    }
    $helperRemoteOutput = @(Invoke-ExactSsh @(
        'sudo', '-n', '/usr/bin/mktemp',
        "/usr/local/sbin/.clearra-oracle-freeze-v080-$commitPrefix.XXXXXXXX"
    ))
    if ($helperRemoteOutput.Count -ne 1 -or
        $helperRemoteOutput[0] -cnotmatch "^/usr/local/sbin/\.clearra-oracle-freeze-v080-$commitPrefix\.[A-Za-z0-9]{8}$") {
        throw 'Remote root freeze helper path is invalid.'
    }
    $helperRemote = $helperRemoteOutput[0]
    [void](Invoke-ExactSsh @(
        'sudo', '-n', '/usr/bin/install', '-o', 'root', '-g', 'root', '-m', '0755', '--',
        "$uploadRoot/clearra-oracle-freeze-v080", $helperRemote
    ))
    $helperInput = $localInputs | Where-Object { $_.RemoteName -ceq 'clearra-oracle-freeze-v080' }
    $helperMetadata = @(Invoke-ExactSsh @('sudo', '-n', '/usr/bin/stat', '-c', '%u:%g:%a:%s', '--', $helperRemote))
    if ($helperMetadata.Count -ne 1 -or $helperMetadata[0] -cne "0:0:755:$($helperInput.Length)") {
        throw 'Root-owned Oracle freeze helper metadata does not match.'
    }

    $helperArguments = @(
        'sudo', '-n', $helperRemote,
        '--source-commit', $SourceCommit,
        '--nonce', $freezeNonce,
        '--self-sha256', $helperInput.Sha256,
        '--self-path', $helperRemote
    )
    if ($remoteOverlayMode) {
        $helperArguments += @(
            '--remote-overlay-archive', $RemoteOverlayArchive,
            '--remote-overlay-sha256', $RemoteOverlaySha256
        )
    }
    $attestation = @(Invoke-ExactSsh $helperArguments)
    if ($attestation.Count -ne 7) {
        throw 'Oracle freeze helper returned an invalid attestation cardinality.'
    }
    $values = [ordered]@{}
    foreach ($line in $attestation) {
        $separator = $line.IndexOf('=')
        if ($separator -le 0) { throw 'Oracle freeze helper returned a malformed attestation.' }
        $key = $line.Substring(0, $separator)
        $value = $line.Substring($separator + 1)
        if ($key -cnotmatch '^oracle_[a-z0-9_]+$' -or $values.Contains($key)) {
            throw 'Oracle freeze helper returned a duplicate or invalid key.'
        }
        $values[$key] = $value
    }
    $expectedKeys = @(
        'oracle_freeze', 'oracle_source_commit', 'oracle_release_id',
        'oracle_candidate_sha256', 'oracle_manifest_sha256',
        'oracle_manifest_size', 'oracle_manifest_base64'
    )
    if (@($values.Keys).Count -ne $expectedKeys.Count -or
        (Compare-Object -CaseSensitive -SyncWindow 0 $expectedKeys @($values.Keys))) {
        throw 'Oracle freeze helper returned unexpected attestation keys.'
    }
    if ($values.oracle_freeze -cne 'ready' -or
        $values.oracle_source_commit -cne $SourceCommit -or
        $values.oracle_release_id -cne "v0.8.0-$commitPrefix" -or
        $values.oracle_candidate_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $values.oracle_manifest_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $values.oracle_manifest_size -cnotmatch '^[1-9][0-9]{2,8}$' -or
        $values.oracle_manifest_base64 -cnotmatch '^[A-Za-z0-9+/]+={0,2}$') {
        throw 'Oracle freeze helper attestation values are invalid.'
    }
    try {
        $manifestBytes = [Convert]::FromBase64String($values.oracle_manifest_base64)
    } catch {
        throw 'Oracle freeze manifest encoding is invalid.'
    }
    if ($manifestBytes.Length -ne [long]$values.oracle_manifest_size -or
        $manifestBytes -contains 13) {
        throw 'Oracle freeze manifest byte contract is invalid.'
    }
    [IO.File]::WriteAllBytes($temporaryManifest, $manifestBytes)
    if ((Get-ExactSha256 -Path $temporaryManifest) -cne $values.oracle_manifest_sha256) {
        throw 'Oracle freeze manifest digest does not match.'
    }
    $generatorAudit = @(& node $generatorPath --manifest $temporaryManifest --audit)
    if ($LASTEXITCODE -ne 0 -or $generatorAudit.Count -ne 7 -or
        $generatorAudit[0] -cne 'oracle_stage_manifest=ok' -or
        $generatorAudit[2] -cne "oracle_source_commit=$SourceCommit" -or
        $generatorAudit[4] -cne "oracle_release_sha256=$($values.oracle_candidate_sha256)") {
        throw 'The independently frozen manifest failed the tracked stage generator audit.'
    }

    $stream = [IO.File]::Open($manifestFullPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $stream.Write($manifestBytes, 0, $manifestBytes.Length)
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
    }
    if ((Get-ExactSha256 -Path $manifestFullPath) -cne $values.oracle_manifest_sha256) {
        throw 'Published Oracle freeze manifest digest does not match.'
    }

    $helperBasename = [IO.Path]::GetFileName($helperRemote)
    $helperResidue = @(Invoke-ExactSsh @('sudo', '-n', '/usr/bin/find', '/usr/local/sbin', '-maxdepth', '1', '-name', $helperBasename, '-print'))
    $uploadBasename = [IO.Path]::GetFileName($uploadRoot)
    $uploadResidue = @(Invoke-ExactSsh @('/usr/bin/find', '/home/ubuntu', '-maxdepth', '1', '-name', $uploadBasename, '-print'))
    if ($helperResidue.Count -ne 0 -or $uploadResidue.Count -ne 0) {
        throw 'Successful Oracle freeze left remote upload or helper residue.'
    }

    'oracle_freeze_invoker=ready'
    "oracle_source_commit=$SourceCommit"
    "oracle_release_id=$($values.oracle_release_id)"
    "oracle_candidate_sha256=$($values.oracle_candidate_sha256)"
    "oracle_manifest_sha256=$($values.oracle_manifest_sha256)"
    "oracle_manifest_size=$($values.oracle_manifest_size)"
    "oracle_manifest_path=$manifestFullPath"
} catch {
    throw "Oracle freeze failed; preserve nonce $freezeNonce for exact audit. $($_.Exception.Message)"
} finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        $resolvedTemporaryRoot = [IO.Path]::GetFullPath($temporaryRoot)
        $expectedTemporaryParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if ($resolvedTemporaryRoot.StartsWith($expectedTemporaryParent, [StringComparison]::OrdinalIgnoreCase) -and
            [IO.Path]::GetFileName($resolvedTemporaryRoot) -ceq "clearra-oracle-freeze-v080-$freezeNonce") {
            Remove-Item -LiteralPath $resolvedTemporaryRoot -Recurse -Force
        }
    }
}
