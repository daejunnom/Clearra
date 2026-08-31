[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$wrapper = Join-Path $PSScriptRoot 'invoke-release-deploy-v080.ps1'
$sourceCommit = '0123456789abcdef0123456789abcdef01234567'
$scriptReleaseId = 'v0.8.0-0123456'
$scriptReleaseSha256 = 'b' * 64
$deploymentNonce = 'a' * 64
$candidateRevision = 'clearra-current-job-v080-0123456'
$candidateUrl = 'https://candidate.example.test'
$verifiedAfter = '2026-08-30T00:00:00.000Z'
$dummyIdentity = Join-Path $PSScriptRoot 'identity-must-not-be-read'

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

$prestageCapture = @(& $wrapper `
    -Operation capture-prestage-authority `
    -PriorRevision 'clearra-current-job-v075-042ec21' `
    -PriorRuntimeAuthorityKind 'clearra.rollback.legacy-health-no-runtime.v1' `
    -DeploymentNonce $deploymentNonce `
    -AuditOnly)
Assert-AuditResult -Output $prestageCapture -Operation 'capture-prestage-authority'

$prestageCleanup = @(& $wrapper `
    -Operation cleanup-prestage-backup `
    -DeploymentNonce $deploymentNonce `
    -AuditOnly)
Assert-AuditResult -Output $prestageCleanup -Operation 'cleanup-prestage-backup'

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
    "if (`$Operation -notin @('capture-prestage-authority', 'capture-rollback-authority', 'observe-candidate', 'classify-current-authority')) {"
)) {
    if ($source.IndexOf($required, [StringComparison]::Ordinal) -lt 0) {
        throw "Typed invoker is missing pinned identity/SSH marker: $required"
    }
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
$identityLock = $null
$linkCreated = $false
$global:clearraOracleTestMockOutput = $null
$global:clearraOracleTestMockSshInvocationCount = 0
$global:clearraOracleTestLockedIdentityPath = $lockedIdentityPath

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

function ssh {
    $sshConfigArgumentIndex = [Array]::IndexOf([object[]]$args, '-F')
    $expectedSshConfigPath = if (
        [Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
            [Runtime.InteropServices.OSPlatform]::Windows
        )
    ) { 'NUL' } else { '/dev/null' }
    if ($sshConfigArgumentIndex -lt 0 -or
        $sshConfigArgumentIndex + 1 -ge $args.Count -or
        [string]$args[$sshConfigArgumentIndex + 1] -cne $expectedSshConfigPath) {
        throw 'Oracle wrapper did not assemble the host-specific SSH config path.'
    }
    $identityArgumentIndex = [Array]::IndexOf([object[]]$args, '-i')
    if ($identityArgumentIndex -lt 0 -or
        $identityArgumentIndex + 1 -ge $args.Count -or
        [string]$args[$identityArgumentIndex + 1] -cne $global:clearraOracleTestLockedIdentityPath) {
        throw 'Oracle wrapper did not pass the locked identity path only as an SSH argument.'
    }
    $global:clearraOracleTestMockSshInvocationCount += 1
    $global:LASTEXITCODE = 0
    $global:clearraOracleTestMockOutput
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
    Remove-Variable -Name clearraOracleTestMockOutput -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable -Name clearraOracleTestMockSshInvocationCount -Scope Global -ErrorAction SilentlyContinue
    Remove-Variable -Name clearraOracleTestLockedIdentityPath -Scope Global -ErrorAction SilentlyContinue
}

'oracle_release_deploy_wrapper_test=pass'
