$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$source = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'invoke-discord-runtime-recovery-v080.ps1') -Raw
foreach ($functionName in @(
    'Invoke-NodeExact',
    'Get-CloudObjectProperty',
    'Get-CloudOptionalTextProperty',
    'Get-CloudRequiredTextProperty',
    'Get-CloudServiceStatus',
    'Get-CloudTrafficEntries',
    'Get-CloudTrafficPercent',
    'Get-ExactActiveCloudRevision',
    'Get-ValidatedCandidateTagEntryCount',
    'Test-CloudTrafficEntryMatchesCandidate'
)) {
    $pattern = '(?ms)^function ' + [regex]::Escape($functionName) + ' \{.*?^\}'
    $match = [regex]::Match($source, $pattern)
    if (-not $match.Success) { throw "$functionName function was not found" }
    Invoke-Expression $match.Value
}

function global:node {
    Write-Output 'validator_status=passed'
    $global:LASTEXITCODE = 0
}
$value = Invoke-NodeExact fixture.mjs verify
if ($null -ne $value) { throw 'validator stdout escaped into the PowerShell return pipeline' }

function global:node {
    Write-Output 'validator_status=failed'
    $global:LASTEXITCODE = 9
}
$threw = $false
try { Invoke-NodeExact fixture.mjs verify } catch { $threw = $_.Exception.Message -ceq 'tracked recovery validator failed' }
if (-not $threw) { throw 'nonzero validator status did not fail closed' }

$mixedTraffic = @'
{
  "status": {
    "latestCreatedRevisionName": "candidate-00002",
    "traffic": [
      { "latestRevision": true, "revisionName": "prior-00001", "percent": 100 },
      { "revisionName": "candidate-00002", "tag": "candidate-tag" }
    ]
  }
}
'@ | ConvertFrom-Json
if ((Get-ExactActiveCloudRevision -Service $mixedTraffic) -cne 'prior-00001') {
    throw 'tag-only traffic changed the exact active revision'
}
$mixedEntries = @(Get-CloudTrafficEntries -Service $mixedTraffic)
if ((Get-CloudTrafficPercent -Entry $mixedEntries[1]) -ne 0) {
    throw 'tag-only traffic without percent was not normalized to zero'
}
if ((Get-ValidatedCandidateTagEntryCount -Traffic $mixedEntries `
        -CandidateTag 'candidate-tag' -CandidateRevision 'candidate-00002') -ne 1) {
    throw 'exact tag-only candidate traffic was not recognized'
}
if (-not (Test-CloudTrafficEntryMatchesCandidate -Entry $mixedEntries[1] `
        -CandidateTag 'candidate-tag' -CandidateRevision 'candidate-00002')) {
    throw 'candidate traffic matcher lost the tag-only entry'
}
if (Test-CloudTrafficEntryMatchesCandidate -Entry $mixedEntries[0] `
        -CandidateTag 'candidate-tag' -CandidateRevision 'candidate-00002') {
    throw 'active traffic without tag was misclassified as candidate residue'
}

$failureFixtures = @(
    @{
        Json = '{"status":{"traffic":[{"latestRevision":true,"percent":100}]}}'
        Message = 'Cloud active traffic revisionName is unavailable'
    },
    @{
        Json = '{"status":{"traffic":[{"revisionName":"prior","percent":"invalid"}]}}'
        Message = 'Cloud traffic percent is invalid'
    },
    @{
        Json = '{"status":{"traffic":[{"revisionName":"a","percent":50},{"revisionName":"b","percent":50}]}}'
        Message = 'Cloud traffic is not one exact 100-percent revision'
    },
    @{
        Json = '{"status":{}}'
        Message = 'Cloud service traffic readback is unavailable'
    }
)
foreach ($fixture in $failureFixtures) {
    $fixtureThrew = $false
    try {
        [void](Get-ExactActiveCloudRevision -Service ($fixture.Json | ConvertFrom-Json))
    } catch {
        $fixtureThrew = $_.Exception.Message -ceq $fixture.Message
    }
    if (-not $fixtureThrew) { throw "traffic fixture did not fail closed: $($fixture.Message)" }
}

$wrongTagRevisionThrew = $false
try {
    [void](Get-ValidatedCandidateTagEntryCount -Traffic $mixedEntries `
        -CandidateTag 'candidate-tag' -CandidateRevision 'different-revision')
} catch {
    $wrongTagRevisionThrew = $_.Exception.Message -ceq 'Cloud candidate tag differs from the sealed candidate residue'
}
if (-not $wrongTagRevisionThrew) { throw 'candidate tag revision mismatch did not fail closed' }

$missingTagRevision = '{"tag":"candidate-tag"}' | ConvertFrom-Json
$missingTagRevisionThrew = $false
try {
    [void](Get-ValidatedCandidateTagEntryCount -Traffic @($missingTagRevision) `
        -CandidateTag 'candidate-tag' -CandidateRevision 'candidate-00002')
} catch {
    $missingTagRevisionThrew = $_.Exception.Message -ceq 'Cloud candidate tag revisionName is unavailable'
}
if (-not $missingTagRevisionThrew) { throw 'candidate tag without revisionName did not fail closed' }

Write-Output 'discord_runtime_recovery_traffic_shape=passed'
Write-Output 'discord_runtime_recovery_invoke_node_exact=passed'
