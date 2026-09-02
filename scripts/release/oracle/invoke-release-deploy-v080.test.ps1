# SRP rationale: this regression owner has one change reason: execute the Oracle release wrapper's local and modeled remote authority contract end to end.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$wrapper = Join-Path $PSScriptRoot 'invoke-release-deploy-v080.ps1'
$repositoryRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../..'))
$sourceCommit = @(& git -C $repositoryRoot rev-parse HEAD)
if ($LASTEXITCODE -ne 0 -or $sourceCommit.Count -ne 1 -or
    $sourceCommit[0] -cnotmatch '^[0-9a-f]{40}$') {
    throw 'Oracle wrapper test could not resolve the accepted source commit.'
}
$sourceCommit = [string]$sourceCommit[0]
$scriptReleaseId = "v0.8.0-$($sourceCommit.Substring(0, 7))"
$scriptReleaseSha256 = 'b' * 64
$deploymentNonce = 'a' * 64
$candidateRevision = "clearra-current-job-v080-$($sourceCommit.Substring(0, 7))"
$candidateUrl = 'https://candidate.example.test'
$verifiedAfter = '2026-08-30T00:00:00.000Z'
$dummyIdentity = Join-Path $PSScriptRoot 'identity-must-not-be-read'

function ssh-keygen {
    $global:LASTEXITCODE = 0
    '256 SHA256:mdw7bdzZOBrd6sCebPmMVuTaps+ct2OaOle/gaZMBKU 157.151.254.175 (ED25519)'
}

function wsl.exe {
    $global:LASTEXITCODE = 0
    if ($args.Count -ge 2 -and $args[0] -ceq '-e' -and $args[1] -ceq '/usr/bin/wslpath') {
        '/tmp/clearra-oracle-release-deploy-v080'
        return
    }
    if ($args.Count -ge 2 -and $args[0] -ceq '-e' -and $args[1] -ceq '/usr/bin/dash') {
        return
    }
    throw 'Unexpected WSL invocation in Oracle evidence boundary test.'
}

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

function Assert-ExactStringSequence {
    param(
        [Parameter(Mandatory = $true)][object[]] $Actual,
        [Parameter(Mandatory = $true)][string[]] $Expected,
        [Parameter(Mandatory = $true)][string] $Label
    )
    [string[]]$actualStrings = @($Actual | ForEach-Object { [string]$_ })
    if ($actualStrings.Count -ne $Expected.Count -or
        (Compare-Object -CaseSensitive -SyncWindow 0 $Expected $actualStrings)) {
        throw "$Label argument sequence drifted."
    }
}

$windowsTarget = 'C:\accepted\clearra-oracle-release-deploy-v080'
$windowsContract = Invoke-ExtractedWrapperFunction `
    -Path $wrapper `
    -FunctionName 'Get-OraclePosixSyntaxAuditContract' `
    -Arguments @('windows', $windowsTarget)
if ($windowsContract.ProjectionCommand -cne 'wsl.exe' -or
    $windowsContract.SyntaxCommand -cne 'wsl.exe') {
    throw 'Windows release-deploy syntax-audit command contract drifted.'
}
Assert-ExactStringSequence `
    -Actual @($windowsContract.ProjectionArguments) `
    -Expected @('-e', '/usr/bin/wslpath', '-a', '--', $windowsTarget) `
    -Label 'Windows release-deploy projection'
Assert-ExactStringSequence `
    -Actual @($windowsContract.SyntaxArguments) `
    -Expected @('-e', '/usr/bin/dash', '-n', '--') `
    -Label 'Windows release-deploy syntax audit'
$windowsSshConfig = Invoke-ExtractedWrapperFunction `
    -Path $wrapper -FunctionName 'Get-OracleSshConfigPath' -Arguments @('windows')
if ($windowsSshConfig -cne 'NUL') {
    throw 'Windows release-deploy SSH config path drifted.'
}

$linuxTarget = '/tmp/accepted/clearra-oracle-release-deploy-v080'
$linuxContract = Invoke-ExtractedWrapperFunction `
    -Path $wrapper `
    -FunctionName 'Get-OraclePosixSyntaxAuditContract' `
    -Arguments @('linux', $linuxTarget)
if ($null -ne $linuxContract.ProjectionCommand -or
    @($linuxContract.ProjectionArguments).Count -ne 0 -or
    $linuxContract.SyntaxCommand -cne '/usr/bin/dash') {
    throw 'Linux release-deploy syntax-audit command contract drifted.'
}
Assert-ExactStringSequence `
    -Actual @($linuxContract.SyntaxArguments) `
    -Expected @('-n', '--', $linuxTarget) `
    -Label 'Linux release-deploy syntax audit'
$linuxSshConfig = Invoke-ExtractedWrapperFunction `
    -Path $wrapper -FunctionName 'Get-OracleSshConfigPath' -Arguments @('linux')
if ($linuxSshConfig -cne '/dev/null') {
    throw 'Linux release-deploy SSH config path drifted.'
}

function Assert-AuditResult {
    param(
        [Parameter(Mandatory = $true)][string[]] $Output,
        [Parameter(Mandatory = $true)][string] $Operation
    )
    if ($Output.Count -ne 4 -or
        $Output[0] -cne 'oracle_release_deploy_invoker=audit-ok' -or
        $Output[1] -cne "oracle_operation=$Operation" -or
        $Output[2] -cnotmatch '^oracle_remote_argument_count=[1-9][0-9]{0,2}$' -or
        $Output[3] -cnotmatch '^oracle_remote_arguments_sha256=[0-9a-f]{64}$') {
        throw "AuditOnly result did not match for $Operation."
    }
}

$capture = @(& $wrapper `
    -Operation capture-rollback-authority `
    -ScriptReleaseId $scriptReleaseId `
    -ScriptReleaseSha256 $scriptReleaseSha256 `
    -PriorRevision 'clearra-current-job-v075-042ec21' `
    -PriorRuntimeAuthorityKind 'clearra.rollback.legacy-health-no-runtime.v1' `
    -DeploymentNonce $deploymentNonce `
    -IdentityFile $dummyIdentity `
    -AuditOnly)
Assert-AuditResult -Output $capture -Operation 'capture-rollback-authority'

$candidate = @(& $wrapper `
    -Operation verify-candidate `
    -ScriptReleaseId $scriptReleaseId `
    -ScriptReleaseSha256 $scriptReleaseSha256 `
    -Proof "/run/clearra-deploy/clearra-oracle-candidate-$deploymentNonce.json" `
    -SourceCommit $sourceCommit `
    -CandidateUrl "$candidateUrl/" `
    -CandidateRevision $candidateRevision `
    -OracleReleaseId $scriptReleaseId `
    -OracleReleaseSha256 $scriptReleaseSha256 `
    -OracleSettingsSha256 ('c' * 64) `
    -DeploymentNonce $deploymentNonce `
    -VerifiedAfter $verifiedAfter `
    -AuditOnly)
Assert-AuditResult -Output $candidate -Operation 'verify-candidate'

$observation = @(& $wrapper `
    -Operation observe-candidate `
    -ScriptReleaseId $scriptReleaseId `
    -ScriptReleaseSha256 $scriptReleaseSha256 `
    -SourceCommit $sourceCommit `
    -CandidateUrl "$candidateUrl/" `
    -CandidateRevision $candidateRevision `
    -OracleReleaseId $scriptReleaseId `
    -OracleReleaseSha256 $scriptReleaseSha256 `
    -OracleSettingsSha256 ('c' * 64) `
    -DeploymentNonce $deploymentNonce `
    -VerifiedAfter $verifiedAfter `
    -AuditOnly)
Assert-AuditResult -Output $observation -Operation 'observe-candidate'

$classification = @(& $wrapper `
    -Operation classify-current-authority `
    -ScriptReleaseId $scriptReleaseId `
    -ScriptReleaseSha256 $scriptReleaseSha256 `
    -SourceCommit $sourceCommit -CandidateUrl $candidateUrl `
    -CandidateRevision $candidateRevision -OracleReleaseId $scriptReleaseId `
    -OracleReleaseSha256 $scriptReleaseSha256 -OracleSettingsSha256 ('c' * 64) `
    -PriorRelease '/opt/clearra/releases/v0.7.5-042ec21' `
    -PriorReleaseId 'v0.7.5-042ec21' -PriorReleaseSha256 ('d' * 64) `
    -PriorSettingsSha256 ('e' * 64) `
    -PriorRuntimeAuthorityKind 'clearra.rollback.legacy-health-no-runtime.v1' `
    -PriorRuntimeAuthoritySha256 ('f' * 64) `
    -PriorJobUrl 'https://prior.example.test/jobs' `
    -PriorRevision 'clearra-current-job-v075-042ec21' `
    -DeploymentNonce $deploymentNonce -AuditOnly)
Assert-AuditResult -Output $classification -Operation 'classify-current-authority'

$rollback = @(& $wrapper `
    -Operation restore-prior-and-verify `
    -ScriptReleaseId $scriptReleaseId `
    -ScriptReleaseSha256 $scriptReleaseSha256 `
    -PriorRelease '/opt/clearra/releases/v0.7.5-042ec21' `
    -PriorReleaseId 'v0.7.5-042ec21' `
    -PriorReleaseSha256 ('d' * 64) `
    -PriorSettingsBackup "/etc/clearra-gateway/settings.pre-v0.8.0-$deploymentNonce" `
    -PriorSettingsSha256 ('e' * 64) `
    -PriorRuntimeAuthorityKind 'clearra.rollback.legacy-health-no-runtime.v1' `
    -PriorRuntimeAuthoritySha256 ('f' * 64) `
    -PriorJobUrl 'https://prior.example.test/jobs' `
    -PriorRevision 'clearra-current-job-v075-042ec21' `
    -Proof "/run/clearra-deploy/clearra-oracle-rollback-$deploymentNonce.json" `
    -DeploymentNonce $deploymentNonce `
    -VerifiedAfter $verifiedAfter `
    -AuditOnly)
Assert-AuditResult -Output $rollback -Operation 'restore-prior-and-verify'

$rejected = $false
try {
    [void]@(& $wrapper `
        -Operation capture-rollback-authority `
        -ScriptReleaseId $scriptReleaseId `
        -ScriptReleaseSha256 $scriptReleaseSha256 `
        -PriorRevision 'clearra-current-job-v075-042ec21' `
        -PriorRuntimeAuthorityKind 'clearra.rollback.legacy-health-no-runtime.v1' `
        -CandidateUrl $candidateUrl `
        -DeploymentNonce $deploymentNonce `
        -AuditOnly)
} catch {
    $rejected = $_.Exception.Message -like '*Argument CandidateUrl is not valid*'
}
if (-not $rejected) {
    throw 'Typed invoker accepted an operation-crossing argument.'
}

$evidencePath = [IO.Path]::Combine(
    [IO.Path]::GetTempPath(),
    "clearra-oracle-evidence-$([Guid]::NewGuid().ToString('N')).json"
)
$rejected = $false
try {
    [void]@(& $wrapper `
        -Operation capture-rollback-authority `
        -ScriptReleaseId $scriptReleaseId `
        -ScriptReleaseSha256 $scriptReleaseSha256 `
        -PriorRevision 'clearra-current-job-v075-042ec21' `
        -PriorRuntimeAuthorityKind 'clearra.rollback.legacy-health-no-runtime.v1' `
        -DeploymentNonce $deploymentNonce `
        -EvidenceOutput $evidencePath `
        -AuditOnly)
} catch {
    $rejected = $_.Exception.Message -like '*unavailable in AuditOnly*'
}
if (-not $rejected -or (Test-Path -LiteralPath $evidencePath)) {
    throw 'AuditOnly unexpectedly created durable Oracle evidence.'
}

$rejected = $false
try {
    [void]@(& $wrapper `
        -Operation verify-candidate `
        -ScriptReleaseId $scriptReleaseId `
        -ScriptReleaseSha256 $scriptReleaseSha256 `
        -Proof "/run/clearra-deploy/clearra-oracle-candidate-$deploymentNonce.json" `
        -SourceCommit $sourceCommit `
        -CandidateUrl $candidateUrl `
        -CandidateRevision $candidateRevision `
        -OracleReleaseId $scriptReleaseId `
        -OracleReleaseSha256 $scriptReleaseSha256 `
        -OracleSettingsSha256 ('c' * 64) `
        -DeploymentNonce $deploymentNonce `
        -VerifiedAfter $verifiedAfter `
        -EvidenceOutput $evidencePath `
        -AuditOnly)
} catch {
    $rejected = $_.Exception.Message -like '*only valid for capture, observation, or classification*'
}
if (-not $rejected -or (Test-Path -LiteralPath $evidencePath)) {
    throw 'A non-evidence Oracle operation accepted EvidenceOutput.'
}

$source = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $wrapper))
foreach ($required in @(
    "Test-Path -LiteralPath `$IdentityFile -PathType Leaf",
    "Get-Item -LiteralPath `$IdentityFile -Force",
    '$env:CLEARRA_ORACLE_IDENTITY_FILE',
    "'-i', `$IdentityFile",
    "'StrictHostKeyChecking=yes'",
    "'IdentitiesOnly=yes'",
    "'IdentityAgent=none'",
    "'ProxyCommand=none'",
    "'ProxyJump=none'",
    "'ClearAllForwardings=yes'",
    'Assert-EvidenceOutputPath',
    'ConvertTo-CanonicalJson',
    'Write-CanonicalEvidenceOutput',
    '[IO.FileMode]::CreateNew',
    '[IO.FileShare]::None',
    '[Text.UTF8Encoding]::new($false)',
    '$stream.Flush($true)',
    'create-prestage-helper-bundle.mjs',
    '/usr/bin/systemd-run',
    '--on-active=30m',
    '/usr/bin/flock',
    '/usr/bin/env',
    "'/usr/bin/node', `$prestageMain",
    'Root-owned prestage helper inventory',
    'Oracle prestage helper upload digest differs',
    "'-links', '1'",
    'Oracle prestage helper cleanup failed',
    'Oracle prestage helper cleanup watchdog failed closed.',
    '$cleanupTimerMayExist -and $cleanupFailures.Count -eq 0',
    'Invoke-ExactSshResult',
    '$watchdogUnitsAfterTimerStop',
    '"$cleanupService ", [StringComparison]::Ordinal',
    '$cleanupServiceStop = Invoke-ExactSshResult',
    '$cleanupTimer, $cleanupService',
    "'--full', '--plain', '--no-legend'",
    'cleanup watchdog state after timer stop is invalid',
    'cleanup watchdog unit residue remains',
    "if (`$Operation -notin @('capture-prestage-authority', 'capture-rollback-authority', 'observe-candidate', 'classify-current-authority')) {"
)) {
    if ($source.IndexOf($required, [StringComparison]::Ordinal) -lt 0) {
        throw "Typed invoker is missing pinned identity/SSH marker: $required"
    }
}
if ($source.Contains(
    '/opt/clearra/current/apps/clearra-discord-bot/scripts/capture-oracle-rollback-authority.mjs',
    [StringComparison]::Ordinal
)) {
    throw 'Prestage helper execution still depends on code from the active current release.'
}
if ($source -match '(?i)(Get-Content|Get-FileHash|ReadAllBytes|ReadAllText)[^\r\n]*\$IdentityFile') {
    throw 'Typed invoker reads or hashes the identity file.'
}

function Read-CanonicalEvidenceFile {
    param([Parameter(Mandatory = $true)][string] $Path)
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.PSIsContainer -or
        (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        throw 'Oracle evidence output was not a regular, non-link file.'
    }
    $bytes = [IO.File]::ReadAllBytes($Path)
    if ($bytes.Length -eq 0 -or $bytes[$bytes.Length - 1] -ne 10 -or $bytes -contains 13) {
        throw 'Oracle evidence output did not use exactly one LF terminator.'
    }
    if ($bytes.Length -ge 3 -and
        $bytes[0] -eq 0xef -and $bytes[1] -eq 0xbb -and $bytes[2] -eq 0xbf) {
        throw 'Oracle evidence output unexpectedly contains a UTF-8 BOM.'
    }
    $text = [Text.UTF8Encoding]::new($false, $true).GetString($bytes)
    $json = $text.Substring(0, $text.Length - 1)
    $value = $json | ConvertFrom-Json -DateKind String
    if (($value | ConvertTo-Json -Compress -Depth 10) -cne $json) {
        throw 'Oracle evidence output was not canonical compact JSON.'
    }
    $objects = @($value)
    $runtimeIdentityProperty = $value.PSObject.Properties['runtimeIdentity']
    if ($null -ne $runtimeIdentityProperty) {
        $objects += $runtimeIdentityProperty.Value
    }
    foreach ($object in $objects) {
        [string[]]$actualNames = @($object.PSObject.Properties.Name)
        [string[]]$sortedNames = @($actualNames)
        [Array]::Sort($sortedNames, [StringComparer]::Ordinal)
        if (Compare-Object -CaseSensitive -SyncWindow 0 $sortedNames $actualNames) {
            throw 'Oracle evidence output keys were not in canonical ordinal order.'
        }
    }
    return $value
}

$evidenceRoot = [IO.Path]::Combine(
    [IO.Path]::GetTempPath(),
    "clearra-oracle-evidence-boundary-$([Guid]::NewGuid().ToString('N'))"
)
$captureEvidencePath = Join-Path $evidenceRoot 'capture.json'
$observationEvidencePath = Join-Path $evidenceRoot 'observation.json'
$lockedIdentityPath = Join-Path $evidenceRoot 'locked-identity'
$relativeEvidencePath = "clearra-oracle-relative-$([Guid]::NewGuid().ToString('N')).json"
$relativeEvidenceFullPath = Join-Path (Get-Location).Path $relativeEvidencePath
$realParentPath = Join-Path $evidenceRoot 'real-parent'
$linkedParentPath = Join-Path $evidenceRoot 'linked-parent'
$linkedEvidencePath = Join-Path $linkedParentPath 'evidence.json'
$linkedEvidenceTargetPath = Join-Path $realParentPath 'evidence.json'
$prestageFixtureRoot = [IO.Path]::Combine(
    [IO.Path]::GetTempPath(),
    "clearra-oracle-prestage-transport-$([Guid]::NewGuid().ToString('N'))"
)
$identityLock = $null
$linkCreated = $false
$prestageFixture = $null
$global:clearraOracleTestMockOutput = $null
$global:clearraOracleTestMockSshInvocationCount = 0
$global:clearraOracleTestLockedIdentityPath = $lockedIdentityPath
$global:clearraOraclePrestageRemote = $null
$global:clearraOracleTestExpectedKnownHostsPath = Join-Path `
    $PSScriptRoot 'clearra-oracle-known-hosts'

function Test-ExactCommand {
    param(
        [Parameter(Mandatory = $true)][object[]] $Actual,
        [Parameter(Mandatory = $true)][string[]] $Expected
    )
    if ($Actual.Count -ne $Expected.Count) { return $false }
    for ($index = 0; $index -lt $Expected.Count; $index += 1) {
        if ([string]$Actual[$index] -cne $Expected[$index]) { return $false }
    }
    return $true
}

function Test-CommandPrefix {
    param(
        [Parameter(Mandatory = $true)][object[]] $Actual,
        [Parameter(Mandatory = $true)][string[]] $Expected
    )
    if ($Actual.Count -lt $Expected.Count) { return $false }
    for ($index = 0; $index -lt $Expected.Count; $index += 1) {
        if ([string]$Actual[$index] -cne $Expected[$index]) { return $false }
    }
    return $true
}

function Get-ExpectedOracleCommonClientArguments {
    param(
        [Parameter(Mandatory = $true)][string] $SshConfigPath,
        [Parameter(Mandatory = $true)][string] $IdentityFile,
        [Parameter(Mandatory = $true)][string] $KnownHostsPath
    )
    return [string[]]@(
        '-F', $SshConfigPath,
        '-i', $IdentityFile,
        '-o', 'BatchMode=yes',
        '-o', 'IdentitiesOnly=yes',
        '-o', 'IdentityAgent=none',
        '-o', 'PreferredAuthentications=publickey',
        '-o', 'PasswordAuthentication=no',
        '-o', 'KbdInteractiveAuthentication=no',
        '-o', 'GSSAPIAuthentication=no',
        '-o', 'StrictHostKeyChecking=yes',
        '-o', "UserKnownHostsFile=$KnownHostsPath",
        '-o', "GlobalKnownHostsFile=$KnownHostsPath",
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
}

function Get-ExactOracleClientTail {
    param(
        [Parameter(Mandatory = $true)][object[]] $Actual,
        [Parameter(Mandatory = $true)][string[]] $ExpectedPrefix,
        [Parameter(Mandatory = $true)][string] $Label,
        [Parameter(Mandatory = $true)][int] $MinimumTailCount,
        [int] $ExactTailCount = -1
    )
    if ($Actual.Count -lt ($ExpectedPrefix.Count + $MinimumTailCount)) {
        throw "$Label omitted a fixed option or required tail argument."
    }
    if ($ExactTailCount -ge 0 -and
        $Actual.Count -ne ($ExpectedPrefix.Count + $ExactTailCount)) {
        throw "$Label appended an option outside its exact fixed prefix."
    }
    for ($index = 0; $index -lt $ExpectedPrefix.Count; $index += 1) {
        if ([string]$Actual[$index] -cne $ExpectedPrefix[$index]) {
            throw "$Label fixed option prefix drifted at index $index."
        }
    }
    return [object[]]@($Actual[$ExpectedPrefix.Count..($Actual.Count - 1)])
}

function Assert-OracleClientPrefixMutationRejected {
    param(
        [Parameter(Mandatory = $true)][object[]] $Actual,
        [Parameter(Mandatory = $true)][string[]] $ExpectedPrefix,
        [Parameter(Mandatory = $true)][string] $Label,
        [int] $MinimumTailCount = 1,
        [int] $ExactTailCount = -1
    )
    $rejected = $false
    try {
        [void](Get-ExactOracleClientTail `
            -Actual $Actual -ExpectedPrefix $ExpectedPrefix `
            -Label $Label -MinimumTailCount $MinimumTailCount `
            -ExactTailCount $ExactTailCount)
    } catch {
        $rejected = $_.Exception.Message -like "$Label*"
    }
    if (-not $rejected) {
        throw "$Label mutation was accepted by the exact client-prefix boundary."
    }
}

$clientProbeConfig = if (
    [Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [Runtime.InteropServices.OSPlatform]::Windows
    )
) { 'NUL' } else { '/dev/null' }
[string[]]$clientProbeCommon = @(
    Get-ExpectedOracleCommonClientArguments `
        -SshConfigPath $clientProbeConfig `
        -IdentityFile $lockedIdentityPath `
        -KnownHostsPath $global:clearraOracleTestExpectedKnownHostsPath
)
[string[]]$clientProbeSshPrefix = $clientProbeCommon + @('ubuntu@157.151.254.175')
[string[]]$clientProbeSsh = $clientProbeSshPrefix + @('/usr/bin/true')
[object[]]$missingClientOption = @(
    $clientProbeSsh[0..3] + $clientProbeSsh[6..($clientProbeSsh.Count - 1)]
)
Assert-OracleClientPrefixMutationRejected `
    -Actual $missingClientOption -ExpectedPrefix $clientProbeSshPrefix `
    -Label 'Missing Oracle SSH option'
[object[]]$reorderedClientOptions = @($clientProbeSsh)
$reorderedClientOptions[5] = $clientProbeSsh[7]
$reorderedClientOptions[7] = $clientProbeSsh[5]
Assert-OracleClientPrefixMutationRejected `
    -Actual $reorderedClientOptions -ExpectedPrefix $clientProbeSshPrefix `
    -Label 'Reordered Oracle SSH options'
[object[]]$conflictingClientOption = @(
    $clientProbeCommon + @(
        '-o', 'StrictHostKeyChecking=no',
        'ubuntu@157.151.254.175', '/usr/bin/true'
    )
)
Assert-OracleClientPrefixMutationRejected `
    -Actual $conflictingClientOption -ExpectedPrefix $clientProbeSshPrefix `
    -Label 'Conflicting Oracle SSH option'
[string[]]$clientProbeScpPrefix = @('-q') + $clientProbeCommon
[object[]]$appendedScpOption = @(
    $clientProbeScpPrefix + @(
        '-o', 'StrictHostKeyChecking=no', '--', 'local',
        'ubuntu@157.151.254.175:/home/ubuntu/remote'
    )
)
Assert-OracleClientPrefixMutationRejected `
    -Actual $appendedScpOption -ExpectedPrefix $clientProbeScpPrefix `
    -Label 'Conflicting Oracle SCP option' -MinimumTailCount 3 -ExactTailCount 3

function New-PrestageRemoteResult {
    param(
        [int] $ExitCode = 0,
        [AllowEmptyCollection()][string[]] $Output = @()
    )
    $normalized = [Collections.Generic.List[string]]::new()
    foreach ($line in @($Output)) {
        if ($null -ne $line) { [void]$normalized.Add([string]$line) }
    }
    return [pscustomobject]@{
        ExitCode = $ExitCode
        Output = [string[]]@($normalized)
    }
}

function New-PrestageRemoteModel {
    param(
        [Parameter(Mandatory = $true)][string] $Nonce,
        [Parameter(Mandatory = $true)]
        [ValidateSet('capture', 'cleanup')]
        [string] $OperationSlug,
        [ValidateSet('', 'root', 'upload')]
        [string] $CleanupFailure = '',
        [ValidateSet('single', 'duplicate')]
        [string] $NodeOutputMode = 'single',
        [ValidateSet('timer-only', 'service-loaded')]
        [string] $WatchdogArmState = 'timer-only',
        [ValidateSet('success', 'collected-before-stop', 'failed-still-loaded')]
        [string] $ServiceStopBehavior = 'success'
    )
    $root = "/opt/clearra/.v080-prestage-helper-$Nonce-$OperationSlug"
    $upload = "/home/ubuntu/.clearra-v080-prestage-helper-$Nonce-$OperationSlug"
    $unit = "clearra-v080-prestage-helper-$Nonce-$OperationSlug-cleanup"
    return [pscustomobject]@{
        Nonce = $Nonce
        OperationSlug = $OperationSlug
        CleanupFailure = $CleanupFailure
        NodeOutputMode = $NodeOutputMode
        WatchdogArmState = $WatchdogArmState
        ServiceStopBehavior = $ServiceStopBehavior
        RootPath = $root
        UploadRoot = $upload
        CleanupTimer = "$unit.timer"
        CleanupService = "$unit.service"
        Paths = @{}
        Units = @{}
        Events = [Collections.Generic.List[string]]::new()
        ArmEventIndex = -1
        FirstTransportMutationEventIndex = -1
        ScpCount = 0
        UploadFileAuthorityChecks = 0
        UploadFileMetadataChecks = 0
        UploadFileDigestChecks = 0
        RootFileAuthorityChecks = 0
        RootFileMetadataChecks = 0
        RootFileDigestChecks = 0
        UploadInventoryChecks = 0
        RootInventoryChecks = 0
        NodeInvocationCount = 0
        SharedFlockEnvObserved = $false
        TimerStopInvocationCount = 0
        ServiceStateReadbackCount = 0
        ServiceStopInvocationCount = 0
        WatchdogAbsenceReadbackCount = 0
    }
}

function Add-PrestageRemoteEvent {
    param(
        [Parameter(Mandatory = $true)] $Model,
        [Parameter(Mandatory = $true)][string] $Kind,
        [Parameter(Mandatory = $true)][object[]] $Arguments
    )
    [string[]]$tokens = @($Arguments | ForEach-Object { [string]$_ })
    [void]$Model.Events.Add("$Kind$([char]30)$($tokens -join [char]31)")
    return $Model.Events.Count - 1
}

function Set-PrestageRemotePath {
    param(
        [Parameter(Mandatory = $true)] $Model,
        [Parameter(Mandatory = $true)][string] $Path,
        [Parameter(Mandatory = $true)][string] $Type,
        [Parameter(Mandatory = $true)][int] $Uid,
        [Parameter(Mandatory = $true)][int] $Gid,
        [Parameter(Mandatory = $true)][string] $Mode,
        [long] $Size = 0,
        [string] $Sha256 = '',
        [int] $Nlink = 1
    )
    $Model.Paths[$Path] = [pscustomobject]@{
        Type = $Type
        Uid = $Uid
        Gid = $Gid
        Mode = $Mode
        Size = $Size
        Sha256 = $Sha256
        Nlink = $Nlink
    }
}

function Remove-PrestageRemoteTree {
    param(
        [Parameter(Mandatory = $true)] $Model,
        [Parameter(Mandatory = $true)][string] $Root
    )
    foreach ($path in @($Model.Paths.Keys)) {
        if ($path -ceq $Root -or $path.StartsWith("$Root/", [StringComparison]::Ordinal)) {
            [void]$Model.Paths.Remove($path)
        }
    }
}

function Get-PrestageImmediateChildren {
    param(
        [Parameter(Mandatory = $true)] $Model,
        [Parameter(Mandatory = $true)][string] $Root
    )
    return @($Model.Paths.Keys | Where-Object {
        if (-not $_.StartsWith("$Root/", [StringComparison]::Ordinal)) { return $false }
        return $_.Substring($Root.Length + 1).IndexOf('/') -lt 0
    })
}

function Invoke-PrestageRemoteCommand {
    param([Parameter(Mandatory = $true)][object[]] $Command)
    $model = $global:clearraOraclePrestageRemote
    if ($null -eq $model) {
        throw 'Prestage remote command model is not active.'
    }
    [string[]]$tokens = @($Command | ForEach-Object { [string]$_ })
    $eventIndex = Add-PrestageRemoteEvent -Model $model -Kind 'ssh' -Arguments $tokens
    $root = [string]$model.RootPath
    $upload = [string]$model.UploadRoot
    $timer = [string]$model.CleanupTimer
    $service = [string]$model.CleanupService
    $releaseLock = '/run/lock/clearra-oracle-release-deploy.lock'

    if (Test-ExactCommand -Actual $tokens -Expected @(
        'sudo', '-n', '/usr/bin/systemctl', 'list-units', '--all',
        '--full', '--plain', '--no-legend', $timer, $service
    )) {
        $loaded = [Collections.Generic.List[string]]::new()
        foreach ($unit in @($timer, $service)) {
            if ($model.Units.ContainsKey($unit)) {
                [void]$loaded.Add("$unit loaded active running")
            }
        }
        if ($model.TimerStopInvocationCount -gt 0) {
            if ($model.ServiceStateReadbackCount -eq 0) {
                $model.ServiceStateReadbackCount += 1
            } elseif ($loaded.Count -eq 0) {
                $model.WatchdogAbsenceReadbackCount += 1
            }
        }
        return New-PrestageRemoteResult -Output @($loaded)
    }

    if (Test-ExactCommand -Actual $tokens -Expected @(
        'sudo', '-n', '/usr/bin/systemd-run', '--quiet', '--collect',
        "--unit=$($timer.Substring(0, $timer.Length - 6))", '--on-active=30m',
        '/usr/bin/flock', $releaseLock, '/usr/bin/rm', '-rf', '--', $root, $upload
    )) {
        if ($model.Paths.Count -ne 0 -or $model.Units.Count -ne 0) {
            throw 'Prestage watchdog was not the first remote state mutation.'
        }
        $model.Units[$timer] = $true
        if ($model.WatchdogArmState -ceq 'service-loaded') {
            $model.Units[$service] = $true
        }
        $model.ArmEventIndex = $eventIndex
        return New-PrestageRemoteResult
    }

    if (Test-ExactCommand -Actual $tokens -Expected @(
        'sudo', '-n', '/usr/bin/systemctl', 'show', '--property=Id', '--value', $timer
    )) {
        if (-not $model.Units.ContainsKey($timer)) { return New-PrestageRemoteResult -ExitCode 4 }
        return New-PrestageRemoteResult -Output @($timer)
    }
    if (Test-ExactCommand -Actual $tokens -Expected @(
        'sudo', '-n', '/usr/bin/systemctl', 'show', '--property=ActiveState', '--value', $timer
    )) {
        if (-not $model.Units.ContainsKey($timer)) { return New-PrestageRemoteResult -ExitCode 4 }
        return New-PrestageRemoteResult -Output @('active')
    }

    if (Test-ExactCommand -Actual $tokens -Expected @(
        '/usr/bin/find', '/home/ubuntu', '-maxdepth', '1',
        '-name', [IO.Path]::GetFileName($upload), '-print'
    )) {
        $output = if ($model.Paths.ContainsKey($upload)) { @($upload) } else { @() }
        return New-PrestageRemoteResult -Output $output
    }
    if (Test-ExactCommand -Actual $tokens -Expected @(
        'sudo', '-n', '/usr/bin/find', '/opt/clearra', '-maxdepth', '1',
        '-name', [IO.Path]::GetFileName($root), '-print'
    )) {
        $output = if ($model.Paths.ContainsKey($root)) { @($root) } else { @() }
        return New-PrestageRemoteResult -Output $output
    }

    if (Test-ExactCommand -Actual $tokens -Expected @(
        '/usr/bin/mkdir', '-m', '0700', '--', $upload
    )) {
        if ($model.ArmEventIndex -lt 0 -or $model.Paths.ContainsKey($upload)) {
            return New-PrestageRemoteResult -ExitCode 17
        }
        if ($model.FirstTransportMutationEventIndex -lt 0) {
            $model.FirstTransportMutationEventIndex = $eventIndex
        }
        Set-PrestageRemotePath -Model $model -Path $upload -Type 'directory' `
            -Uid 1001 -Gid 1001 -Mode '700'
        return New-PrestageRemoteResult
    }

    if (Test-ExactCommand -Actual $tokens -Expected @(
        '/usr/bin/stat', '-c', '%u:%g:%a', '--', $upload
    )) {
        $entry = $model.Paths[$upload]
        if ($null -eq $entry -or $entry.Type -cne 'directory') {
            return New-PrestageRemoteResult -ExitCode 2
        }
        return New-PrestageRemoteResult -Output @("$($entry.Uid):$($entry.Gid):$($entry.Mode)")
    }
    if (Test-ExactCommand -Actual $tokens -Expected @(
        '/usr/bin/readlink', '-f', '--', $upload
    )) {
        if (-not $model.Paths.ContainsKey($upload)) { return New-PrestageRemoteResult -ExitCode 2 }
        return New-PrestageRemoteResult -Output @($upload)
    }

    if ($tokens.Count -eq 4 -and
        $tokens[0] -ceq '/usr/bin/chmod' -and $tokens[1] -ceq '0600' -and
        $tokens[2] -ceq '--' -and $tokens[3].StartsWith("$upload/", [StringComparison]::Ordinal)) {
        $path = $tokens[3]
        $entry = $model.Paths[$path]
        if ($null -eq $entry -or $entry.Type -cne 'file') {
            return New-PrestageRemoteResult -ExitCode 2
        }
        $entry.Mode = '600'
        return New-PrestageRemoteResult
    }

    if ($tokens.Count -eq 13 -and $tokens[0] -ceq '/usr/bin/find' -and
        $tokens[1].StartsWith("$upload/", [StringComparison]::Ordinal)) {
        $path = $tokens[1]
        $expected = @(
            '/usr/bin/find', $path, '-maxdepth', '0', '-type', 'f', '-links', '1',
            '-uid', '1001', '-gid', '1001', '-print'
        )
        if (-not (Test-ExactCommand -Actual $tokens -Expected $expected)) {
            throw 'Upload file authority command drifted.'
        }
        $entry = $model.Paths[$path]
        if ($null -ne $entry -and $entry.Type -ceq 'file' -and
            $entry.Nlink -eq 1 -and $entry.Uid -eq 1001 -and $entry.Gid -eq 1001) {
            $model.UploadFileAuthorityChecks += 1
            return New-PrestageRemoteResult -Output @($path)
        }
        return New-PrestageRemoteResult
    }
    if ($tokens.Count -eq 5 -and $tokens[0] -ceq '/usr/bin/stat' -and
        $tokens[1] -ceq '-c' -and $tokens[2] -ceq '%u:%g:%a:%s:%h' -and
        $tokens[3] -ceq '--' -and $tokens[4].StartsWith("$upload/", [StringComparison]::Ordinal)) {
        $entry = $model.Paths[$tokens[4]]
        if ($null -eq $entry) { return New-PrestageRemoteResult -ExitCode 2 }
        $model.UploadFileMetadataChecks += 1
        return New-PrestageRemoteResult -Output @(
            "$($entry.Uid):$($entry.Gid):$($entry.Mode):$($entry.Size):$($entry.Nlink)"
        )
    }
    if ($tokens.Count -eq 3 -and $tokens[0] -ceq '/usr/bin/sha256sum' -and
        $tokens[1] -ceq '--' -and $tokens[2].StartsWith("$upload/", [StringComparison]::Ordinal)) {
        $entry = $model.Paths[$tokens[2]]
        if ($null -eq $entry) { return New-PrestageRemoteResult -ExitCode 2 }
        $model.UploadFileDigestChecks += 1
        return New-PrestageRemoteResult -Output @("$($entry.Sha256)  $($tokens[2])")
    }
    if (Test-ExactCommand -Actual $tokens -Expected @(
        '/usr/bin/find', $upload, '-mindepth', '1', '-maxdepth', '1', '-print'
    )) {
        $model.UploadInventoryChecks += 1
        return New-PrestageRemoteResult -Output @(
            Get-PrestageImmediateChildren -Model $model -Root $upload
        )
    }

    if ($tokens.Count -eq 7 -and $tokens[0] -ceq 'sudo' -and $tokens[1] -ceq '-n' -and
        $tokens[2] -ceq '/usr/bin/mkdir' -and $tokens[3] -ceq '-m' -and
        $tokens[5] -ceq '--' -and
        ($tokens[6] -ceq $root -or $tokens[6].StartsWith("$root/", [StringComparison]::Ordinal))) {
        $path = $tokens[6]
        $mode = $tokens[4]
        if (($path -ceq $root -and $mode -cne '0700') -or
            ($path -cne $root -and $mode -cne '0755')) {
            throw 'Root helper directory mode drifted.'
        }
        if ($model.FirstTransportMutationEventIndex -lt 0) {
            $model.FirstTransportMutationEventIndex = $eventIndex
        }
        Set-PrestageRemotePath -Model $model -Path $path -Type 'directory' `
            -Uid 0 -Gid 0 -Mode $(if ($path -ceq $root) { '700' } else { '755' })
        return New-PrestageRemoteResult
    }

    if ($tokens.Count -eq 7 -and $tokens[0] -ceq 'sudo' -and $tokens[1] -ceq '-n' -and
        $tokens[2] -ceq '/usr/bin/stat' -and $tokens[3] -ceq '-c' -and
        $tokens[4] -ceq '%u:%g:%a' -and $tokens[5] -ceq '--') {
        $entry = $model.Paths[$tokens[6]]
        if ($null -eq $entry -or $entry.Type -cne 'directory') {
            return New-PrestageRemoteResult -ExitCode 2
        }
        return New-PrestageRemoteResult -Output @("$($entry.Uid):$($entry.Gid):$($entry.Mode)")
    }
    if ($tokens.Count -eq 6 -and $tokens[0] -ceq 'sudo' -and $tokens[1] -ceq '-n' -and
        $tokens[2] -ceq '/usr/bin/readlink' -and $tokens[3] -ceq '-f' -and
        $tokens[4] -ceq '--') {
        $path = $tokens[5]
        if (-not $model.Paths.ContainsKey($path)) { return New-PrestageRemoteResult -ExitCode 2 }
        return New-PrestageRemoteResult -Output @($path)
    }

    if ($tokens.Count -eq 12 -and $tokens[0] -ceq 'sudo' -and $tokens[1] -ceq '-n' -and
        $tokens[2] -ceq '/usr/bin/install') {
        $source = $tokens[10]
        $destination = $tokens[11]
        $expected = @(
            'sudo', '-n', '/usr/bin/install', '-o', 'root', '-g', 'root',
            '-m', '0644', '--', $source, $destination
        )
        if (-not (Test-ExactCommand -Actual $tokens -Expected $expected) -or
            -not $source.StartsWith("$upload/", [StringComparison]::Ordinal) -or
            -not $destination.StartsWith("$root/", [StringComparison]::Ordinal)) {
            throw 'Root helper install command drifted.'
        }
        $entry = $model.Paths[$source]
        if ($null -eq $entry -or $entry.Type -cne 'file' -or $entry.Mode -cne '600') {
            return New-PrestageRemoteResult -ExitCode 2
        }
        Set-PrestageRemotePath -Model $model -Path $destination -Type 'file' `
            -Uid 0 -Gid 0 -Mode '644' -Size $entry.Size -Sha256 $entry.Sha256
        return New-PrestageRemoteResult
    }

    if ($tokens.Count -eq 15 -and $tokens[0] -ceq 'sudo' -and $tokens[1] -ceq '-n' -and
        $tokens[2] -ceq '/usr/bin/find' -and
        $tokens[3].StartsWith("$root/", [StringComparison]::Ordinal)) {
        $path = $tokens[3]
        $expected = @(
            'sudo', '-n', '/usr/bin/find', $path, '-maxdepth', '0', '-type', 'f',
            '-links', '1', '-uid', '0', '-gid', '0', '-print'
        )
        if (-not (Test-ExactCommand -Actual $tokens -Expected $expected)) {
            throw 'Root file authority command drifted.'
        }
        $entry = $model.Paths[$path]
        if ($null -ne $entry -and $entry.Type -ceq 'file' -and
            $entry.Nlink -eq 1 -and $entry.Uid -eq 0 -and $entry.Gid -eq 0) {
            $model.RootFileAuthorityChecks += 1
            return New-PrestageRemoteResult -Output @($path)
        }
        return New-PrestageRemoteResult
    }
    if ($tokens.Count -eq 7 -and $tokens[0] -ceq 'sudo' -and $tokens[1] -ceq '-n' -and
        $tokens[2] -ceq '/usr/bin/stat' -and $tokens[3] -ceq '-c' -and
        $tokens[4] -ceq '%u:%g:%a:%s:%h' -and $tokens[5] -ceq '--') {
        $entry = $model.Paths[$tokens[6]]
        if ($null -eq $entry -or $entry.Type -cne 'file') {
            return New-PrestageRemoteResult -ExitCode 2
        }
        $model.RootFileMetadataChecks += 1
        return New-PrestageRemoteResult -Output @(
            "$($entry.Uid):$($entry.Gid):$($entry.Mode):$($entry.Size):$($entry.Nlink)"
        )
    }
    if ($tokens.Count -eq 5 -and $tokens[0] -ceq 'sudo' -and $tokens[1] -ceq '-n' -and
        $tokens[2] -ceq '/usr/bin/sha256sum' -and $tokens[3] -ceq '--' -and
        $tokens[4].StartsWith("$root/", [StringComparison]::Ordinal)) {
        $entry = $model.Paths[$tokens[4]]
        if ($null -eq $entry) { return New-PrestageRemoteResult -ExitCode 2 }
        $model.RootFileDigestChecks += 1
        return New-PrestageRemoteResult -Output @("$($entry.Sha256)  $($tokens[4])")
    }
    if (Test-ExactCommand -Actual $tokens -Expected @(
        'sudo', '-n', '/usr/bin/find', $root, '-mindepth', '1', '-print'
    )) {
        $model.RootInventoryChecks += 1
        return New-PrestageRemoteResult -Output @(
            $model.Paths.Keys | Where-Object {
                $_.StartsWith("$root/", [StringComparison]::Ordinal)
            }
        )
    }

    $main = "$root/apps/clearra-discord-bot/scripts/capture-oracle-rollback-authority.mjs"
    $nodePrefix = @(
        'sudo', '-n', '/usr/bin/flock', '-n', $releaseLock,
        '/usr/bin/env', '-i',
        'PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin',
        'HOME=/root', '/usr/bin/node', $main
    )
    if (Test-CommandPrefix -Actual $tokens -Expected $nodePrefix) {
        $entry = $model.Paths[$main]
        if ($null -eq $entry -or $entry.Type -cne 'file' -or
            $entry.Uid -ne 0 -or $entry.Gid -ne 0 -or $entry.Mode -cne '644' -or
            $model.ScpCount -ne 4 -or $model.UploadFileAuthorityChecks -ne 4 -or
            $model.UploadFileMetadataChecks -ne 4 -or
            $model.UploadFileDigestChecks -ne 4 -or
            $model.RootFileAuthorityChecks -ne 4 -or
            $model.RootFileMetadataChecks -ne 4 -or
            $model.RootFileDigestChecks -ne 4 -or
            $model.UploadInventoryChecks -ne 1 -or $model.RootInventoryChecks -ne 1) {
            throw 'Prestage Node execution preceded exact transport verification.'
        }
        [string[]]$operationArguments = @($tokens[$nodePrefix.Count..($tokens.Count - 1)])
        if ($model.OperationSlug -ceq 'capture') {
            $expectedArguments = @(
                '--prior-revision', 'clearra-current-job-v075-042ec21',
                '--prior-runtime-authority-kind', 'clearra.rollback.legacy-health-no-runtime.v1',
                '--deployment-nonce', [string]$model.Nonce
            )
            if (-not (Test-ExactCommand -Actual $operationArguments -Expected $expectedArguments)) {
                throw 'Prestage capture Node argument contract drifted.'
            }
            $payload = [ordered]@{
                priorRevision = 'clearra-current-job-v075-042ec21'
                priorOracleRelease = '/opt/clearra/releases/v0.7.4-042ec21'
                priorOracleReleaseId = 'v0.7.4-042ec21'
                priorOracleReleaseSha256 = ('d' * 64)
                priorOracleSettingsBackup = "/etc/clearra-gateway/settings.pre-v0.8.0-$($model.Nonce)"
                priorOracleSettingsSha256 = ('e' * 64)
                priorRuntimeAuthorityKind = 'clearra.rollback.legacy-health-no-runtime.v1'
                priorRuntimeAuthoritySha256 = ('f' * 64)
                priorJobUrl = 'https://prior.example.test/jobs'
                deploymentNonce = [string]$model.Nonce
            }
        } else {
            $expectedArguments = @('--cleanup-deployment-nonce', [string]$model.Nonce)
            if (-not (Test-ExactCommand -Actual $operationArguments -Expected $expectedArguments)) {
                throw 'Prestage cleanup Node argument contract drifted.'
            }
            $payload = [ordered]@{
                deploymentNonce = [string]$model.Nonce
                backupRemoved = $true
            }
        }
        $model.NodeInvocationCount += 1
        $model.SharedFlockEnvObserved = $true
        $line = $payload | ConvertTo-Json -Compress
        $output = if ($model.NodeOutputMode -ceq 'duplicate') { @($line, $line) } else { @($line) }
        return New-PrestageRemoteResult -Output $output
    }

    if (Test-ExactCommand -Actual $tokens -Expected @(
        'sudo', '-n', '/usr/bin/rm', '-rf', '--', $root
    )) {
        if ($model.CleanupFailure -ceq 'root') {
            return New-PrestageRemoteResult -ExitCode 91
        }
        Remove-PrestageRemoteTree -Model $model -Root $root
        return New-PrestageRemoteResult
    }
    if (Test-ExactCommand -Actual $tokens -Expected @(
        '/usr/bin/rm', '-rf', '--', $upload
    )) {
        if ($model.CleanupFailure -ceq 'upload') {
            return New-PrestageRemoteResult -ExitCode 92
        }
        Remove-PrestageRemoteTree -Model $model -Root $upload
        return New-PrestageRemoteResult
    }
    if (Test-ExactCommand -Actual $tokens -Expected @(
        'sudo', '-n', '/usr/bin/systemctl', 'stop', $timer
    )) {
        if (-not $model.Units.ContainsKey($timer)) {
            return New-PrestageRemoteResult -ExitCode 5
        }
        $model.TimerStopInvocationCount += 1
        [void]$model.Units.Remove($timer)
        return New-PrestageRemoteResult
    }
    if (Test-ExactCommand -Actual $tokens -Expected @(
        'sudo', '-n', '/usr/bin/systemctl', 'stop', $service
    )) {
        $model.ServiceStopInvocationCount += 1
        if ($model.ServiceStopBehavior -ceq 'collected-before-stop') {
            [void]$model.Units.Remove($service)
            return New-PrestageRemoteResult -ExitCode 5
        }
        if ($model.ServiceStopBehavior -ceq 'failed-still-loaded') {
            return New-PrestageRemoteResult -ExitCode 5
        }
        if (-not $model.Units.ContainsKey($service)) {
            return New-PrestageRemoteResult -ExitCode 5
        }
        [void]$model.Units.Remove($service)
        return New-PrestageRemoteResult
    }

    throw "Unexpected prestage remote command: $($tokens -join ' ')"
}

function ssh {
    $expectedSshConfigPath = if (
        [Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
            [Runtime.InteropServices.OSPlatform]::Windows
        )
    ) { 'NUL' } else { '/dev/null' }
    [string[]]$expectedPrefix = @(
        Get-ExpectedOracleCommonClientArguments `
            -SshConfigPath $expectedSshConfigPath `
            -IdentityFile $global:clearraOracleTestLockedIdentityPath `
            -KnownHostsPath $global:clearraOracleTestExpectedKnownHostsPath
    ) + @('ubuntu@157.151.254.175')
    [object[]]$remoteCommand = @(
        Get-ExactOracleClientTail `
            -Actual ([object[]]$args) -ExpectedPrefix $expectedPrefix `
            -Label 'Oracle SSH invocation' -MinimumTailCount 1
    )
    if ($null -ne $global:clearraOraclePrestageRemote) {
        $result = Invoke-PrestageRemoteCommand -Command $remoteCommand
        $global:LASTEXITCODE = [int]$result.ExitCode
        @($result.Output)
        return
    }
    $global:clearraOracleTestMockSshInvocationCount += 1
    $global:LASTEXITCODE = 0
    $global:clearraOracleTestMockOutput
}

function scp {
    if ($null -eq $global:clearraOraclePrestageRemote) {
        throw 'Unexpected SCP invocation outside the prestage remote model.'
    }
    $expectedSshConfigPath = if (
        [Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
            [Runtime.InteropServices.OSPlatform]::Windows
        )
    ) { 'NUL' } else { '/dev/null' }
    [string[]]$expectedPrefix = @('-q') + @(
        Get-ExpectedOracleCommonClientArguments `
            -SshConfigPath $expectedSshConfigPath `
            -IdentityFile $global:clearraOracleTestLockedIdentityPath `
            -KnownHostsPath $global:clearraOracleTestExpectedKnownHostsPath
    )
    [object[]]$tail = @(
        Get-ExactOracleClientTail `
            -Actual ([object[]]$args) -ExpectedPrefix $expectedPrefix `
            -Label 'Oracle SCP invocation' -MinimumTailCount 3 -ExactTailCount 3
    )
    if ($tail.Count -ne 3 -or [string]$tail[0] -cne '--') {
        throw 'Prestage SCP invocation drifted from its exact shell-free boundary.'
    }
    $localPath = [string]$tail[1]
    $destination = [string]$tail[2]
    $match = [regex]::Match(
        $destination,
        '^ubuntu@157\.151\.254\.175:(/home/ubuntu/\.clearra-v080-prestage-helper-[0-9a-f]{64}-(?:capture|cleanup)/[A-Za-z0-9._-]{1,128})$'
    )
    if (-not $match.Success) {
        throw 'Prestage SCP destination escaped its nonce namespace.'
    }
    $remotePath = $match.Groups[1].Value
    $model = $global:clearraOraclePrestageRemote
    if (-not $remotePath.StartsWith("$($model.UploadRoot)/", [StringComparison]::Ordinal) -or
        -not $model.Paths.ContainsKey([string]$model.UploadRoot) -or
        $model.Paths.ContainsKey($remotePath)) {
        throw 'Prestage SCP state differs from the sealed upload root.'
    }
    $item = Get-Item -LiteralPath $localPath -Force
    if ($item.PSIsContainer -or
        (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        throw 'Prestage SCP source is not a regular non-link file.'
    }
    [void](Add-PrestageRemoteEvent -Model $model -Kind 'scp' -Arguments @($localPath, $remotePath))
    Set-PrestageRemotePath -Model $model -Path $remotePath -Type 'file' `
        -Uid 1001 -Gid 1001 -Mode '644' -Size $item.Length `
        -Sha256 (Get-FileHash -Algorithm SHA256 -LiteralPath $localPath).Hash.ToLowerInvariant()
    $model.ScpCount += 1
    $global:LASTEXITCODE = 0
}

function Invoke-CaptureEvidenceFile {
    param([Parameter(Mandatory = $true)][string] $OutputPath)
    return @(& $wrapper `
        -Operation capture-rollback-authority `
        -ScriptReleaseId $scriptReleaseId `
        -ScriptReleaseSha256 $scriptReleaseSha256 `
        -PriorRevision $captureObject.priorRevision `
        -PriorRuntimeAuthorityKind $captureObject.priorRuntimeAuthorityKind `
        -DeploymentNonce $deploymentNonce `
        -EvidenceOutput $OutputPath `
        -IdentityFile $lockedIdentityPath)
}

function Assert-CaptureEvidencePathRejected {
    param(
        [Parameter(Mandatory = $true)][string] $Path,
        [Parameter(Mandatory = $true)][string] $ErrorPattern,
        [Parameter(Mandatory = $true)][string] $Label
    )
    $rejected = $false
    try {
        [void](Invoke-CaptureEvidenceFile -OutputPath $Path)
    } catch {
        $rejected = $_.Exception.Message -like $ErrorPattern
    }
    if (-not $rejected) {
        throw "Oracle evidence output accepted $Label."
    }
}

function Invoke-PrestageFixtureGit {
    param(
        [Parameter(Mandatory = $true)][string] $Root,
        [Parameter(Mandatory = $true)][string[]] $GitArguments
    )
    $output = @(& git -C $Root @GitArguments)
    if ($LASTEXITCODE -ne 0) {
        throw "Prestage fixture Git command failed: $($GitArguments[0])"
    }
    return $output
}

function New-PrestageGitFixture {
    param(
        [Parameter(Mandatory = $true)][string] $SourceRoot,
        [Parameter(Mandatory = $true)][string] $TargetRoot
    )
    if ([IO.Directory]::Exists($TargetRoot) -or [IO.File]::Exists($TargetRoot)) {
        throw 'Prestage fixture root must be new.'
    }
    [IO.Directory]::CreateDirectory($TargetRoot) | Out-Null
    $fixtureFiles = @(
        'scripts/release/oracle/invoke-release-deploy-v080.ps1',
        'scripts/release/oracle/clearra-oracle-known-hosts',
        'scripts/release/oracle/clearra-oracle-release-deploy-v080',
        'scripts/release/oracle/create-prestage-helper-bundle.mjs',
        'apps/clearra-discord-bot/scripts/capture-oracle-rollback-authority.mjs',
        'apps/clearra-discord-bot/scripts/oracle-runtime-authority.mjs',
        'apps/clearra-discord-bot/scripts/release-tree-digest.mjs',
        'apps/clearra-discord-bot/src/job-service/runtime-identity.mjs'
    )
    foreach ($relativePath in $fixtureFiles) {
        $source = [IO.Path]::Combine($SourceRoot, $relativePath.Replace('/', [IO.Path]::DirectorySeparatorChar))
        $destination = [IO.Path]::Combine($TargetRoot, $relativePath.Replace('/', [IO.Path]::DirectorySeparatorChar))
        $parent = [IO.Path]::GetDirectoryName($destination)
        [IO.Directory]::CreateDirectory($parent) | Out-Null
        [IO.File]::Copy($source, $destination, $false)
    }
    [void](Invoke-PrestageFixtureGit -Root $TargetRoot -GitArguments @(
        'init', '--quiet', '--initial-branch=main'
    ))
    foreach ($setting in @(
        @('user.name', 'Clearra Prestage Test'),
        @('user.email', 'clearra-prestage@example.invalid'),
        @('commit.gpgSign', 'false'),
        @('core.autocrlf', 'false'),
        @('core.filemode', 'false')
    )) {
        [void](Invoke-PrestageFixtureGit -Root $TargetRoot -GitArguments @(
            'config', [string]$setting[0], [string]$setting[1]
        ))
    }
    [void](Invoke-PrestageFixtureGit -Root $TargetRoot -GitArguments @('add', '--', '.'))
    [void](Invoke-PrestageFixtureGit -Root $TargetRoot -GitArguments @(
        'commit', '--quiet', '-m', 'sealed prestage transport fixture'
    ))
    $commit = @(Invoke-PrestageFixtureGit -Root $TargetRoot -GitArguments @(
        'rev-parse', 'HEAD'
    ))
    if ($commit.Count -ne 1 -or $commit[0] -cnotmatch '^[0-9a-f]{40}$') {
        throw 'Prestage fixture source commit is invalid.'
    }
    return [pscustomobject]@{
        Root = $TargetRoot
        Wrapper = Join-Path $TargetRoot 'scripts/release/oracle/invoke-release-deploy-v080.ps1'
        SourceCommit = [string]$commit[0]
    }
}

function Invoke-ModeledPrestageWrapper {
    param(
        [Parameter(Mandatory = $true)] $Fixture,
        [Parameter(Mandatory = $true)] $Model,
        [Parameter(Mandatory = $true)][string] $IdentityFile
    )
    $global:clearraOraclePrestageRemote = $Model
    $previousKnownHostsPath = $global:clearraOracleTestExpectedKnownHostsPath
    $global:clearraOracleTestExpectedKnownHostsPath = Join-Path `
        $Fixture.Root 'scripts/release/oracle/clearra-oracle-known-hosts'
    try {
        if ($Model.OperationSlug -ceq 'capture') {
            return @(& $Fixture.Wrapper `
                -Operation capture-prestage-authority `
                -SourceCommit $Fixture.SourceCommit `
                -PriorRevision 'clearra-current-job-v075-042ec21' `
                -PriorRuntimeAuthorityKind 'clearra.rollback.legacy-health-no-runtime.v1' `
                -DeploymentNonce $Model.Nonce `
                -IdentityFile $IdentityFile)
        }
        return @(& $Fixture.Wrapper `
            -Operation cleanup-prestage-backup `
            -SourceCommit $Fixture.SourceCommit `
            -DeploymentNonce $Model.Nonce `
            -IdentityFile $IdentityFile)
    } finally {
        $global:clearraOracleTestExpectedKnownHostsPath = $previousKnownHostsPath
        $global:clearraOraclePrestageRemote = $null
    }
}

function Assert-ModeledPrestageTransport {
    param(
        [Parameter(Mandatory = $true)] $Model,
        [Parameter(Mandatory = $true)][string] $Label
    )
    if ($Model.ArmEventIndex -lt 0 -or
        $Model.FirstTransportMutationEventIndex -lt 0 -or
        $Model.ArmEventIndex -ge $Model.FirstTransportMutationEventIndex) {
        throw "$Label did not arm its watchdog before the first transport-path mutation."
    }
    if ($Model.ScpCount -ne 4 -or
        $Model.UploadFileAuthorityChecks -ne 4 -or
        $Model.UploadFileMetadataChecks -ne 4 -or
        $Model.UploadFileDigestChecks -ne 4 -or
        $Model.RootFileAuthorityChecks -ne 4 -or
        $Model.RootFileMetadataChecks -ne 4 -or
        $Model.RootFileDigestChecks -ne 4 -or
        $Model.UploadInventoryChecks -ne 1 -or
        $Model.RootInventoryChecks -ne 1) {
        throw "$Label did not verify the exact four-file remote inventory and authority."
    }
    if ($Model.NodeInvocationCount -ne 1 -or -not $Model.SharedFlockEnvObserved) {
        throw "$Label did not execute once through the shared flock and clean environment."
    }
}

function Assert-ModeledPrestageSuccess {
    param(
        [Parameter(Mandatory = $true)] $Model,
        [Parameter(Mandatory = $true)][string] $Label
    )
    Assert-ModeledPrestageTransport -Model $Model -Label $Label
    $expectedServiceStops = if ($Model.WatchdogArmState -ceq 'service-loaded') { 1 } else { 0 }
    if ($Model.Paths.Count -ne 0 -or $Model.Units.Count -ne 0 -or
        $Model.TimerStopInvocationCount -ne 1 -or
        $Model.ServiceStateReadbackCount -ne 1 -or
        $Model.ServiceStopInvocationCount -ne $expectedServiceStops -or
        $Model.WatchdogAbsenceReadbackCount -ne 1) {
        throw "$Label did not stop the timer first, conditionally stop its loaded service, and read back both watchdog units absent."
    }
}

function Assert-ModeledPrestageCleanupFailure {
    param(
        [Parameter(Mandatory = $true)] $Model,
        [Parameter(Mandatory = $true)][string] $Label
    )
    Assert-ModeledPrestageTransport -Model $Model -Label $Label
    if ($Model.TimerStopInvocationCount -ne 0 -or
        $Model.ServiceStateReadbackCount -ne 0 -or
        $Model.ServiceStopInvocationCount -ne 0 -or
        $Model.WatchdogAbsenceReadbackCount -ne 0 -or
        -not $Model.Units.ContainsKey([string]$Model.CleanupTimer) -or
        -not $Model.Units.ContainsKey([string]$Model.CleanupService)) {
        throw "$Label changed its loaded timer or service after transport cleanup failed."
    }
    if ($Model.CleanupFailure -ceq 'root') {
        if (-not $Model.Paths.ContainsKey([string]$Model.RootPath) -or
            $Model.Paths.ContainsKey([string]$Model.UploadRoot)) {
            throw "$Label did not preserve only the failed root residue for its armed watchdog."
        }
    } elseif ($Model.CleanupFailure -ceq 'upload') {
        if ($Model.Paths.ContainsKey([string]$Model.RootPath) -or
            -not $Model.Paths.ContainsKey([string]$Model.UploadRoot)) {
            throw "$Label did not preserve only the failed upload residue for its armed watchdog."
        }
    } else {
        throw "$Label does not declare a cleanup failure."
    }
}

try {
    [IO.Directory]::CreateDirectory($evidenceRoot) | Out-Null
    [IO.Directory]::CreateDirectory($realParentPath) | Out-Null
    $parentItem = Get-Item -LiteralPath $evidenceRoot -Force
    if (-not $parentItem.PSIsContainer -or
        (($parentItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
        throw 'Oracle evidence test parent is not a regular, non-link directory.'
    }

    [IO.File]::WriteAllBytes($lockedIdentityPath, [byte[]](1..32))
    $identityLock = [IO.File]::Open(
        $lockedIdentityPath,
        [IO.FileMode]::Open,
        [IO.FileAccess]::ReadWrite,
        [IO.FileShare]::None
    )

    $captureObject = [ordered]@{
        priorRevision = 'clearra-current-job-v075-042ec21'
        priorOracleRelease = '/opt/clearra/releases/v0.7.5-042ec21'
        priorOracleReleaseId = 'v0.7.5-042ec21'
        priorOracleReleaseSha256 = ('d' * 64)
        priorOracleSettingsBackup = "/etc/clearra-gateway/settings.pre-v0.8.0-$deploymentNonce"
        priorOracleSettingsSha256 = ('e' * 64)
        priorRuntimeAuthorityKind = 'clearra.rollback.legacy-health-no-runtime.v1'
        priorRuntimeAuthoritySha256 = ('f' * 64)
        priorJobUrl = 'https://prior.example.test/jobs'
        deploymentNonce = $deploymentNonce
    }
    $global:clearraOracleTestMockOutput = $captureObject | ConvertTo-Json -Compress
    [void](Invoke-CaptureEvidenceFile -OutputPath $captureEvidencePath)
    $captureEvidence = Read-CanonicalEvidenceFile -Path $captureEvidencePath
    if ($captureEvidence.deploymentNonce -cne $deploymentNonce -or
        $captureEvidence.priorRevision -cne $captureObject.priorRevision) {
        throw 'Oracle rollback capture evidence changed validated identity fields.'
    }

    $observationObject = [ordered]@{
        contract = 'clearra.oracle.candidate-observation.v1'
        sourceCommit = $sourceCommit
        candidateUrl = $candidateUrl
        candidateRevision = $candidateRevision
        jobUrl = "$candidateUrl/jobs"
        oracleReleaseId = $scriptReleaseId
        activeReleasePath = "/opt/clearra/releases/$scriptReleaseId"
        oracleReleaseSha256 = $scriptReleaseSha256
        oracleSettingsSha256 = ('c' * 64)
        deploymentNonce = $deploymentNonce
        verifiedAfter = $verifiedAfter
        gatewayPid = 1234
        gatewayStartMonotonicUsec = 5678
        bootId = '12345678-1234-4234-8234-123456789abc'
        readyRecordObserved = $true
        freshOperationAt = '2026-08-30T00:00:01.000Z'
        observedAt = '2026-08-30T00:00:02.000Z'
        runtimeIdentity = [ordered]@{
            candidateRevision = $candidateRevision
            sourceCommit = $sourceCommit
        }
    }
    $global:clearraOracleTestMockOutput = $observationObject | ConvertTo-Json -Compress -Depth 4
    [void]@(& $wrapper `
        -Operation observe-candidate `
        -ScriptReleaseId $scriptReleaseId `
        -ScriptReleaseSha256 $scriptReleaseSha256 `
        -SourceCommit $sourceCommit `
        -CandidateUrl $candidateUrl `
        -CandidateRevision $candidateRevision `
        -OracleReleaseId $scriptReleaseId `
        -OracleReleaseSha256 $scriptReleaseSha256 `
        -OracleSettingsSha256 ('c' * 64) `
        -DeploymentNonce $deploymentNonce `
        -VerifiedAfter $verifiedAfter `
        -EvidenceOutput $observationEvidencePath `
        -IdentityFile $lockedIdentityPath)
    $observationEvidence = Read-CanonicalEvidenceFile -Path $observationEvidencePath
    if ($observationEvidence.freshOperationAt -cne '2026-08-30T00:00:01.000Z' -or
        $observationEvidence.observedAt -cne '2026-08-30T00:00:02.000Z' -or
        $observationEvidence.verifiedAfter -cne $verifiedAfter -or
        $observationEvidence.sourceCommit -cne $sourceCommit) {
        throw 'Oracle observation evidence did not preserve canonical UTC timestamps and source identity.'
    }

    $observationObject.verifiedAfter = '2026-08-30T00:00:00.001Z'
    $global:clearraOracleTestMockOutput = $observationObject | ConvertTo-Json -Compress -Depth 4
    $rejected = $false
    try {
        [void]@(& $wrapper `
            -Operation observe-candidate `
            -ScriptReleaseId $scriptReleaseId `
            -ScriptReleaseSha256 $scriptReleaseSha256 `
            -SourceCommit $sourceCommit `
            -CandidateUrl $candidateUrl `
            -CandidateRevision $candidateRevision `
            -OracleReleaseId $scriptReleaseId `
            -OracleReleaseSha256 $scriptReleaseSha256 `
            -OracleSettingsSha256 ('c' * 64) `
            -DeploymentNonce $deploymentNonce `
            -VerifiedAfter $verifiedAfter `
            -IdentityFile $lockedIdentityPath)
    } catch {
        $rejected = $_.Exception.Message -like '*invalid closed result*'
    }
    if (-not $rejected) {
        throw 'Oracle observation accepted a mismatched verified-after echo.'
    }
    $observationObject.verifiedAfter = $verifiedAfter
    foreach ($timestampDrift in @(
        @{
            Label = 'an operation before verified-after'
            FreshOperationAt = '2026-08-29T23:59:59.999Z'
            ObservedAt = '2026-08-30T00:00:02.000Z'
        },
        @{
            Label = 'an observation before its operation'
            FreshOperationAt = '2026-08-30T00:00:01.000Z'
            ObservedAt = '2026-08-30T00:00:00.999Z'
        }
    )) {
        $observationObject.freshOperationAt = $timestampDrift.FreshOperationAt
        $observationObject.observedAt = $timestampDrift.ObservedAt
        $global:clearraOracleTestMockOutput = $observationObject | ConvertTo-Json -Compress -Depth 4
        $rejected = $false
        try {
            [void]@(& $wrapper `
                -Operation observe-candidate `
                -ScriptReleaseId $scriptReleaseId `
                -ScriptReleaseSha256 $scriptReleaseSha256 `
                -SourceCommit $sourceCommit `
                -CandidateUrl $candidateUrl `
                -CandidateRevision $candidateRevision `
                -OracleReleaseId $scriptReleaseId `
                -OracleReleaseSha256 $scriptReleaseSha256 `
                -OracleSettingsSha256 ('c' * 64) `
                -DeploymentNonce $deploymentNonce `
                -VerifiedAfter $verifiedAfter `
                -IdentityFile $lockedIdentityPath)
        } catch {
            $rejected = $_.Exception.Message -like '*timestamps are out of order*'
        }
        if (-not $rejected) {
            throw "Oracle observation accepted $($timestampDrift.Label)."
        }
    }
    $observationObject.freshOperationAt = '2026-08-30T00:00:01.000Z'
    $observationObject.observedAt = '2026-08-30T00:00:02.000Z'
    $global:clearraOracleTestMockOutput = $observationObject | ConvertTo-Json -Compress -Depth 4

    $captureBytesBeforeRetry = [IO.File]::ReadAllBytes($captureEvidencePath)
    Assert-CaptureEvidencePathRejected `
        -Path $captureEvidencePath `
        -ErrorPattern '*must be a new path*' `
        -Label 'an existing path overwrite'
    $captureBytesAfterRetry = [IO.File]::ReadAllBytes($captureEvidencePath)
    if ([Convert]::ToHexString($captureBytesAfterRetry) -cne
        [Convert]::ToHexString($captureBytesBeforeRetry)) {
        throw 'Oracle evidence output changed after a rejected overwrite.'
    }

    Assert-CaptureEvidencePathRejected `
        -Path $relativeEvidencePath `
        -ErrorPattern '*must be an absolute path*' `
        -Label 'a relative path'

    if ([Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [Runtime.InteropServices.OSPlatform]::Windows
    )) {
        New-Item -ItemType Junction -Path $linkedParentPath -Target $realParentPath | Out-Null
    } else {
        New-Item -ItemType SymbolicLink -Path $linkedParentPath -Target $realParentPath | Out-Null
    }
    $linkCreated = $true
    $linkedParentItem = Get-Item -LiteralPath $linkedParentPath -Force
    if (($linkedParentItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -eq 0) {
        throw 'Oracle evidence test could not establish a linked parent boundary.'
    }
    Assert-CaptureEvidencePathRejected `
        -Path $linkedEvidencePath `
        -ErrorPattern '*must not traverse a reparse point*' `
        -Label 'a linked parent path'
    if (Test-Path -LiteralPath $linkedEvidenceTargetPath) {
        throw 'Oracle evidence output accepted a linked parent path.'
    }
    if ($global:clearraOracleTestMockSshInvocationCount -ne 5) {
        throw 'Oracle evidence boundary test unexpectedly invoked the SSH command path.'
    }

    $prestageFixture = New-PrestageGitFixture `
        -SourceRoot $repositoryRoot -TargetRoot $prestageFixtureRoot

    $prestageCapture = @(& $prestageFixture.Wrapper `
        -Operation capture-prestage-authority `
        -SourceCommit $prestageFixture.SourceCommit `
        -PriorRevision 'clearra-current-job-v075-042ec21' `
        -PriorRuntimeAuthorityKind 'clearra.rollback.legacy-health-no-runtime.v1' `
        -DeploymentNonce $deploymentNonce `
        -AuditOnly)
    Assert-AuditResult -Output $prestageCapture -Operation 'capture-prestage-authority'

    $prestageCleanup = @(& $prestageFixture.Wrapper `
        -Operation cleanup-prestage-backup `
        -SourceCommit $prestageFixture.SourceCommit `
        -DeploymentNonce $deploymentNonce `
        -AuditOnly)
    Assert-AuditResult -Output $prestageCleanup -Operation 'cleanup-prestage-backup'

    $modeledCapture = New-PrestageRemoteModel `
        -Nonce ('1' * 64) -OperationSlug capture
    $modeledCaptureOutput = @(
        Invoke-ModeledPrestageWrapper `
            -Fixture $prestageFixture -Model $modeledCapture `
            -IdentityFile $lockedIdentityPath
    )
    if ($modeledCaptureOutput.Count -ne 1) {
        throw 'Modeled prestage capture did not preserve one-line output cardinality.'
    }
    $modeledCaptureJson = $modeledCaptureOutput[0] | ConvertFrom-Json -DateKind String
    if ($modeledCaptureJson.deploymentNonce -cne $modeledCapture.Nonce -or
        $modeledCaptureJson.priorRevision -cne 'clearra-current-job-v075-042ec21') {
        throw 'Modeled prestage capture output identity drifted.'
    }
    Assert-ModeledPrestageSuccess `
        -Model $modeledCapture -Label 'Modeled prestage capture'

    $modeledCleanup = New-PrestageRemoteModel `
        -Nonce ('2' * 64) -OperationSlug cleanup
    $modeledCleanupOutput = @(
        Invoke-ModeledPrestageWrapper `
            -Fixture $prestageFixture -Model $modeledCleanup `
            -IdentityFile $lockedIdentityPath
    )
    if ($modeledCleanupOutput.Count -ne 1) {
        throw 'Modeled prestage backup cleanup did not preserve one-line output cardinality.'
    }
    $modeledCleanupJson = $modeledCleanupOutput[0] | ConvertFrom-Json -DateKind String
    if ($modeledCleanupJson.deploymentNonce -cne $modeledCleanup.Nonce -or
        $modeledCleanupJson.backupRemoved -cne $true) {
        throw 'Modeled prestage backup cleanup output identity drifted.'
    }
    Assert-ModeledPrestageSuccess `
        -Model $modeledCleanup -Label 'Modeled prestage backup cleanup'

    $loadedServiceModel = New-PrestageRemoteModel `
        -Nonce ('6' * 64) -OperationSlug cleanup -WatchdogArmState service-loaded
    $loadedServiceOutput = @(
        Invoke-ModeledPrestageWrapper `
            -Fixture $prestageFixture -Model $loadedServiceModel `
            -IdentityFile $lockedIdentityPath
    )
    if ($loadedServiceOutput.Count -ne 1) {
        throw 'Modeled loaded-service cleanup did not preserve one-line output cardinality.'
    }
    Assert-ModeledPrestageSuccess `
        -Model $loadedServiceModel -Label 'Modeled loaded-service cleanup'

    $collectedServiceModel = New-PrestageRemoteModel `
        -Nonce ('7' * 64) -OperationSlug cleanup -WatchdogArmState service-loaded `
        -ServiceStopBehavior collected-before-stop
    $collectedServiceOutput = @(
        Invoke-ModeledPrestageWrapper `
            -Fixture $prestageFixture -Model $collectedServiceModel `
            -IdentityFile $lockedIdentityPath
    )
    if ($collectedServiceOutput.Count -ne 1) {
        throw 'Modeled collected-service cleanup did not preserve one-line output cardinality.'
    }
    Assert-ModeledPrestageSuccess `
        -Model $collectedServiceModel -Label 'Modeled collected-service cleanup'

    $failedServiceStopModel = New-PrestageRemoteModel `
        -Nonce ('8' * 64) -OperationSlug cleanup -WatchdogArmState service-loaded `
        -ServiceStopBehavior failed-still-loaded
    $failedServiceStopMessage = $null
    try {
        [void]@(
            Invoke-ModeledPrestageWrapper `
                -Fixture $prestageFixture -Model $failedServiceStopModel `
                -IdentityFile $lockedIdentityPath
        )
    } catch {
        $failedServiceStopMessage = $_.Exception.Message
    }
    if ($failedServiceStopMessage -notlike
        '*Oracle prestage helper cleanup failed*cleanup watchdog unit residue remains*' -or
        $failedServiceStopModel.Paths.Count -ne 0 -or
        $failedServiceStopModel.TimerStopInvocationCount -ne 1 -or
        $failedServiceStopModel.ServiceStateReadbackCount -ne 1 -or
        $failedServiceStopModel.ServiceStopInvocationCount -ne 1 -or
        $failedServiceStopModel.WatchdogAbsenceReadbackCount -ne 0 -or
        $failedServiceStopModel.Units.ContainsKey([string]$failedServiceStopModel.CleanupTimer) -or
        -not $failedServiceStopModel.Units.ContainsKey([string]$failedServiceStopModel.CleanupService)) {
        throw 'Modeled loaded-service stop failure did not fail closed on final unit residue.'
    }

    foreach ($failureKind in @('root', 'upload')) {
        $failureNonce = if ($failureKind -ceq 'root') { '3' * 64 } else { '4' * 64 }
        $failureModel = New-PrestageRemoteModel `
            -Nonce $failureNonce -OperationSlug capture -CleanupFailure $failureKind `
            -WatchdogArmState service-loaded
        $failureMessage = $null
        try {
            [void]@(
                Invoke-ModeledPrestageWrapper `
                    -Fixture $prestageFixture -Model $failureModel `
                    -IdentityFile $lockedIdentityPath
            )
        } catch {
            $failureMessage = $_.Exception.Message
        }
        if ($failureMessage -notlike '*Oracle prestage helper cleanup failed*') {
            throw "Modeled $failureKind cleanup failure did not fail closed."
        }
        Assert-ModeledPrestageCleanupFailure `
            -Model $failureModel -Label "Modeled $failureKind cleanup failure"
    }

    $duplicateOutputModel = New-PrestageRemoteModel `
        -Nonce ('5' * 64) -OperationSlug capture -NodeOutputMode duplicate
    $duplicateOutputFailure = $null
    try {
        [void]@(
            Invoke-ModeledPrestageWrapper `
                -Fixture $prestageFixture -Model $duplicateOutputModel `
                -IdentityFile $lockedIdentityPath
        )
    } catch {
        $duplicateOutputFailure = $_.Exception.Message
    }
    if ($duplicateOutputFailure -notlike '*invalid output cardinality*') {
        throw 'Modeled prestage capture accepted duplicate helper output.'
    }
    Assert-ModeledPrestageSuccess `
        -Model $duplicateOutputModel -Label 'Modeled duplicate-output rejection'
} finally {
    if ($null -ne $identityLock) {
        $identityLock.Dispose()
    }
    foreach ($path in @(
        $captureEvidencePath,
        $observationEvidencePath,
        $lockedIdentityPath,
        $relativeEvidenceFullPath,
        $linkedEvidenceTargetPath
    )) {
        if ([IO.File]::Exists($path)) {
            [IO.File]::Delete($path)
        }
    }
    if ($linkCreated -and (Test-Path -LiteralPath $linkedParentPath)) {
        Remove-Item -LiteralPath $linkedParentPath -Force
    }
    if ([IO.Directory]::Exists($realParentPath)) {
        [IO.Directory]::Delete($realParentPath)
    }
    if ([IO.Directory]::Exists($evidenceRoot)) {
        [IO.Directory]::Delete($evidenceRoot)
    }
    if ([IO.Directory]::Exists($prestageFixtureRoot)) {
        $canonicalFixtureRoot = [IO.Path]::GetFullPath($prestageFixtureRoot)
        $canonicalTempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
        if (-not $canonicalFixtureRoot.StartsWith(
                $canonicalTempRoot,
                [StringComparison]::OrdinalIgnoreCase
            ) -or
            [IO.Path]::GetFileName($canonicalFixtureRoot) -cnotmatch
                '^clearra-oracle-prestage-transport-[0-9a-f]{32}$') {
            throw 'Prestage fixture cleanup target escaped its exact temporary namespace.'
        }
        Remove-Item -LiteralPath $canonicalFixtureRoot -Recurse -Force
    }
    Remove-Variable -Name clearraOracleTestMockOutput -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable -Name clearraOracleTestMockSshInvocationCount -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable -Name clearraOracleTestLockedIdentityPath -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable -Name clearraOraclePrestageRemote -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable -Name clearraOracleTestExpectedKnownHostsPath -Scope Global -ErrorAction SilentlyContinue
}

'oracle_release_deploy_wrapper_test=pass'
