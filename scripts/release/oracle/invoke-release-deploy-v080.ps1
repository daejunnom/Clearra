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
$remoteLauncherPath = '/usr/local/sbin/clearra-oracle-release-deploy'
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
$remoteArguments = if ($Operation -in @('capture-prestage-authority', 'cleanup-prestage-backup')) {
    @(
        'sudo', '-n', '/usr/bin/node',
        '/opt/clearra/current/apps/clearra-discord-bot/scripts/capture-oracle-rollback-authority.mjs'
    )
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
    'cleanup-prestage-backup' {
        Assert-UnusedArguments -Values $operationValues -Allowed @()
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

foreach ($argument in $remoteArguments) {
    if ($argument -cnotmatch '^[A-Za-z0-9_./:%=@+-]{1,2048}$') {
        throw 'Oracle deployment argument is outside the non-secret token grammar.'
    }
}

if ($AuditOnly) {
    'oracle_release_deploy_invoker=audit-ok'
    "oracle_operation=$Operation"
    "oracle_remote_argument_count=$($remoteArguments.Count)"
    "oracle_remote_arguments_sha256=$(Get-TextSha256 -Text (($remoteArguments -join "`n") + "`n"))"
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
$output = @(& ssh @sshArguments @remoteArguments)
if ($LASTEXITCODE -ne 0) {
    throw "Oracle deployment command failed with exit code $LASTEXITCODE."
}

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
