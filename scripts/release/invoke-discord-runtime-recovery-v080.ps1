[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('prestage', 'live')]
    [string] $Stage,

    [Parameter(Mandatory = $true)][string] $ArtifactRoot,
    [Parameter(Mandatory = $true)][string] $EvidenceRoot,
    [Parameter(Mandatory = $true)][string] $SourceCommit,
    [Parameter(Mandatory = $true)][string] $TrustedHelperSourceCommit,
    [Parameter(Mandatory = $true)][string] $OriginalWorkflowRunId,
    [Parameter(Mandatory = $true)][string] $OriginalWorkflowRunAttempt,
    [Parameter(Mandatory = $true)][string] $RecoveryArtifactId,
    [Parameter(Mandatory = $true)][string] $RecoveryArtifactDigest,
    [Parameter(Mandatory = $true)][string] $RecoveryAuthorityPath,
    [string] $CatalogDispositionPath,
    [string] $CatalogRecoveryRequired,
    [string] $CatalogArtifactId,
    [string] $CatalogArtifactDigest,
    [Parameter(Mandatory = $true)][string] $Repository,
    [Parameter(Mandatory = $true)][string] $GcpProjectId,
    [Parameter(Mandatory = $true)][string] $GcpRegion,
    [Parameter(Mandatory = $true)][string] $IdentityFile,
    [string] $RemoteOverlayArchive,
    [string] $RemoteOverlaySha256,
    [switch] $RestoreOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Get-ExactLeaf {
    param([string] $Path, [string] $Label)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "$Label is unavailable" }
    $item = Get-Item -LiteralPath $Path -Force
    if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw "$Label must not be a reparse point"
    }
    return $item
}

function Invoke-NodeExact {
    param([Parameter(ValueFromRemainingArguments = $true)][object[]] $Arguments)
    & node @Arguments
    if ($LASTEXITCODE -ne 0) { throw 'tracked recovery validator failed' }
}

function Get-ActiveCloudRevision {
    param([string] $OutputPath)
    gcloud run services describe clearra-current-job `
        --project=$GcpProjectId --region=$GcpRegion --format=json |
        Set-Content -LiteralPath $OutputPath -Encoding utf8NoBOM
    if ($LASTEXITCODE -ne 0) { throw 'Cloud service authority readback failed' }
    $service = Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json
    $active = @($service.status.traffic | Where-Object { [int]$_.percent -gt 0 })
    if ($active.Count -ne 1 -or [int]$active[0].percent -ne 100) {
        throw 'Cloud traffic is not one exact 100-percent revision'
    }
    return [string]$active[0].revisionName
}

function Seal-ExactCandidateCloudResidue {
    param(
        $Intent,
        [string] $PriorRevision,
        [string] $ServiceOutputPath,
        [string] $RevisionOutputPath
    )
    $revisionJson = & gcloud run revisions list `
        --project=$GcpProjectId --region=$GcpRegion `
        --filter="metadata.name=$($Intent.cloud_candidate_revision)" --format=json
    if ($LASTEXITCODE -ne 0) { throw 'Cloud candidate residue inventory failed' }
    $revisions = @($revisionJson | ConvertFrom-Json)
    if ($revisions.Count -gt 1) { throw 'Cloud candidate residue inventory is ambiguous' }
    if ($revisions.Count -eq 1) {
        $revision = $revisions[0]
        if ([string]$revision.metadata.name -cne [string]$Intent.cloud_candidate_revision -or
            @($revision.spec.containers).Count -ne 1 -or
            [string]$revision.spec.containers[0].image -cne [string]$Intent.cloud_image_digest) {
            throw 'Cloud candidate residue differs from prestage intent'
        }
    }

    [void](Get-ActiveCloudRevision -OutputPath $ServiceOutputPath)
    $serviceBefore = Get-Content -LiteralPath $ServiceOutputPath -Raw | ConvertFrom-Json
    $candidateTagEntries = @($serviceBefore.status.traffic | Where-Object {
        $_.tag -ceq [string]$Intent.cloud_candidate_tag
    })
    if ($candidateTagEntries.Count -gt 1 -or
        ($candidateTagEntries.Count -eq 1 -and
         [string]$candidateTagEntries[0].revisionName -cne [string]$Intent.cloud_candidate_revision)) {
        throw 'Cloud candidate tag differs from the sealed candidate residue'
    }
    if ($candidateTagEntries.Count -eq 1) {
        $candidateTag = [string]$Intent.cloud_candidate_tag
        gcloud run services update-traffic clearra-current-job `
            --project=$GcpProjectId --region=$GcpRegion `
            "--remove-tags=$candidateTag" --quiet
        if ($LASTEXITCODE -ne 0) { throw 'Cloud candidate residue tag removal failed' }
    }

    if ((Get-ActiveCloudRevision -OutputPath $ServiceOutputPath) -cne $PriorRevision) {
        throw 'Cloud residue readback differs from the exact prior authority'
    }
    $service = Get-Content -LiteralPath $ServiceOutputPath -Raw | ConvertFrom-Json
    if (@($service.status.traffic | Where-Object {
        $_.revisionName -ceq [string]$Intent.cloud_candidate_revision -or
        $_.tag -ceq [string]$Intent.cloud_candidate_tag
    }).Count -ne 0) {
        throw 'Cloud residue readback retains candidate traffic or a direct-routing tag'
    }

    $revisionReadbackJson = & gcloud run revisions list `
        --project=$GcpProjectId --region=$GcpRegion `
        --filter="metadata.name=$($Intent.cloud_candidate_revision)" --format=json
    if ($LASTEXITCODE -ne 0) { throw 'Cloud candidate residue readback failed' }
    $revisionReadback = @($revisionReadbackJson | ConvertFrom-Json)
    if ($revisionReadback.Count -ne $revisions.Count) {
        throw 'Cloud candidate residue changed during guarded recovery'
    }
    $latestCreated = [string]$service.status.latestCreatedRevisionName
    $disposition = 'exact-candidate-absent'
    if ($revisionReadback.Count -eq 1) {
        $revision = $revisionReadback[0]
        if ([string]$revision.metadata.name -cne [string]$Intent.cloud_candidate_revision -or
            @($revision.spec.containers).Count -ne 1 -or
            [string]$revision.spec.containers[0].image -cne [string]$Intent.cloud_image_digest -or
            $latestCreated -cne [string]$Intent.cloud_candidate_revision) {
            throw 'Cloud candidate residue is not the exact immutable latest revision'
        }
        $disposition = 'preserved-latest-zero-traffic-tagless'
    } elseif ($latestCreated -ceq [string]$Intent.cloud_candidate_revision) {
        throw 'Cloud latest-created revision is absent from the exact residue inventory'
    }
    [ordered]@{
        schema_id = 'clearra.cloud-candidate-residue-readback.v1'
        disposition = $disposition
        prior_revision = $PriorRevision
        active_revision = $PriorRevision
        active_percent = 100
        candidate_revision = [string]$Intent.cloud_candidate_revision
        candidate_image_digest = [string]$Intent.cloud_image_digest
        candidate_revision_present = ($revisionReadback.Count -eq 1)
        candidate_traffic_percent = 0
        candidate_tag = [string]$Intent.cloud_candidate_tag
        candidate_tag_present = $false
        latest_created_revision = $latestCreated
        deletion_deferred_until_superseded = ($revisionReadback.Count -eq 1)
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $RevisionOutputPath -Encoding utf8NoBOM
}

function Verify-PrestageAuthority {
    $statePath = Join-Path $ArtifactRoot 'prestage/prestage-state.json'
    $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
    $nonce = [string]$state.deployment_nonce
    Invoke-NodeExact scripts/release/discord-deployment-state.mjs verify `
        --stage prestage --source-commit $SourceCommit `
        --workflow-run-id $OriginalWorkflowRunId `
        --workflow-run-attempt $OriginalWorkflowRunAttempt `
        --accepted-run-id ([string]$state.accepted_run_id) `
        --accepted-run-attempt ([string]$state.accepted_run_attempt) `
        --deployment-nonce $nonce --report $statePath `
        --binding "cloud_prior_authority=$ArtifactRoot/prestage/cloud-prior-authority.json" `
        --binding "intended_candidate_authority=$ArtifactRoot/prestage/intended-candidate-authority.json" `
        --binding "oracle_rollback_capture=$ArtifactRoot/prestage/oracle-rollback-capture.json" `
        --binding "prepared_state=$ArtifactRoot/prepared/prepared-state.json"
    Invoke-NodeExact scripts/release/discord-deployment-recovery.mjs verify-intent `
        --source-commit $SourceCommit `
        --workflow-run-id $OriginalWorkflowRunId `
        --workflow-run-attempt $OriginalWorkflowRunAttempt `
        --deployment-nonce $nonce `
        --report "$ArtifactRoot/prestage/intended-candidate-authority.json"
    return $state
}

function Verify-LiveAuthority {
    param($PrestageState)
    $statePath = Join-Path $ArtifactRoot 'candidate/candidate-state.json'
    $state = Get-Content -LiteralPath $statePath -Raw | ConvertFrom-Json
    if ([string]$state.deployment_nonce -cne [string]$PrestageState.deployment_nonce) {
        throw 'candidate and prestage nonces differ'
    }
    Invoke-NodeExact scripts/release/discord-deployment-state.mjs verify `
        --stage candidate --source-commit $SourceCommit `
        --workflow-run-id $OriginalWorkflowRunId `
        --workflow-run-attempt $OriginalWorkflowRunAttempt `
        --accepted-run-id ([string]$state.accepted_run_id) `
        --accepted-run-attempt ([string]$state.accepted_run_attempt) `
        --deployment-nonce ([string]$state.deployment_nonce) --report $statePath `
        --binding "cloud_candidate_authority=$ArtifactRoot/candidate/cloud-candidate-authority.json" `
        --binding "cloud_candidate_smoke=$ArtifactRoot/candidate/cloud-candidate-smoke.json" `
        --binding "oracle_stage_manifest=$ArtifactRoot/candidate/oracle-inactive-stage-v080.json" `
        --binding "prestage_state=$ArtifactRoot/prestage/prestage-state.json"
    return $state
}

function Invoke-OracleClassification {
    param([string] $OutputPath, $Intent, $Candidate, $Prior, $Manifest, $Rollback, [string] $Nonce)
    $settingsSha = (& node scripts/release/oracle/candidate-settings-v080.mjs `
        --source-commit $SourceCommit --candidate-url ([string]$Candidate.candidateUrl) --hash-only).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'candidate settings authority derivation failed' }
    & scripts/release/oracle/invoke-release-deploy-v080.ps1 `
        -Operation classify-current-authority `
        -ScriptReleaseId ([string]$Manifest.releaseId) `
        -ScriptReleaseSha256 ([string]$Manifest.candidate.treeSha256) `
        -SourceCommit $SourceCommit -CandidateUrl ([string]$Candidate.candidateUrl) `
        -CandidateRevision ([string]$Candidate.candidateRevision) `
        -OracleReleaseId ([string]$Manifest.releaseId) `
        -OracleReleaseSha256 ([string]$Manifest.candidate.treeSha256) `
        -OracleSettingsSha256 $settingsSha `
        -PriorRelease ([string]$Rollback.priorOracleRelease) `
        -PriorReleaseId ([string]$Rollback.priorOracleReleaseId) `
        -PriorReleaseSha256 ([string]$Rollback.priorOracleReleaseSha256) `
        -PriorSettingsSha256 ([string]$Rollback.priorOracleSettingsSha256) `
        -PriorRuntimeAuthorityKind ([string]$Rollback.priorRuntimeAuthorityKind) `
        -PriorRuntimeAuthoritySha256 ([string]$Rollback.priorRuntimeAuthoritySha256) `
        -PriorJobUrl ([string]$Rollback.priorJobUrl) `
        -PriorRevision ([string]$Prior.prior_revision) `
        -DeploymentNonce $Nonce -EvidenceOutput $OutputPath -IdentityFile $IdentityFile | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Oracle current authority classification failed' }
    return Get-Content -LiteralPath $OutputPath -Raw | ConvertFrom-Json
}

[void](Get-ExactLeaf -Path $IdentityFile -Label 'Oracle identity')
if ($SourceCommit -cnotmatch '^[0-9a-f]{40}$' -or
    $TrustedHelperSourceCommit -cnotmatch '^[0-9a-f]{40}$' -or
    $OriginalWorkflowRunId -cnotmatch '^[1-9][0-9]*$' -or
    $OriginalWorkflowRunAttempt -cnotmatch '^[1-9][0-9]*$' -or
    $RecoveryArtifactId -cnotmatch '^[1-9][0-9]*$' -or
    $RecoveryArtifactDigest -cnotmatch '^sha256:[0-9a-f]{64}$' -or
    $Repository -cnotmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') {
    throw 'runtime recovery identity is invalid'
}
if (-not (Test-Path -LiteralPath $EvidenceRoot -PathType Container)) {
    [void](New-Item -ItemType Directory -Path $EvidenceRoot)
}
$resultPath = Join-Path $EvidenceRoot 'recovery-result.json'
if (Test-Path -LiteralPath $resultPath -PathType Leaf) {
    Invoke-NodeExact scripts/release/discord-deployment-recovery.mjs verify-result `
        --stage $Stage --repository $Repository --source-commit $SourceCommit `
        --original-workflow-run-id $OriginalWorkflowRunId `
        --original-workflow-run-attempt $OriginalWorkflowRunAttempt `
        --recovery-workflow-run-id $env:GITHUB_RUN_ID `
        --recovery-workflow-run-attempt $env:GITHUB_RUN_ATTEMPT `
        --artifact-id $RecoveryArtifactId --artifact-digest $RecoveryArtifactDigest `
        --catalog-disposition $CatalogDispositionPath `
        --catalog-recovery-required $CatalogRecoveryRequired `
        --catalog-artifact-id $(if ($CatalogArtifactId) { $CatalogArtifactId } else { 'none' }) `
        --catalog-artifact-digest $(if ($CatalogArtifactDigest) { $CatalogArtifactDigest } else { 'none' }) `
        --report $resultPath
    Write-Output 'discord_runtime_recovery=already-complete'
    return
}
[void](Get-ExactLeaf -Path $RecoveryAuthorityPath -Label 'Recovery resolution authority')
if (-not $RestoreOnly) {
    [void](Get-ExactLeaf -Path $CatalogDispositionPath -Label 'Discord catalog recovery disposition')
    if ($CatalogRecoveryRequired -cnotin @('true', 'false')) {
        throw 'Discord catalog recovery requirement is invalid'
    }
}

$prestageState = Verify-PrestageAuthority
$nonce = [string]$prestageState.deployment_nonce
$intent = Get-Content "$ArtifactRoot/prestage/intended-candidate-authority.json" -Raw | ConvertFrom-Json
$prior = Get-Content "$ArtifactRoot/prestage/cloud-prior-authority.json" -Raw | ConvertFrom-Json
$rollback = Get-Content "$ArtifactRoot/prestage/oracle-rollback-capture.json" -Raw | ConvertFrom-Json

if ($Stage -ceq 'prestage') {
    if ($RemoteOverlaySha256 -cnotmatch '^[0-9a-f]{64}$' -or
        $RemoteOverlayArchive -cne "/opt/clearra/sealed-release-inputs/private-overlay-no-config-$RemoteOverlaySha256.tar" -or
        $RemoteOverlayArchive -cne [string]$intent.remote_overlay_archive -or
        $RemoteOverlaySha256 -cne [string]$intent.remote_overlay_sha256) {
        throw 'prestage cleanup remote overlay authority differs'
    }
    $cloudBefore = Join-Path $EvidenceRoot "cloud-prestage-before-$([Guid]::NewGuid().ToString('N')).json"
    if ((Get-ActiveCloudRevision -OutputPath $cloudBefore) -cne [string]$prior.prior_revision) {
        throw 'prestage cleanup refuses Cloud traffic outside exact prior authority'
    }
    $manifestPath = Join-Path $EvidenceRoot "oracle-cleanup-manifest-$([Guid]::NewGuid().ToString('N')).json"
    $source = Join-Path $ArtifactRoot 'prepared/exact-source.tar.gz'
    $ctk = Join-Path $ArtifactRoot 'prepared/oracle-layers/ctk3-dist.tar'
    $deps = Join-Path $ArtifactRoot 'prepared/oracle-layers/node_modules.tar'
    & scripts/release/oracle/invoke-freeze-v080.ps1 `
        -SourceCommit $SourceCommit -SourceArchive $source `
        -RemoteOverlayArchive $RemoteOverlayArchive -RemoteOverlaySha256 $RemoteOverlaySha256 `
        -Ctk3DistArchive $ctk -DependenciesArchive $deps `
        -ManifestOutput $manifestPath -IdentityFile $IdentityFile
    if ($LASTEXITCODE -ne 0) { throw 'prestage cleanup freeze authority failed' }
    $oracleInactiveCleanup = Join-Path $EvidenceRoot 'oracle-inactive-cleanup.txt'
    & scripts/release/oracle/invoke-inactive-stage-v080.ps1 `
        -ManifestPath $manifestPath -SourceArchive $source `
        -RemoteOverlayArchive $RemoteOverlayArchive -RemoteOverlaySha256 $RemoteOverlaySha256 `
        -Ctk3DistArchive $ctk -DependenciesArchive $deps `
        -CleanupOnly -IdentityFile $IdentityFile | Set-Content -LiteralPath $oracleInactiveCleanup -Encoding utf8NoBOM
    if ($LASTEXITCODE -ne 0) { throw 'bounded Oracle inactive candidate cleanup failed' }
    $cloudAfter = Join-Path $EvidenceRoot 'cloud-prestage-cleanup-readback.json'
    $cloudRevisionAfter = Join-Path $EvidenceRoot 'cloud-prestage-revision-cleanup-readback.json'
    Seal-ExactCandidateCloudResidue -Intent $intent `
        -PriorRevision ([string]$prior.prior_revision) `
        -ServiceOutputPath $cloudAfter -RevisionOutputPath $cloudRevisionAfter
    $oracleBackupCleanup = Join-Path $EvidenceRoot 'oracle-backup-cleanup.json'
    & scripts/release/oracle/invoke-release-deploy-v080.ps1 `
        -Operation cleanup-prestage-backup -SourceCommit $TrustedHelperSourceCommit `
        -DeploymentNonce $nonce `
        -IdentityFile $IdentityFile | Set-Content -LiteralPath $oracleBackupCleanup -Encoding utf8NoBOM
    if ($LASTEXITCODE -ne 0) { throw 'bounded Oracle rollback backup cleanup failed' }
    if ($RestoreOnly) {
        Write-Output 'discord_runtime_recovery=prestage-cleanup-restored-only'
        return
    }
    Invoke-NodeExact scripts/release/discord-deployment-recovery.mjs seal-result `
        --stage prestage --repository $Repository --source-commit $SourceCommit `
        --original-workflow-run-id $OriginalWorkflowRunId `
        --original-workflow-run-attempt $OriginalWorkflowRunAttempt `
        --recovery-workflow-run-id $env:GITHUB_RUN_ID `
        --recovery-workflow-run-attempt $env:GITHUB_RUN_ATTEMPT `
        --artifact-id $RecoveryArtifactId --artifact-digest $RecoveryArtifactDigest `
        --catalog-disposition $CatalogDispositionPath `
        --catalog-recovery-required $CatalogRecoveryRequired `
        --catalog-artifact-id $(if ($CatalogArtifactId) { $CatalogArtifactId } else { 'none' }) `
        --catalog-artifact-digest $(if ($CatalogArtifactDigest) { $CatalogArtifactDigest } else { 'none' }) `
        --recovered-at ([DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ss.fffZ')) `
        --binding "cloud_cleanup_readback=$cloudAfter" `
        --binding "cloud_pre_mutation_readback=$cloudBefore" `
        --binding "cloud_candidate_residue_readback=$cloudRevisionAfter" `
        --binding "intended_candidate_authority=$ArtifactRoot/prestage/intended-candidate-authority.json" `
        --binding "oracle_backup_cleanup=$oracleBackupCleanup" `
        --binding "oracle_inactive_cleanup=$oracleInactiveCleanup" `
        --binding "oracle_rollback_capture=$ArtifactRoot/prestage/oracle-rollback-capture.json" `
        --binding "prestage_state=$ArtifactRoot/prestage/prestage-state.json" `
        --binding "recovery_authority=$RecoveryAuthorityPath" `
        --output $resultPath
    Write-Output 'discord_runtime_recovery=prestage-cleanup-complete'
    return
}

$candidateState = Verify-LiveAuthority -PrestageState $prestageState
$candidate = Get-Content "$ArtifactRoot/candidate/cloud-candidate-authority.json" -Raw | ConvertFrom-Json
$manifest = Get-Content "$ArtifactRoot/candidate/oracle-inactive-stage-v080.json" -Raw | ConvertFrom-Json
$guardId = [Guid]::NewGuid().ToString('N')
$cloudBefore = Join-Path $EvidenceRoot "cloud-current-$guardId.json"
$cloudStateRevision = Get-ActiveCloudRevision -OutputPath $cloudBefore
$cloudState = if ($cloudStateRevision -ceq [string]$prior.prior_revision) {
    'prior'
} elseif ($cloudStateRevision -ceq [string]$candidate.candidateRevision) {
    'candidate'
} else {
    throw 'Cloud current state is neither exact prior nor exact candidate'
}
$oracleBeforePath = Join-Path $EvidenceRoot "oracle-current-$guardId.json"
$oracleState = Invoke-OracleClassification `
    -OutputPath $oracleBeforePath -Intent $intent -Candidate $candidate -Prior $prior `
    -Manifest $manifest -Rollback $rollback -Nonce $nonce
if ($oracleState.state -cnotin @('prior', 'candidate')) {
    throw 'Oracle current state is neither exact prior nor exact candidate'
}

if ($oracleState.state -ceq 'candidate') {
    $after = [DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ss.fffZ')
    & scripts/release/oracle/invoke-release-deploy-v080.ps1 `
        -Operation restore-prior-and-verify -ScriptReleaseId ([string]$manifest.releaseId) `
        -ScriptReleaseSha256 ([string]$manifest.candidate.treeSha256) `
        -PriorRelease ([string]$rollback.priorOracleRelease) `
        -PriorReleaseId ([string]$rollback.priorOracleReleaseId) `
        -PriorReleaseSha256 ([string]$rollback.priorOracleReleaseSha256) `
        -PriorSettingsBackup ([string]$rollback.priorOracleSettingsBackup) `
        -PriorSettingsSha256 ([string]$rollback.priorOracleSettingsSha256) `
        -PriorRuntimeAuthorityKind ([string]$rollback.priorRuntimeAuthorityKind) `
        -PriorRuntimeAuthoritySha256 ([string]$rollback.priorRuntimeAuthoritySha256) `
        -PriorJobUrl ([string]$rollback.priorJobUrl) `
        -PriorRevision ([string]$prior.prior_revision) `
        -Proof "/run/clearra-deploy/clearra-oracle-rollback-$nonce.json" `
        -DeploymentNonce $nonce -VerifiedAfter $after -IdentityFile $IdentityFile | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'exact prior Oracle recovery failed' }
}
$oracleAfterPath = Join-Path $EvidenceRoot "oracle-restore-attestation-$guardId.json"
$oracleAfter = Invoke-OracleClassification `
    -OutputPath $oracleAfterPath -Intent $intent -Candidate $candidate -Prior $prior `
    -Manifest $manifest -Rollback $rollback -Nonce $nonce
if ($oracleAfter.state -cne 'prior') { throw 'Oracle recovery readback is not exact prior' }

if ($cloudState -ceq 'candidate') {
    gcloud run services update-traffic clearra-current-job `
        --project=$GcpProjectId --region=$GcpRegion `
        --to-revisions="$($prior.prior_revision)=100" --quiet
    if ($LASTEXITCODE -ne 0) { throw 'exact prior Cloud recovery failed' }
}
$cloudAfterPath = Join-Path $EvidenceRoot "cloud-restore-readback-$guardId.json"
$cloudRevisionAfterPath = Join-Path $EvidenceRoot "cloud-revision-cleanup-readback-$guardId.json"
Seal-ExactCandidateCloudResidue -Intent $intent `
    -PriorRevision ([string]$prior.prior_revision) `
    -ServiceOutputPath $cloudAfterPath -RevisionOutputPath $cloudRevisionAfterPath

if ($RestoreOnly) {
    Write-Output 'discord_runtime_recovery=live-restored-only'
    return
}
Invoke-NodeExact scripts/release/discord-deployment-recovery.mjs seal-result `
    --stage live `
    --repository $Repository --source-commit $SourceCommit `
    --original-workflow-run-id $OriginalWorkflowRunId `
    --original-workflow-run-attempt $OriginalWorkflowRunAttempt `
    --recovery-workflow-run-id $env:GITHUB_RUN_ID `
    --recovery-workflow-run-attempt $env:GITHUB_RUN_ATTEMPT `
    --artifact-id $RecoveryArtifactId --artifact-digest $RecoveryArtifactDigest `
    --catalog-disposition $CatalogDispositionPath `
    --catalog-recovery-required $CatalogRecoveryRequired `
    --catalog-artifact-id $(if ($CatalogArtifactId) { $CatalogArtifactId } else { 'none' }) `
    --catalog-artifact-digest $(if ($CatalogArtifactDigest) { $CatalogArtifactDigest } else { 'none' }) `
    --recovered-at ([DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ss.fffZ')) `
    --binding "candidate_state=$ArtifactRoot/candidate/candidate-state.json" `
    --binding "cloud_pre_mutation_classification=$cloudBefore" `
    --binding "cloud_prior_authority=$ArtifactRoot/prestage/cloud-prior-authority.json" `
    --binding "cloud_restore_readback=$cloudAfterPath" `
    --binding "cloud_candidate_residue_readback=$cloudRevisionAfterPath" `
    --binding "oracle_pre_mutation_classification=$oracleBeforePath" `
    --binding "oracle_restore_attestation=$oracleAfterPath" `
    --binding "oracle_rollback_capture=$ArtifactRoot/prestage/oracle-rollback-capture.json" `
    --binding "oracle_stage_manifest=$ArtifactRoot/candidate/oracle-inactive-stage-v080.json" `
    --binding "prestage_state=$ArtifactRoot/prestage/prestage-state.json" `
    --binding "recovery_authority=$RecoveryAuthorityPath" `
    --output $resultPath
Write-Output 'discord_runtime_recovery=live-restore-complete'
