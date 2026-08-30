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
    "'ClearAllForwardings=yes'"
)) {
    if ($source.IndexOf($required, [StringComparison]::Ordinal) -lt 0) {
        throw "Typed invoker is missing pinned identity/SSH marker: $required"
    }
}
if ($source -match '(?i)(Get-Content|Get-FileHash|ReadAllBytes|ReadAllText)[^\r\n]*\$IdentityFile') {
    throw 'Typed invoker reads or hashes the identity file.'
}

'oracle_release_deploy_wrapper_test=pass'
