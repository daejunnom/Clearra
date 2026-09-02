[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('capture-prestage-authority', 'cleanup-prestage-backup', 'capture-rollback-authority', 'verify-candidate', 'observe-candidate', 'classify-current-authority', 'restore-prior-and-verify')]
    [string] $Operation,

    [string] $ScriptReleaseId,

    [string] $ScriptReleaseSha256,

    [string] $PriorRevision,
    [string] $DeploymentNonce,
    [string] $Proof,
    [string] $SourceCommit,
    [string] $CandidateUrl,
    [string] $CandidateRevision,
    [string] $OracleReleaseId,
    [string] $OracleReleaseSha256,
    [string] $OracleSettingsSha256,
    [string] $VerifiedAfter,
    [string] $PriorRelease,
    [string] $PriorReleaseId,
    [string] $PriorReleaseSha256,
    [string] $PriorSettingsBackup,
    [string] $PriorSettingsSha256,
    [string] $PriorRuntimeAuthorityKind,
    [string] $PriorRuntimeAuthoritySha256,
    [string] $PriorJobUrl,
    [string] $EvidenceOutput,
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
$launcherPath = Join-Path $PSScriptRoot 'clearra-oracle-release-deploy-v080'
$bundleManifestGeneratorPath = Join-Path $PSScriptRoot 'create-prestage-helper-bundle.mjs'
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$remoteLauncherPath = '/usr/local/sbin/clearra-oracle-release-deploy'
$releaseDeployLockPath = '/run/lock/clearra-oracle-release-deploy.lock'
$releaseIdPattern = '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$'
$candidateReleaseIdPattern = '^v0\.8\.0-[0-9a-f]{7}$'
$sha256Pattern = '^[0-9a-f]{64}$'
$commitPattern = '^[0-9a-f]{40}$'
$runtimeAuthorityKinds = @(
    'clearra.rollback.runtime-identity.v1',
    'clearra.rollback.legacy-health-no-runtime.v1'
)

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

function Get-TextSha256 {
    param([Parameter(Mandatory = $true)][string] $Text)
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes($Text)
        return [Convert]::ToHexString($algorithm.ComputeHash($bytes)).ToLowerInvariant()
    } finally {
        $algorithm.Dispose()
    }
}

function Get-PrestageHelperBundleManifest {
    param(
        [Parameter(Mandatory = $true)][string] $SourceCommit,
        [Parameter(Mandatory = $true)][string] $DeploymentNonce,
        [Parameter(Mandatory = $true)]
        [ValidateSet('capture-prestage-authority', 'cleanup-prestage-backup')]
        [string] $Operation
    )
    $output = @(& node $bundleManifestGeneratorPath `
        --repository-root $repositoryRoot `
        --source-commit $SourceCommit `
        --deployment-nonce $DeploymentNonce `
        --operation $Operation)
    if ($LASTEXITCODE -ne 0 -or $output.Count -ne 1) {
        throw 'Oracle prestage helper manifest generation failed.'
    }
    try {
        $manifest = $output[0] | ConvertFrom-Json -DateKind String
    } catch {
        throw 'Oracle prestage helper manifest is not valid JSON.'
    }
    $expectedKeys = @(
        'schema_id', 'source_commit', 'deployment_nonce', 'operation',
        'files', 'file_count', 'total_size', 'bundle_sha256'
    )
    $actualKeys = @($manifest.PSObject.Properties.Name)
    if ($actualKeys.Count -ne $expectedKeys.Count -or
        (Compare-Object -CaseSensitive -SyncWindow 0 $expectedKeys $actualKeys) -or
        $manifest.schema_id -cne 'clearra.oracle.prestage-helper-bundle.v1' -or
        $manifest.source_commit -cne $SourceCommit -or
        $manifest.deployment_nonce -cne $DeploymentNonce -or
        $manifest.operation -cne $Operation -or
        $manifest.bundle_sha256 -cnotmatch '^[0-9a-f]{64}$' -or
        -not (Test-JsonSafePositiveInteger $manifest.file_count) -or
        [long]$manifest.file_count -ne 4 -or
        -not (Test-JsonSafePositiveInteger $manifest.total_size) -or
        [long]$manifest.total_size -gt 4194304) {
        throw 'Oracle prestage helper manifest has an invalid closed authority.'
    }
    $expectedPaths = @(
        'apps/clearra-discord-bot/scripts/capture-oracle-rollback-authority.mjs',
        'apps/clearra-discord-bot/scripts/oracle-runtime-authority.mjs',
        'apps/clearra-discord-bot/scripts/release-tree-digest.mjs',
        'apps/clearra-discord-bot/src/job-service/runtime-identity.mjs'
    )
    $files = @($manifest.files)
    if ($files.Count -ne $expectedPaths.Count) {
        throw 'Oracle prestage helper manifest file count drifted.'
    }
    [long]$totalSize = 0
    for ($index = 0; $index -lt $files.Count; $index += 1) {
        $entry = $files[$index]
        $entryKeys = @($entry.PSObject.Properties.Name)
        $wantedEntryKeys = @('path', 'size', 'sha256', 'mode')
        if ($entryKeys.Count -ne $wantedEntryKeys.Count -or
            (Compare-Object -CaseSensitive -SyncWindow 0 $wantedEntryKeys $entryKeys) -or
            $entry.path -cne $expectedPaths[$index] -or
            $entry.mode -cne '0644' -or
            -not (Test-JsonSafePositiveInteger $entry.size) -or
            [long]$entry.size -gt 1048576 -or
            $entry.sha256 -cnotmatch '^[0-9a-f]{64}$') {
            throw 'Oracle prestage helper manifest file authority drifted.'
        }
        $totalSize += [long]$entry.size
    }
    if ($totalSize -ne [long]$manifest.total_size) {
        throw 'Oracle prestage helper manifest total size drifted.'
    }
    return $manifest
}

function Sort-OrdinalStrings {
    param([Parameter(Mandatory = $true)][object[]] $Values)
    [string[]]$result = @($Values | ForEach-Object { [string]$_ })
    [Array]::Sort($result, [StringComparer]::Ordinal)
    return $result
}

function Assert-ExactOrdinalSet {
    param(
        [Parameter(Mandatory = $true)][object[]] $Actual,
        [Parameter(Mandatory = $true)][object[]] $Expected,
        [Parameter(Mandatory = $true)][string] $Label
    )
    [string[]]$actualSorted = @(Sort-OrdinalStrings -Values $Actual)
    [string[]]$expectedSorted = @(Sort-OrdinalStrings -Values $Expected)
    if ($actualSorted.Count -ne $expectedSorted.Count -or
        (Compare-Object -CaseSensitive -SyncWindow 0 $expectedSorted $actualSorted)) {
        throw "$Label differs from its closed set."
    }
}

function Test-JsonSafePositiveInteger {
    param($Value)
    return (($Value -is [int]) -or ($Value -is [long])) -and
        ([long]$Value -ge 1) -and
        ([long]$Value -le 9007199254740991)
}

function Require-Match {
    param(
        [AllowEmptyString()][string] $Value,
        [Parameter(Mandatory = $true)][string] $Pattern,
        [Parameter(Mandatory = $true)][string] $Label
    )
    if ([string]::IsNullOrEmpty($Value) -or $Value -cnotmatch $Pattern) {
        throw "$Label is invalid."
    }
    return $Value
}

function Get-CanonicalOrigin {
    param([AllowEmptyString()][string] $Value)
    try {
        $uri = [Uri]::new($Value, [UriKind]::Absolute)
    } catch {
        throw 'Candidate URL must be a canonical credential-free HTTPS origin.'
    }
    if ($uri.Scheme -cne 'https' -or
        -not [string]::IsNullOrEmpty($uri.UserInfo) -or
        -not [string]::IsNullOrEmpty($uri.Query) -or
        -not [string]::IsNullOrEmpty($uri.Fragment) -or
        $uri.AbsolutePath -cne '/') {
        throw 'Candidate URL must be a canonical credential-free HTTPS origin.'
    }
    $canonical = "https://$($uri.Authority)"
    if ($Value -cne $canonical -and $Value -cne "$canonical/") {
        throw 'Candidate URL must be a canonical credential-free HTTPS origin.'
    }
    return $canonical
}

function Get-CanonicalJobUrl {
    param([AllowEmptyString()][string] $Value)
    try {
        $uri = [Uri]::new($Value, [UriKind]::Absolute)
    } catch {
        throw 'Prior job URL must be a canonical credential-free HTTPS /jobs URL.'
    }
    $canonical = "https://$($uri.Authority)/jobs"
    if ($uri.Scheme -cne 'https' -or
        -not [string]::IsNullOrEmpty($uri.UserInfo) -or
        -not [string]::IsNullOrEmpty($uri.Query) -or
        -not [string]::IsNullOrEmpty($uri.Fragment) -or
        $Value -cne $canonical) {
        throw 'Prior job URL must be a canonical credential-free HTTPS /jobs URL.'
    }
    return $canonical
}

function Get-CanonicalTimestamp {
    param($Value)
    $parsed = [DateTimeOffset]::MinValue
    if ($Value -is [DateTimeOffset]) {
        $parsed = [DateTimeOffset]$Value
    } elseif ($Value -is [DateTime]) {
        $parsed = [DateTimeOffset]([DateTime]$Value)
    } elseif ($Value -is [string] -and
        -not [string]::IsNullOrEmpty($Value) -and
        [DateTimeOffset]::TryParse(
            $Value,
            [Globalization.CultureInfo]::InvariantCulture,
            [Globalization.DateTimeStyles]::RoundtripKind,
            [ref]$parsed
        )) {
        # Parsed above.
    } else {
        throw 'Verified-after timestamp is invalid.'
    }
    return $parsed.ToUniversalTime().ToString(
        'yyyy-MM-ddTHH:mm:ss.fffZ',
        [Globalization.CultureInfo]::InvariantCulture
    )
}

function Assert-UnusedArguments {
    param(
        [Parameter(Mandatory = $true)][hashtable] $Values,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()][string[]] $Allowed
    )
    foreach ($entry in $Values.GetEnumerator()) {
        if ($entry.Key -notin $Allowed -and -not [string]::IsNullOrEmpty([string]$entry.Value)) {
            throw "Argument $($entry.Key) is not valid for operation $Operation."
        }
    }
}

function Assert-EvidenceOutputPath {
    param([Parameter(Mandatory = $true)][string] $Path)
    if (-not [IO.Path]::IsPathFullyQualified($Path)) {
        throw 'Oracle evidence output must be an absolute path.'
    }
    $target = [IO.Path]::GetFullPath($Path)
    if (Test-Path -LiteralPath $target) {
        throw 'Oracle evidence output must be a new path.'
    }
    $current = [IO.Path]::GetDirectoryName($target)
    if ([string]::IsNullOrWhiteSpace($current)) {
        throw 'Oracle evidence output parent is invalid.'
    }
    while ($true) {
        if (-not (Test-Path -LiteralPath $current -PathType Container)) {
            throw 'Oracle evidence output parent must already exist.'
        }
        $item = Get-Item -LiteralPath $current -Force
        if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw 'Oracle evidence output path must not traverse a reparse point.'
        }
        $parent = [IO.Path]::GetDirectoryName($current)
        if ([string]::IsNullOrEmpty($parent) -or $parent -ceq $current) {
            break
        }
        $current = $parent
    }
    return $target
}

function ConvertTo-CanonicalJson {
    param($Value)
    if ($null -eq $Value) {
        return 'null'
    }
    if ($Value -is [bool]) {
        if ($Value) { return 'true' }
        return 'false'
    }
    if ($Value -is [string]) {
        return ($Value | ConvertTo-Json -Compress)
    }
    if (($Value -is [int]) -or ($Value -is [long])) {
        return ([Convert]::ToString([long]$Value, [Globalization.CultureInfo]::InvariantCulture))
    }
    if ($Value -is [Array]) {
        $items = @($Value | ForEach-Object { ConvertTo-CanonicalJson -Value $_ })
        return '[' + ($items -join ',') + ']'
    }
    if ($Value -is [Collections.IDictionary]) {
        [string[]]$names = @($Value.Keys | ForEach-Object { [string]$_ })
        [Array]::Sort($names, [StringComparer]::Ordinal)
        $fields = @($names | ForEach-Object {
            (ConvertTo-CanonicalJson -Value $_) + ':' +
                (ConvertTo-CanonicalJson -Value $Value[$_])
        })
        return '{' + ($fields -join ',') + '}'
    }
    if ($Value -is [System.Management.Automation.PSCustomObject]) {
        [string[]]$names = @($Value.PSObject.Properties.Name)
        [Array]::Sort($names, [StringComparer]::Ordinal)
        $fields = @($names | ForEach-Object {
            (ConvertTo-CanonicalJson -Value $_) + ':' +
                (ConvertTo-CanonicalJson -Value $Value.$_)
        })
        return '{' + ($fields -join ',') + '}'
    }
    throw 'Oracle evidence contains an unsupported JSON value type.'
}

function Write-CanonicalEvidenceOutput {
    param(
        [Parameter(Mandatory = $true)][string] $Path,
        [Parameter(Mandatory = $true)] $Value
    )
    $json = ConvertTo-CanonicalJson -Value $Value
    $bytes = [Text.UTF8Encoding]::new($false).GetBytes("$json`n")
    $stream = [IO.FileStream]::new(
        $Path,
        [IO.FileMode]::CreateNew,
        [IO.FileAccess]::Write,
        [IO.FileShare]::None,
        4096,
        [IO.FileOptions]::WriteThrough
    )
    try {
        $stream.Write($bytes, 0, $bytes.Length)
        $stream.Flush($true)
    } finally {
        $stream.Dispose()
    }
}

[void](Get-ExactLeaf -Path $knownHostsPath -Label 'Pinned Oracle host-key file')
[void](Get-ExactLeaf -Path $launcherPath -Label 'Tracked Oracle deployment launcher')
[void](Get-ExactLeaf -Path $bundleManifestGeneratorPath -Label 'Tracked prestage helper manifest generator')
if ((Get-ExactSha256 -Path $knownHostsPath) -cne $expectedKnownHostsSha256) {
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
    -Path $launcherPath `
    -ProjectionError 'The tracked Oracle deployment launcher could not be projected into WSL.' `
    -SyntaxError 'The tracked Oracle deployment launcher failed its POSIX syntax audit.'

[void](Require-Match -Value $DeploymentNonce -Pattern $sha256Pattern -Label 'Deployment nonce')
if ($Operation -notin @('capture-prestage-authority', 'cleanup-prestage-backup')) {
    [void](Require-Match -Value $ScriptReleaseId -Pattern $candidateReleaseIdPattern -Label 'Script release ID')
    [void](Require-Match -Value $ScriptReleaseSha256 -Pattern $sha256Pattern -Label 'Script release SHA-256')
}

$operationValues = @{
    PriorRevision = $PriorRevision
    Proof = $Proof
    SourceCommit = $SourceCommit
    CandidateUrl = $CandidateUrl
    CandidateRevision = $CandidateRevision
    OracleReleaseId = $OracleReleaseId
    OracleReleaseSha256 = $OracleReleaseSha256
    OracleSettingsSha256 = $OracleSettingsSha256
    VerifiedAfter = $VerifiedAfter
    PriorRelease = $PriorRelease
    PriorReleaseId = $PriorReleaseId
    PriorReleaseSha256 = $PriorReleaseSha256
    PriorSettingsBackup = $PriorSettingsBackup
    PriorSettingsSha256 = $PriorSettingsSha256
    PriorRuntimeAuthorityKind = $PriorRuntimeAuthorityKind
    PriorRuntimeAuthoritySha256 = $PriorRuntimeAuthoritySha256
    PriorJobUrl = $PriorJobUrl
}
$usesPrestageHelperBundle = $Operation -in @(
    'capture-prestage-authority', 'cleanup-prestage-backup'
)
$remoteArguments = if ($usesPrestageHelperBundle) {
    @()
} else {
    @(
        'sudo', '-n', $remoteLauncherPath,
        '--operation', $Operation,
        '--script-release-id', $ScriptReleaseId,
        '--script-release-sha256', $ScriptReleaseSha256
    )
}

switch ($Operation) {
    'capture-prestage-authority' {
        Assert-UnusedArguments -Values $operationValues -Allowed @(
            'PriorRevision', 'PriorRuntimeAuthorityKind', 'SourceCommit'
        )
        [void](Require-Match -Value $SourceCommit -Pattern $commitPattern -Label 'Source commit')
        [void](Require-Match -Value $PriorRevision -Pattern $releaseIdPattern -Label 'Prior Cloud revision')
        if ($PriorRuntimeAuthorityKind -cnotin $runtimeAuthorityKinds) {
            throw 'Prior runtime authority kind is invalid.'
        }
        $remoteArguments += @(
            '--prior-revision', $PriorRevision,
            '--prior-runtime-authority-kind', $PriorRuntimeAuthorityKind,
            '--deployment-nonce', $DeploymentNonce
        )
    }
    'cleanup-prestage-backup' {
        Assert-UnusedArguments -Values $operationValues -Allowed @('SourceCommit')
        [void](Require-Match -Value $SourceCommit -Pattern $commitPattern -Label 'Source commit')
        $remoteArguments += @('--cleanup-deployment-nonce', $DeploymentNonce)
    }
    'capture-rollback-authority' {
        Assert-UnusedArguments -Values $operationValues -Allowed @(
            'PriorRevision', 'PriorRuntimeAuthorityKind'
        )
        [void](Require-Match -Value $PriorRevision -Pattern $releaseIdPattern -Label 'Prior Cloud revision')
        if ($PriorRuntimeAuthorityKind -cnotin $runtimeAuthorityKinds) {
            throw 'Prior runtime authority kind is invalid.'
        }
        $remoteArguments += @(
            '--prior-revision', $PriorRevision,
            '--prior-runtime-authority-kind', $PriorRuntimeAuthorityKind,
            '--deployment-nonce', $DeploymentNonce
        )
    }
    'verify-candidate' {
        Assert-UnusedArguments -Values $operationValues -Allowed @(
            'Proof', 'SourceCommit', 'CandidateUrl', 'CandidateRevision',
            'OracleReleaseId', 'OracleReleaseSha256', 'OracleSettingsSha256',
            'VerifiedAfter'
        )
        [void](Require-Match -Value $SourceCommit -Pattern $commitPattern -Label 'Source commit')
        $commitPrefix = $SourceCommit.Substring(0, 7)
        if ($ScriptReleaseId -cne "v0.8.0-$commitPrefix") {
            throw 'Script release ID does not match the source commit.'
        }
        $canonicalCandidateUrl = Get-CanonicalOrigin -Value $CandidateUrl
        if ($CandidateRevision -cne "clearra-current-job-v080-$commitPrefix") {
            throw 'Candidate revision does not match the source commit.'
        }
        if ($OracleReleaseId -cne $ScriptReleaseId) {
            throw 'Oracle release ID does not match the script release.'
        }
        if ($OracleReleaseSha256 -cne $ScriptReleaseSha256) {
            throw 'Oracle release SHA-256 does not match the script release.'
        }
        [void](Require-Match -Value $OracleSettingsSha256 -Pattern $sha256Pattern -Label 'Oracle settings SHA-256')
        $canonicalVerifiedAfter = Get-CanonicalTimestamp -Value $VerifiedAfter
        $expectedProof = "/run/clearra-deploy/clearra-oracle-candidate-$DeploymentNonce.json"
        if ($Proof -cne $expectedProof) {
            throw 'Candidate proof path does not match the deployment nonce.'
        }
        $remoteArguments += @(
            '--proof', $Proof,
            '--source-commit', $SourceCommit,
            '--candidate-url', $canonicalCandidateUrl,
            '--candidate-revision', $CandidateRevision,
            '--oracle-release-id', $OracleReleaseId,
            '--oracle-release-sha256', $OracleReleaseSha256,
            '--oracle-settings-sha256', $OracleSettingsSha256,
            '--deployment-nonce', $DeploymentNonce,
            '--verified-after', $canonicalVerifiedAfter
        )
    }
    'observe-candidate' {
        Assert-UnusedArguments -Values $operationValues -Allowed @(
            'SourceCommit', 'CandidateUrl', 'CandidateRevision',
            'OracleReleaseId', 'OracleReleaseSha256', 'OracleSettingsSha256',
            'VerifiedAfter'
        )
        [void](Require-Match -Value $SourceCommit -Pattern $commitPattern -Label 'Source commit')
        $commitPrefix = $SourceCommit.Substring(0, 7)
        if ($ScriptReleaseId -cne "v0.8.0-$commitPrefix") {
            throw 'Script release ID does not match the source commit.'
        }
        $canonicalCandidateUrl = Get-CanonicalOrigin -Value $CandidateUrl
        if ($CandidateRevision -cne "clearra-current-job-v080-$commitPrefix") {
            throw 'Candidate revision does not match the source commit.'
        }
        if ($OracleReleaseId -cne $ScriptReleaseId) {
            throw 'Oracle release ID does not match the script release.'
        }
        if ($OracleReleaseSha256 -cne $ScriptReleaseSha256) {
            throw 'Oracle release SHA-256 does not match the script release.'
        }
        [void](Require-Match -Value $OracleSettingsSha256 -Pattern $sha256Pattern -Label 'Oracle settings SHA-256')
        $canonicalVerifiedAfter = Get-CanonicalTimestamp -Value $VerifiedAfter
        $remoteArguments += @(
            '--source-commit', $SourceCommit,
            '--candidate-url', $canonicalCandidateUrl,
            '--candidate-revision', $CandidateRevision,
            '--oracle-release-id', $OracleReleaseId,
            '--oracle-release-sha256', $OracleReleaseSha256,
            '--oracle-settings-sha256', $OracleSettingsSha256,
            '--deployment-nonce', $DeploymentNonce,
            '--verified-after', $canonicalVerifiedAfter
        )
    }
    'classify-current-authority' {
        Assert-UnusedArguments -Values $operationValues -Allowed @(
            'PriorRevision', 'SourceCommit', 'CandidateUrl', 'CandidateRevision',
            'OracleReleaseId', 'OracleReleaseSha256', 'OracleSettingsSha256',
            'PriorRelease', 'PriorReleaseId', 'PriorReleaseSha256',
            'PriorSettingsSha256', 'PriorRuntimeAuthorityKind',
            'PriorRuntimeAuthoritySha256', 'PriorJobUrl'
        )
        [void](Require-Match -Value $SourceCommit -Pattern $commitPattern -Label 'Source commit')
        $commitPrefix = $SourceCommit.Substring(0, 7)
        if ($ScriptReleaseId -cne "v0.8.0-$commitPrefix" -or
            $OracleReleaseId -cne $ScriptReleaseId -or
            $OracleReleaseSha256 -cne $ScriptReleaseSha256) {
            throw 'Candidate Oracle authority does not match the script release.'
        }
        $canonicalCandidateUrl = Get-CanonicalOrigin -Value $CandidateUrl
        if ($CandidateRevision -cne "clearra-current-job-v080-$commitPrefix") {
            throw 'Candidate revision does not match the source commit.'
        }
        [void](Require-Match -Value $OracleSettingsSha256 -Pattern $sha256Pattern -Label 'Oracle settings SHA-256')
        [void](Require-Match -Value $PriorRevision -Pattern $releaseIdPattern -Label 'Prior Cloud revision')
        [void](Require-Match -Value $PriorReleaseId -Pattern $releaseIdPattern -Label 'Prior Oracle release ID')
        if ($PriorRelease -cne "/opt/clearra/releases/$PriorReleaseId") {
            throw 'Prior Oracle release path does not match its release ID.'
        }
        [void](Require-Match -Value $PriorReleaseSha256 -Pattern $sha256Pattern -Label 'Prior Oracle release SHA-256')
        [void](Require-Match -Value $PriorSettingsSha256 -Pattern $sha256Pattern -Label 'Prior Oracle settings SHA-256')
        [void](Require-Match -Value $PriorRuntimeAuthoritySha256 -Pattern $sha256Pattern -Label 'Prior runtime authority SHA-256')
        if ($PriorRuntimeAuthorityKind -cnotin $runtimeAuthorityKinds) {
            throw 'Prior runtime authority kind is invalid.'
        }
        $canonicalPriorJobUrl = Get-CanonicalJobUrl -Value $PriorJobUrl
        $remoteArguments += @(
            '--source-commit', $SourceCommit,
            '--candidate-url', $canonicalCandidateUrl,
            '--candidate-revision', $CandidateRevision,
            '--oracle-release-id', $OracleReleaseId,
            '--oracle-release-sha256', $OracleReleaseSha256,
            '--oracle-settings-sha256', $OracleSettingsSha256,
            '--prior-release', $PriorRelease,
            '--prior-release-id', $PriorReleaseId,
            '--prior-release-sha256', $PriorReleaseSha256,
            '--prior-settings-sha256', $PriorSettingsSha256,
            '--prior-runtime-authority-kind', $PriorRuntimeAuthorityKind,
            '--prior-runtime-authority-sha256', $PriorRuntimeAuthoritySha256,
            '--prior-job-url', $canonicalPriorJobUrl,
            '--prior-revision', $PriorRevision,
            '--deployment-nonce', $DeploymentNonce
        )
    }
    'restore-prior-and-verify' {
        Assert-UnusedArguments -Values $operationValues -Allowed @(
            'PriorRevision', 'Proof', 'VerifiedAfter', 'PriorRelease',
            'PriorReleaseId', 'PriorReleaseSha256', 'PriorSettingsBackup',
            'PriorSettingsSha256', 'PriorRuntimeAuthorityKind',
            'PriorRuntimeAuthoritySha256', 'PriorJobUrl'
        )
        [void](Require-Match -Value $PriorRevision -Pattern $releaseIdPattern -Label 'Prior Cloud revision')
        [void](Require-Match -Value $PriorReleaseId -Pattern $releaseIdPattern -Label 'Prior Oracle release ID')
        if ($PriorRelease -cne "/opt/clearra/releases/$PriorReleaseId") {
            throw 'Prior Oracle release path does not match its release ID.'
        }
        [void](Require-Match -Value $PriorReleaseSha256 -Pattern $sha256Pattern -Label 'Prior Oracle release SHA-256')
        [void](Require-Match -Value $PriorSettingsSha256 -Pattern $sha256Pattern -Label 'Prior Oracle settings SHA-256')
        [void](Require-Match -Value $PriorRuntimeAuthoritySha256 -Pattern $sha256Pattern -Label 'Prior runtime authority SHA-256')
        if ($PriorRuntimeAuthorityKind -cnotin $runtimeAuthorityKinds) {
            throw 'Prior runtime authority kind is invalid.'
        }
        $expectedSettingsBackup = "/etc/clearra-gateway/settings.pre-v0.8.0-$DeploymentNonce"
        if ($PriorSettingsBackup -cne $expectedSettingsBackup) {
            throw 'Prior Oracle settings backup does not match the deployment nonce.'
        }
        $canonicalPriorJobUrl = Get-CanonicalJobUrl -Value $PriorJobUrl
        $canonicalVerifiedAfter = Get-CanonicalTimestamp -Value $VerifiedAfter
        $expectedProof = "/run/clearra-deploy/clearra-oracle-rollback-$DeploymentNonce.json"
        if ($Proof -cne $expectedProof) {
            throw 'Rollback proof path does not match the deployment nonce.'
        }
        $remoteArguments += @(
            '--prior-release', $PriorRelease,
            '--prior-release-id', $PriorReleaseId,
            '--prior-release-sha256', $PriorReleaseSha256,
            '--prior-settings-backup', $PriorSettingsBackup,
            '--prior-settings-sha256', $PriorSettingsSha256,
            '--prior-runtime-authority-kind', $PriorRuntimeAuthorityKind,
            '--prior-runtime-authority-sha256', $PriorRuntimeAuthoritySha256,
            '--prior-job-url', $canonicalPriorJobUrl,
            '--prior-revision', $PriorRevision,
            '--proof', $Proof,
            '--deployment-nonce', $DeploymentNonce,
            '--verified-after', $canonicalVerifiedAfter
        )
    }
}

$evidenceOutputPath = $null
if (-not [string]::IsNullOrWhiteSpace($EvidenceOutput)) {
    if ($Operation -notin @('capture-prestage-authority', 'capture-rollback-authority', 'observe-candidate', 'classify-current-authority')) {
        throw 'Oracle evidence output is only valid for capture, observation, or classification.'
    }
    if ($AuditOnly) {
        throw 'Oracle evidence output is unavailable in AuditOnly.'
    }
    $evidenceOutputPath = Assert-EvidenceOutputPath -Path $EvidenceOutput
}

$prestageManifest = if ($usesPrestageHelperBundle) {
    Get-PrestageHelperBundleManifest `
        -SourceCommit $SourceCommit `
        -DeploymentNonce $DeploymentNonce `
        -Operation $Operation
} else { $null }
$prestageOperationSlug = if ($Operation -ceq 'capture-prestage-authority') {
    'capture'
} elseif ($Operation -ceq 'cleanup-prestage-backup') {
    'cleanup'
} else { $null }
$prestageRoot = if ($usesPrestageHelperBundle) {
    "/opt/clearra/.v080-prestage-helper-$DeploymentNonce-$prestageOperationSlug"
} else { $null }
$prestageMain = if ($usesPrestageHelperBundle) {
    "$prestageRoot/apps/clearra-discord-bot/scripts/capture-oracle-rollback-authority.mjs"
} else { $null }
$auditedRemoteArguments = if ($usesPrestageHelperBundle) {
    @(
        'sudo', '-n', '/usr/bin/flock', '-n', $releaseDeployLockPath,
        '/usr/bin/env', '-i',
        'PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin',
        'HOME=/root', '/usr/bin/node', $prestageMain
    ) + $remoteArguments
} else { $remoteArguments }

foreach ($argument in $auditedRemoteArguments) {
    if ($argument -cnotmatch '^[A-Za-z0-9_./:%=@+-]{1,2048}$') {
        throw 'Oracle deployment argument is outside the non-secret token grammar.'
    }
}

if ($AuditOnly) {
    'oracle_release_deploy_invoker=audit-ok'
    "oracle_operation=$Operation"
    "oracle_remote_argument_count=$($auditedRemoteArguments.Count)"
    "oracle_remote_arguments_sha256=$(Get-TextSha256 -Text (($auditedRemoteArguments -join "`n") + "`n"))"
    return
}

if ([string]::IsNullOrWhiteSpace($IdentityFile)) {
    $IdentityFile = $env:CLEARRA_ORACLE_IDENTITY_FILE
}
if ([string]::IsNullOrWhiteSpace($IdentityFile)) {
    throw 'An approved Oracle identity file is required outside AuditOnly.'
}
# Identity authority is intentionally limited to a leaf/non-reparse check. The
# wrapper never opens, hashes, copies, or prints the identity file.
if (-not (Test-Path -LiteralPath $IdentityFile -PathType Leaf)) {
    throw 'Oracle identity file is unavailable.'
}
$identityItem = Get-Item -LiteralPath $IdentityFile -Force
if (($identityItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'Oracle identity file must not be a reparse point.'
}

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

function Invoke-ExactSshResult {
    param([Parameter(Mandatory = $true)][string[]] $RemoteArguments)
    foreach ($argument in $RemoteArguments) {
        if ($argument -cnotmatch '^[A-Za-z0-9_./:%=@+-]{1,2048}$') {
            throw 'Oracle remote argument is outside the non-secret token grammar.'
        }
    }
    $result = @(& ssh @sshArguments @RemoteArguments)
    return [pscustomobject]@{
        ExitCode = [int]$LASTEXITCODE
        Output = [object[]]@($result)
    }
}

function Invoke-ExactSsh {
    param([Parameter(Mandatory = $true)][string[]] $RemoteArguments)
    $invocation = Invoke-ExactSshResult -RemoteArguments $RemoteArguments
    if ($invocation.ExitCode -ne 0) {
        throw "Oracle deployment command failed with exit code $($invocation.ExitCode)."
    }
    return @($invocation.Output)
}

function Invoke-PrestageHelperBundle {
    param(
        [Parameter(Mandatory = $true)][pscustomobject] $Manifest,
        [Parameter(Mandatory = $true)][string] $OperationSlug,
        [Parameter(Mandatory = $true)][string] $RootPath,
        [Parameter(Mandatory = $true)][string[]] $NodeArguments
    )
    if ($OperationSlug -cnotin @('capture', 'cleanup') -or
        $RootPath -cne "/opt/clearra/.v080-prestage-helper-$DeploymentNonce-$OperationSlug") {
        throw 'Oracle prestage helper transport identity is invalid.'
    }
    $uploadRoot = "/home/ubuntu/.clearra-v080-prestage-helper-$DeploymentNonce-$OperationSlug"
    $uploadName = [IO.Path]::GetFileName($uploadRoot)
    $rootName = [IO.Path]::GetFileName($RootPath)
    $cleanupUnit = "clearra-v080-prestage-helper-$DeploymentNonce-$OperationSlug-cleanup"
    $cleanupTimer = "$cleanupUnit.timer"
    $cleanupService = "$cleanupUnit.service"
    if ($uploadName -cnotmatch '^\.clearra-v080-prestage-helper-[0-9a-f]{64}-(capture|cleanup)$' -or
        $rootName -cnotmatch '^\.v080-prestage-helper-[0-9a-f]{64}-(capture|cleanup)$') {
        throw 'Oracle prestage helper transport paths are outside the nonce namespace.'
    }

    $localFiles = @()
    $flatNames = @()
    foreach ($entry in @($Manifest.files)) {
        $localPath = [IO.Path]::GetFullPath((Join-Path $repositoryRoot ([string]$entry.path)))
        if (-not $localPath.StartsWith("$repositoryRoot$([IO.Path]::DirectorySeparatorChar)", [StringComparison]::Ordinal)) {
            throw 'Oracle prestage helper local path escapes the accepted source.'
        }
        $item = Get-ExactLeaf -Path $localPath -Label "Accepted prestage helper $($entry.path)"
        if ($item.Length -ne [long]$entry.size -or
            (Get-ExactSha256 -Path $localPath) -cne [string]$entry.sha256) {
            throw 'Oracle prestage helper changed after manifest sealing.'
        }
        $flatName = [IO.Path]::GetFileName([string]$entry.path)
        if ($flatName -cnotmatch '^[A-Za-z0-9._-]{1,128}$' -or $flatNames -ccontains $flatName) {
            throw 'Oracle prestage helper upload leaf set is ambiguous.'
        }
        $flatNames += $flatName
        $localFiles += [pscustomobject]@{
            Entry = $entry
            LocalPath = $localPath
            FlatName = $flatName
            UploadPath = "$uploadRoot/$flatName"
            RootPath = "$RootPath/$($entry.path)"
        }
    }
    if ($localFiles.Count -ne 4) {
        throw 'Oracle prestage helper local file set is incomplete.'
    }

    $rootDirectories = @(
        "$RootPath/apps",
        "$RootPath/apps/clearra-discord-bot",
        "$RootPath/apps/clearra-discord-bot/scripts",
        "$RootPath/apps/clearra-discord-bot/src",
        "$RootPath/apps/clearra-discord-bot/src/job-service"
    )
    $rootMayExist = $false
    $uploadMayExist = $false
    $cleanupTimerMayExist = $false
    $primaryFailure = $null
    $cleanupFailures = [Collections.Generic.List[string]]::new()
    $result = @()
    try {
        $existingCleanupUnits = @(Invoke-ExactSsh @(
            'sudo', '-n', '/usr/bin/systemctl', 'list-units', '--all',
            '--full', '--plain', '--no-legend', $cleanupTimer, $cleanupService
        ))
        if ($existingCleanupUnits.Count -ne 0) {
            throw 'Oracle prestage helper cleanup watchdog namespace already exists.'
        }
        # Arm a nonce-exact root cleanup before creating any transport path. If
        # the runner or any one of the following SSH/SCP processes is killed,
        # this transient unit eventually removes only this invocation's inert
        # transport roots. The business settings backup is deliberately not a
        # watchdog target.
        [void](Invoke-ExactSsh @(
            'sudo', '-n', '/usr/bin/systemd-run', '--quiet', '--collect',
            "--unit=$cleanupUnit", '--on-active=30m',
            '/usr/bin/flock', $releaseDeployLockPath,
            '/usr/bin/rm', '-rf', '--', $RootPath, $uploadRoot
        ))
        $cleanupTimerMayExist = $true
        $cleanupTimerId = @(Invoke-ExactSsh @(
            'sudo', '-n', '/usr/bin/systemctl', 'show', '--property=Id',
            '--value', $cleanupTimer
        ))
        $cleanupTimerState = @(Invoke-ExactSsh @(
            'sudo', '-n', '/usr/bin/systemctl', 'show', '--property=ActiveState',
            '--value', $cleanupTimer
        ))
        if ($cleanupTimerId.Count -ne 1 -or $cleanupTimerId[0] -cne $cleanupTimer -or
            $cleanupTimerState.Count -ne 1 -or $cleanupTimerState[0] -cne 'active') {
            throw 'Oracle prestage helper cleanup watchdog failed closed.'
        }

        $existingUpload = @(Invoke-ExactSsh @(
            '/usr/bin/find', '/home/ubuntu', '-maxdepth', '1',
            '-name', $uploadName, '-print'
        ))
        $existingRoot = @(Invoke-ExactSsh @(
            'sudo', '-n', '/usr/bin/find', '/opt/clearra', '-maxdepth', '1',
            '-name', $rootName, '-print'
        ))
        if ($existingUpload.Count -ne 0 -or $existingRoot.Count -ne 0) {
            throw 'Oracle prestage helper nonce namespace already exists.'
        }

        $uploadMayExist = $true
        [void](Invoke-ExactSsh @('/usr/bin/mkdir', '-m', '0700', '--', $uploadRoot))
        $uploadMetadata = @(Invoke-ExactSsh @(
            '/usr/bin/stat', '-c', '%u:%g:%a', '--', $uploadRoot
        ))
        $uploadResolved = @(Invoke-ExactSsh @('/usr/bin/readlink', '-f', '--', $uploadRoot))
        if ($uploadMetadata.Count -ne 1 -or $uploadMetadata[0] -cne '1001:1001:700' -or
            $uploadResolved.Count -ne 1 -or $uploadResolved[0] -cne $uploadRoot) {
            throw 'Oracle prestage helper upload root authority differs.'
        }

        foreach ($file in $localFiles) {
            $null = & scp @scpArguments '--' $file.LocalPath "${userName}@${hostName}:$($file.UploadPath)"
            if ($LASTEXITCODE -ne 0) {
                throw "Oracle prestage helper upload failed: $($file.FlatName)"
            }
            [void](Invoke-ExactSsh @('/usr/bin/chmod', '0600', '--', $file.UploadPath))
            $regularFile = @(Invoke-ExactSsh @(
                '/usr/bin/find', $file.UploadPath, '-maxdepth', '0',
                '-type', 'f', '-links', '1', '-uid', '1001', '-gid', '1001', '-print'
            ))
            $metadata = @(Invoke-ExactSsh @(
                '/usr/bin/stat', '-c', '%u:%g:%a:%s:%h', '--', $file.UploadPath
            ))
            if ($regularFile.Count -ne 1 -or $regularFile[0] -cne $file.UploadPath -or
                $metadata.Count -ne 1 -or
                $metadata[0] -cne "1001:1001:600:$([long]$file.Entry.size):1") {
                throw "Oracle prestage helper upload metadata differs: $($file.FlatName)"
            }
            $digest = @(Invoke-ExactSsh @('/usr/bin/sha256sum', '--', $file.UploadPath))
            if ($digest.Count -ne 1 -or $digest[0] -cnotmatch '^([0-9a-f]{64})  /' -or
                $Matches[1] -cne [string]$file.Entry.sha256) {
                throw "Oracle prestage helper upload digest differs: $($file.FlatName)"
            }
        }
        $uploadInventory = @(Invoke-ExactSsh @(
            '/usr/bin/find', $uploadRoot, '-mindepth', '1', '-maxdepth', '1',
            '-print'
        ))
        $expectedUploadInventory = @($localFiles | ForEach-Object {
            $_.UploadPath
        })
        Assert-ExactOrdinalSet `
            -Actual $uploadInventory -Expected $expectedUploadInventory `
            -Label 'Oracle prestage helper upload inventory'

        $rootMayExist = $true
        [void](Invoke-ExactSsh @(
            'sudo', '-n', '/usr/bin/mkdir', '-m', '0700', '--', $RootPath
        ))
        foreach ($directory in $rootDirectories) {
            [void](Invoke-ExactSsh @(
                'sudo', '-n', '/usr/bin/mkdir', '-m', '0755', '--', $directory
            ))
            $directoryMetadata = @(Invoke-ExactSsh @(
                'sudo', '-n', '/usr/bin/stat', '-c', '%u:%g:%a', '--', $directory
            ))
            $directoryResolved = @(Invoke-ExactSsh @(
                'sudo', '-n', '/usr/bin/readlink', '-f', '--', $directory
            ))
            if ($directoryMetadata.Count -ne 1 -or $directoryMetadata[0] -cne '0:0:755' -or
                $directoryResolved.Count -ne 1 -or $directoryResolved[0] -cne $directory) {
                throw 'Root-owned prestage helper directory authority differs.'
            }
        }
        foreach ($file in $localFiles) {
            [void](Invoke-ExactSsh @(
                'sudo', '-n', '/usr/bin/install', '-o', 'root', '-g', 'root',
                '-m', '0644', '--', $file.UploadPath, $file.RootPath
            ))
            $regularFile = @(Invoke-ExactSsh @(
                'sudo', '-n', '/usr/bin/find', $file.RootPath, '-maxdepth', '0',
                '-type', 'f', '-links', '1', '-uid', '0', '-gid', '0', '-print'
            ))
            $metadata = @(Invoke-ExactSsh @(
                'sudo', '-n', '/usr/bin/stat', '-c', '%u:%g:%a:%s:%h', '--', $file.RootPath
            ))
            if ($regularFile.Count -ne 1 -or $regularFile[0] -cne $file.RootPath -or
                $metadata.Count -ne 1 -or
                $metadata[0] -cne "0:0:644:$([long]$file.Entry.size):1") {
                throw "Root-owned prestage helper metadata differs: $($file.Entry.path)"
            }
            $digest = @(Invoke-ExactSsh @(
                'sudo', '-n', '/usr/bin/sha256sum', '--', $file.RootPath
            ))
            if ($digest.Count -ne 1 -or $digest[0] -cnotmatch '^([0-9a-f]{64})  /' -or
                $Matches[1] -cne [string]$file.Entry.sha256) {
                throw "Root-owned prestage helper digest differs: $($file.Entry.path)"
            }
        }
        $rootMetadata = @(Invoke-ExactSsh @(
            'sudo', '-n', '/usr/bin/stat', '-c', '%u:%g:%a', '--', $RootPath
        ))
        $rootResolved = @(Invoke-ExactSsh @(
            'sudo', '-n', '/usr/bin/readlink', '-f', '--', $RootPath
        ))
        $mainResolved = @(Invoke-ExactSsh @(
            'sudo', '-n', '/usr/bin/readlink', '-f', '--', $prestageMain
        ))
        if ($rootMetadata.Count -ne 1 -or $rootMetadata[0] -cne '0:0:700' -or
            $rootResolved.Count -ne 1 -or $rootResolved[0] -cne $RootPath -or
            $mainResolved.Count -ne 1 -or $mainResolved[0] -cne $prestageMain) {
            throw 'Root-owned prestage helper self path differs.'
        }
        $rootInventory = @(Invoke-ExactSsh @(
            'sudo', '-n', '/usr/bin/find', $RootPath, '-mindepth', '1',
            '-print'
        ))
        $expectedRootInventory = @($rootDirectories) + @($localFiles | ForEach-Object {
            $_.RootPath
        })
        Assert-ExactOrdinalSet `
            -Actual $rootInventory -Expected $expectedRootInventory `
            -Label 'Root-owned prestage helper inventory'

        $result = @(Invoke-ExactSsh $NodeArguments)
    } catch {
        $primaryFailure = $_
    } finally {
        if ($rootMayExist) {
            try {
                [void](Invoke-ExactSsh @(
                    'sudo', '-n', '/usr/bin/rm', '-rf', '--', $RootPath
                ))
                $residue = @(Invoke-ExactSsh @(
                    'sudo', '-n', '/usr/bin/find', '/opt/clearra', '-maxdepth', '1',
                    '-name', $rootName, '-print'
                ))
                if ($residue.Count -ne 0) {
                    throw 'root helper residue remains'
                }
            } catch {
                $cleanupFailures.Add("root:$($_.Exception.Message)")
            }
        }
        if ($uploadMayExist) {
            try {
                [void](Invoke-ExactSsh @('/usr/bin/rm', '-rf', '--', $uploadRoot))
                $residue = @(Invoke-ExactSsh @(
                    '/usr/bin/find', '/home/ubuntu', '-maxdepth', '1',
                    '-name', $uploadName, '-print'
                ))
                if ($residue.Count -ne 0) {
                    throw 'upload residue remains'
                }
            } catch {
                $cleanupFailures.Add("upload:$($_.Exception.Message)")
            }
        }
        # Disarm the remote fallback only after both transport namespaces were
        # removed and their absence was read back. systemd-run initially loads
        # the timer but may not load its paired service until the timer fires.
        # Stop the timer first, then stop the service only when an exact unit
        # readback says that it is loaded. If the service is collected between
        # that readback and stop, final absence remains the authority. If either
        # transport cleanup failed, leave both loaded units untouched so the
        # nonce-exact watchdog can retry under the shared deployment lock.
        if ($cleanupTimerMayExist -and $cleanupFailures.Count -eq 0) {
            try {
                [void](Invoke-ExactSsh @(
                    'sudo', '-n', '/usr/bin/systemctl', 'stop',
                    $cleanupTimer
                ))
                $watchdogUnitsAfterTimerStop = @(Invoke-ExactSsh @(
                    'sudo', '-n', '/usr/bin/systemctl', 'list-units', '--all',
                    '--full', '--plain', '--no-legend', $cleanupTimer, $cleanupService
                ))
                if ($watchdogUnitsAfterTimerStop.Count -gt 1 -or
                    ($watchdogUnitsAfterTimerStop.Count -eq 1 -and
                    -not ([string]$watchdogUnitsAfterTimerStop[0]).StartsWith(
                        "$cleanupService ", [StringComparison]::Ordinal
                    ))) {
                    throw 'cleanup watchdog state after timer stop is invalid'
                }
                if ($watchdogUnitsAfterTimerStop.Count -eq 1) {
                    # A transient service may finish and be collected after the
                    # exact list-units readback. Record the stop result, but use
                    # the final two-unit absence readback as the race-safe,
                    # fail-closed authority.
                    $cleanupServiceStop = Invoke-ExactSshResult @(
                        'sudo', '-n', '/usr/bin/systemctl', 'stop',
                        $cleanupService
                    )
                }
                $watchdogResidue = @(Invoke-ExactSsh @(
                    'sudo', '-n', '/usr/bin/systemctl', 'list-units', '--all',
                    '--full', '--plain', '--no-legend', $cleanupTimer, $cleanupService
                ))
                if ($watchdogResidue.Count -ne 0) {
                    throw 'cleanup watchdog unit residue remains'
                }
            } catch {
                $cleanupFailures.Add("watchdog:$($_.Exception.Message)")
            }
        }
    }
    if ($cleanupFailures.Count -ne 0) {
        $primaryMessage = if ($null -eq $primaryFailure) { 'none' } else {
            $primaryFailure.Exception.Message
        }
        throw "Oracle prestage helper cleanup failed after '$primaryMessage': $($cleanupFailures -join ', ')"
    }
    if ($null -ne $primaryFailure) {
        throw $primaryFailure
    }
    return $result
}

$output = @(
    if ($usesPrestageHelperBundle) {
        Invoke-PrestageHelperBundle `
            -Manifest $prestageManifest `
            -OperationSlug $prestageOperationSlug `
            -RootPath $prestageRoot `
            -NodeArguments $auditedRemoteArguments
    } else {
        Invoke-ExactSsh $remoteArguments
    }
)

$validatedEvidence = $null
switch ($Operation) {
    'capture-prestage-authority' {
        if ($output.Count -ne 1) {
            throw 'Oracle prestage authority capture returned invalid output cardinality.'
        }
        try {
            $capture = $output[0] | ConvertFrom-Json
        } catch {
            throw 'Oracle prestage authority capture returned invalid JSON.'
        }
        $expectedKeys = @(
            'priorRevision', 'priorOracleRelease', 'priorOracleReleaseId',
            'priorOracleReleaseSha256', 'priorOracleSettingsBackup',
            'priorOracleSettingsSha256', 'priorRuntimeAuthorityKind',
            'priorRuntimeAuthoritySha256', 'priorJobUrl', 'deploymentNonce'
        )
        $actualKeys = @($capture.PSObject.Properties.Name)
        if ($actualKeys.Count -ne $expectedKeys.Count -or
            (Compare-Object -CaseSensitive -SyncWindow 0 $expectedKeys $actualKeys) -or
            $capture.priorRevision -cne $PriorRevision -or
            $capture.priorRuntimeAuthorityKind -cne $PriorRuntimeAuthorityKind -or
            $capture.deploymentNonce -cne $DeploymentNonce) {
            throw 'Oracle prestage authority capture returned an invalid closed result.'
        }
        $validatedEvidence = $capture
    }
    'cleanup-prestage-backup' {
        if ($output.Count -ne 1) {
            throw 'Oracle prestage backup cleanup returned invalid output cardinality.'
        }
        try {
            $cleanup = $output[0] | ConvertFrom-Json
        } catch {
            throw 'Oracle prestage backup cleanup returned invalid JSON.'
        }
        $expectedKeys = @('deploymentNonce', 'backupRemoved')
        $actualKeys = @($cleanup.PSObject.Properties.Name)
        if ($actualKeys.Count -ne $expectedKeys.Count -or
            (Compare-Object -CaseSensitive -SyncWindow 0 $expectedKeys $actualKeys) -or
            $cleanup.deploymentNonce -cne $DeploymentNonce -or
            $cleanup.backupRemoved -isnot [bool]) {
            throw 'Oracle prestage backup cleanup returned an invalid closed result.'
        }
    }
    'capture-rollback-authority' {
        if ($output.Count -ne 1) {
            throw 'Oracle rollback authority capture returned invalid output cardinality.'
        }
        try {
            $capture = $output[0] | ConvertFrom-Json
        } catch {
            throw 'Oracle rollback authority capture returned invalid JSON.'
        }
        $expectedKeys = @(
            'priorRevision', 'priorOracleRelease', 'priorOracleReleaseId',
            'priorOracleReleaseSha256', 'priorOracleSettingsBackup',
            'priorOracleSettingsSha256', 'priorRuntimeAuthorityKind',
            'priorRuntimeAuthoritySha256', 'priorJobUrl', 'deploymentNonce'
        )
        $actualKeys = @($capture.PSObject.Properties.Name)
        if ($actualKeys.Count -ne $expectedKeys.Count -or
            (Compare-Object -CaseSensitive -SyncWindow 0 $expectedKeys $actualKeys) -or
            $capture.priorRevision -cne $PriorRevision -or
            $capture.priorRuntimeAuthorityKind -cne $PriorRuntimeAuthorityKind -or
            $capture.deploymentNonce -cne $DeploymentNonce) {
            throw 'Oracle rollback authority capture returned an invalid closed result.'
        }
        $validatedEvidence = $capture
    }
    'verify-candidate' {
        if ($output.Count -ne 1 -or $output[0] -cne 'oracle_candidate=verified') {
            throw 'Oracle candidate verification did not return the exact success attestation.'
        }
    }
    'observe-candidate' {
        if ($output.Count -ne 1) {
            throw 'Oracle candidate observation returned invalid output cardinality.'
        }
        try {
            $observation = $output[0] | ConvertFrom-Json
        } catch {
            throw 'Oracle candidate observation returned invalid JSON.'
        }
        $expectedKeys = @(
            'contract', 'sourceCommit', 'candidateUrl', 'candidateRevision',
            'jobUrl', 'oracleReleaseId', 'activeReleasePath',
            'oracleReleaseSha256', 'oracleSettingsSha256', 'deploymentNonce',
            'verifiedAfter', 'gatewayPid', 'gatewayStartMonotonicUsec', 'bootId',
            'readyRecordObserved', 'freshOperationAt', 'observedAt',
            'runtimeIdentity'
        )
        $actualKeys = @($observation.PSObject.Properties.Name)
        if ($actualKeys.Count -ne $expectedKeys.Count -or
            (Compare-Object -CaseSensitive -SyncWindow 0 $expectedKeys $actualKeys) -or
            $observation.contract -cne 'clearra.oracle.candidate-observation.v1' -or
            $observation.sourceCommit -cne $SourceCommit -or
            $observation.candidateUrl -cne (Get-CanonicalOrigin -Value $CandidateUrl) -or
            $observation.candidateRevision -cne $CandidateRevision -or
            $observation.jobUrl -cne "$(Get-CanonicalOrigin -Value $CandidateUrl)/jobs" -or
            $observation.oracleReleaseId -cne $OracleReleaseId -or
            $observation.activeReleasePath -cne "/opt/clearra/releases/$OracleReleaseId" -or
            $observation.oracleReleaseSha256 -cne $OracleReleaseSha256 -or
            $observation.oracleSettingsSha256 -cne $OracleSettingsSha256 -or
            $observation.deploymentNonce -cne $DeploymentNonce -or
            -not (Test-JsonSafePositiveInteger -Value $observation.gatewayPid) -or
            -not (Test-JsonSafePositiveInteger -Value $observation.gatewayStartMonotonicUsec) -or
            $observation.bootId -cnotmatch '^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$' -or
            $observation.readyRecordObserved -cne $true) {
            throw 'Oracle candidate observation returned an invalid closed result.'
        }
        $observation.verifiedAfter = Get-CanonicalTimestamp `
            -Value $observation.verifiedAfter
        if ($observation.verifiedAfter -cne $canonicalVerifiedAfter) {
            throw 'Oracle candidate observation returned an invalid closed result.'
        }
        $observation.freshOperationAt = Get-CanonicalTimestamp `
            -Value $observation.freshOperationAt
        $observation.observedAt = Get-CanonicalTimestamp `
            -Value $observation.observedAt
        if ([StringComparer]::Ordinal.Compare(
                $observation.freshOperationAt,
                $observation.verifiedAfter
            ) -lt 0 -or
            [StringComparer]::Ordinal.Compare(
                $observation.observedAt,
                $observation.freshOperationAt
            ) -lt 0) {
            throw 'Oracle candidate observation timestamps are out of order.'
        }
        $validatedEvidence = $observation
    }
    'classify-current-authority' {
        if ($output.Count -ne 1) {
            throw 'Oracle current authority classification returned invalid output cardinality.'
        }
        try {
            $classification = $output[0] | ConvertFrom-Json
        } catch {
            throw 'Oracle current authority classification returned invalid JSON.'
        }
        $expectedKeys = @(
            'contract', 'state', 'reason', 'activeReleaseId',
            'activeReleasePath', 'activeReleaseSha256', 'activeSettingsSha256',
            'activeJobUrl', 'runtimeAuthorityKind', 'runtimeAuthoritySha256'
        )
        $actualKeys = @($classification.PSObject.Properties.Name)
        if ($actualKeys.Count -ne $expectedKeys.Count -or
            (Compare-Object -CaseSensitive -SyncWindow 0 $expectedKeys $actualKeys) -or
            $classification.contract -cne 'clearra.oracle.current-authority-classification.v1' -or
            $classification.state -cnotin @('prior', 'candidate', 'other') -or
            [string]::IsNullOrWhiteSpace([string]$classification.reason)) {
            throw 'Oracle current authority classification returned an invalid closed result.'
        }
        if ($classification.state -ceq 'prior' -and (
                $classification.activeReleaseId -cne $PriorReleaseId -or
                $classification.activeReleasePath -cne $PriorRelease -or
                $classification.activeReleaseSha256 -cne $PriorReleaseSha256 -or
                $classification.activeSettingsSha256 -cne $PriorSettingsSha256 -or
                $classification.activeJobUrl -cne (Get-CanonicalJobUrl -Value $PriorJobUrl) -or
                $classification.runtimeAuthorityKind -cne $PriorRuntimeAuthorityKind -or
                $classification.runtimeAuthoritySha256 -cne $PriorRuntimeAuthoritySha256)) {
            throw 'Oracle prior classification differs from the sealed authority.'
        }
        if ($classification.state -ceq 'candidate' -and (
                $classification.activeReleaseId -cne $OracleReleaseId -or
                $classification.activeReleasePath -cne "/opt/clearra/releases/$OracleReleaseId" -or
                $classification.activeReleaseSha256 -cne $OracleReleaseSha256 -or
                $classification.activeSettingsSha256 -cne $OracleSettingsSha256 -or
                $classification.activeJobUrl -cne "$(Get-CanonicalOrigin -Value $CandidateUrl)/jobs" -or
                $classification.runtimeAuthorityKind -cne 'clearra.rollback.runtime-identity.v1' -or
                $classification.runtimeAuthoritySha256 -cnotmatch $sha256Pattern)) {
            throw 'Oracle candidate classification differs from the sealed authority.'
        }
        $validatedEvidence = $classification
    }
    'restore-prior-and-verify' {
        if ($output.Count -ne 1 -or $output[0] -cne 'oracle_rollback=verified') {
            throw 'Oracle rollback verification did not return the exact success attestation.'
        }
    }
}
if ($null -ne $evidenceOutputPath) {
    Write-CanonicalEvidenceOutput -Path $evidenceOutputPath -Value $validatedEvidence
}
$output
