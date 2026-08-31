# Active Clearra Cloud Run job service

`clearra-current-job` is the active heavy-compute tier for Discord commands.
Discord does not call it. The Oracle Gateway owns slash ACKs, Modals, interaction
tokens, message edits, `$`/`>` ingress, and bounded GIF rendering; Oracle calls
this service only through the authenticated `clearra.job.v1` seam.

```text
Discord -> Oracle Gateway -> POST clearra-current-job /jobs -> Clearra CLI
                    ^                                      |
                    +----------- bounded result -----------+
```

The job request contains curated Clearra arguments, an idempotency key, and an
absolute deadline. It contains no Discord bot token, interaction token, channel
credential, or Discord webhook URL. The job service never calls Discord.

## Artifact boundary

Two job-service artifacts intentionally remain separate:

- `Dockerfile.current-job-service` and
  `cloudbuild-current-job-service.yaml` build the Clearra CLI from the current
  approved source commit. `clearra-current-job` must use this image so
  post-v0.5.1 fixes, current command syntax, CTK3 output, and worker policy
  remain aligned with Oracle.
- `Dockerfile.job-service` and `cloudbuild-job-service.yaml` package the
  released v0.5.1 Linux CLI. That immutable artifact is compatibility-test only
  and must not receive active Oracle traffic. Its build now intentionally fails
  the required finesse capability gate, so it cannot become a healthy service
  revision by mistake. The downloaded binary requires an exact SHA-256 and is
  permanently labeled `clearra.search.contract.legacy-v1`; it cannot claim the
  current v2 search contract.

The retired Discord interaction image is not a job-service artifact and must not
receive active traffic. Discord interactions are owned by Oracle Gateway.

## One-time Policy Troubleshooter prerequisite

Policy Troubleshooter is a separately approved prerequisite; the helper never enables
an API. Enable it before the accepted-source deployment window, then
leave the helper to prove that it is enabled:

The operator performing this one-time enable must have
`serviceusage.services.enable`, normally through Service Usage Admin
(`roles/serviceusage.serviceUsageAdmin`). That
broader role is not a bootstrap-helper requirement: the deployment caller keeps
Service Usage Consumer, and the helper fails closed rather than enabling the API.

```powershell
$projectId = "clearra-cloud"
gcloud services enable policytroubleshooter.googleapis.com --project=$projectId
if ($LASTEXITCODE -ne 0) { throw "Policy Troubleshooter API prerequisite failed" }
```

## Initialize exact-source evidence before mutation

After the one canonical `workflow_dispatch` acceptance succeeds, initialize one
new evidence directory from the clean exact-main checkout. This must finish
before any Pages, Cloud, Oracle, or Discord public mutation. It downloads the
run/attempt-bound canonical acceptance report, makes the source-bound acceptance
stage, and atomically appends that whole stage to the new journal. The directory
and journal are reused by every later section; neither is recreated for a
deployment retry.

```powershell
$sourceCommit = "<full-40-character-accepted-main-commit>"
$repository = "daejunnom/Clearra"
$acceptedRunId = "<canonical-successful-workflow-dispatch-run-id>"
$acceptedRunAttempt = "<exact-positive-run-attempt>"
$attemptId = "<new-release-attempt-id>"
$evidenceDirectory = Join-Path `
  $env:LOCALAPPDATA `
  "Clearra/reports/release-v0.8.0/$attemptId"
$sourceRoot = (Get-Location).Path
$canonicalAcceptanceDirectory = Join-Path $evidenceDirectory "canonical-acceptance-evidence"
$canonicalAcceptanceEvidencePath = Join-Path `
  $canonicalAcceptanceDirectory `
  "clearra-canonical-acceptance-evidence.v1.json"
$canonicalAcceptanceArtifactName = "canonical-acceptance-evidence-$sourceCommit-run-$acceptedRunId-attempt-$acceptedRunAttempt"
$acceptanceStageEvidencePath = Join-Path `
  $evidenceDirectory `
  "final-source-acceptance-stage.json"
$attemptJournal = Join-Path $evidenceDirectory "final-source-attempt.jsonl"

if ($sourceCommit -cnotmatch '^[0-9a-f]{40}$' -or
    $acceptedRunId -cnotmatch '^[1-9][0-9]{0,19}$' -or
    $acceptedRunAttempt -cnotmatch '^[1-9][0-9]{0,19}$' -or
    (Test-Path -LiteralPath $evidenceDirectory)) {
  throw "final-source acceptance authority or new evidence directory is invalid"
}
New-Item -ItemType Directory -Path $canonicalAcceptanceDirectory | Out-Null
gh run download $acceptedRunId `
  --repo $repository `
  --name $canonicalAcceptanceArtifactName `
  --dir $canonicalAcceptanceDirectory
if ($LASTEXITCODE -ne 0) { throw "canonical acceptance evidence download failed" }

node scripts/release/final-source-stage-evidence.mjs acceptance `
  --expected-source-commit $sourceCommit `
  --source-root $sourceRoot `
  --canonical-acceptance-evidence $canonicalAcceptanceEvidencePath `
  --output $acceptanceStageEvidencePath
if ($LASTEXITCODE -ne 0) { throw "acceptance stage evidence failed" }
$acceptanceStageEvidenceFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $acceptanceStageEvidencePath
).Hash.ToLowerInvariant()

node scripts/release/final-source-attempt-journal.mjs initialize `
  --journal $attemptJournal `
  --attempt-id $attemptId `
  --source-commit $sourceCommit
if ($LASTEXITCODE -ne 0) { throw "final-source journal initialization failed" }
node scripts/release/final-source-attempt-journal.mjs append-stage `
  --journal $attemptJournal `
  --stage-evidence $acceptanceStageEvidencePath `
  --stage-evidence-file-sha256 $acceptanceStageEvidenceFileSha256
if ($LASTEXITCODE -ne 0) { throw "acceptance stage journal append failed" }
```

## Build the current-source image

Build in Tokyo and use an immutable source-revision tag. The build context must
be a temporary commit-byte archive of the exact commit that passed canonical
acceptance. The archive helper forces LF export and deterministic modes, rejects
local drift in either helper module, and compares every tar directory, regular
file byte sequence, executable mode, and safe symlink against the accepted Git
tree before producing a `.tar.gz`. This prevents Git attributes or a Windows Git
setting from rewriting, omitting, substituting, or changing the mode of tracked
source while exporting the commit.
Never submit the working tree (`gcloud builds submit ... .`): tracked dirty
changes and untracked files are outside the approved source identity.

```powershell
$projectId = gcloud config get-value project
$sourceCommit = "<full-40-character-git-commit>"
$repository = "daejunnom/Clearra"
$tag = "source-$sourceCommit"
$buildServiceAccount = "projects/$projectId/serviceAccounts/clearra-build@$projectId.iam.gserviceaccount.com"
$archiveRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("clearra-current-job-" + [Guid]::NewGuid().ToString("N"))
$archivePath = Join-Path $archiveRoot "source.tar.gz"
$configContext = Join-Path $archiveRoot "config"

if ($sourceCommit -cnotmatch '^[0-9a-f]{40}$') {
  throw "sourceCommit must be the full lowercase accepted commit SHA"
}
$resolvedCommit = (git rev-parse "$sourceCommit`^{commit}").Trim()
if ($LASTEXITCODE -ne 0 -or $resolvedCommit -cne $sourceCommit) {
  throw "sourceCommit does not resolve exactly to the accepted commit"
}
node apps/clearra-discord-bot/scripts/verify-accepted-source.mjs `
  --source-commit $sourceCommit `
  --repository $repository
if ($LASTEXITCODE -ne 0) { throw "accepted-source preflight failed" }

node apps/clearra-discord-bot/scripts/prepare-cloud-runtime-service-account.mjs `
  --project $projectId
if ($LASTEXITCODE -ne 0) { throw "Cloud runtime IAM bootstrap/preflight failed" }

try {
  New-Item -ItemType Directory -Path $configContext -Force | Out-Null
  node scripts/release/create-exact-source-archive.mjs `
    --source-commit $sourceCommit `
    --output $archivePath
  if ($LASTEXITCODE -ne 0) { throw "exact source archive failed" }
  tar -xzf $archivePath -C $configContext `
    apps/clearra-discord-bot/cloudbuild-current-job-service.yaml
  if ($LASTEXITCODE -ne 0) { throw "build config extraction failed" }

  $buildConfig = Join-Path $configContext "apps/clearra-discord-bot/cloudbuild-current-job-service.yaml"
  gcloud builds submit `
    --project=$projectId `
    --region=asia-northeast1 `
    --service-account=$buildServiceAccount `
    --config=$buildConfig `
    --substitutions="_REGION=asia-northeast1,_REPOSITORY=clearra,_IMAGE_NAME=clearra-current-job,_TAG=$tag,_SOURCE_COMMIT=$sourceCommit" `
    $archivePath
  if ($LASTEXITCODE -ne 0) { throw "Cloud Build submission failed" }
} finally {
  if (Test-Path -LiteralPath $archiveRoot) {
    Remove-Item -LiteralPath $archiveRoot -Recurse -Force
  }
}
```

The Docker build compiles `clearra-cli` in release mode with the current feature
contract and executes tiny `finesse search` and `finesse score` JSON probes. The
service repeats those probes before opening its listen port. Its final stage
contains the CLI, job-service and command-policy sources, production Node
dependencies, and built CTK3 package. Secrets are runtime bindings, never build
arguments or image contents.

The local `.tar.gz` itself is the Cloud Build source boundary. Do not submit a
Windows-extracted directory, because that would discard the verified Unix mode
and symlink contract. The same verified archive is the tracked public-source
layer for an Oracle candidate: transfer it with its SHA-256, recheck the digest,
and extract it on Oracle before applying the separately frozen private overlay.

The full source commit is also baked into the image as both the source and
engine build identity. The image build rejects missing, abbreviated, or
different identities. `/health` and every job envelope return that identity
with the `clearra.search.contract.v2` contract revision.

## Approved Tokyo shape

The active service shape is:

```text
Service:                clearra-current-job
Region:                 asia-northeast1 (Tokyo)
Ingress:                all; public platform invocation
Container port:         8080
Request concurrency:    1
Minimum instances:      0
Maximum instances:      4
CPU per instance:       8 vCPU
Memory per instance:    16 GiB
CPU allocation:         instance-based / no CPU throttling
Startup CPU boost:      enabled
Request timeout:        900 seconds
```

Each instance executes one job at a time. Cloud Run may route four requests to
four instances, so the service can perform at most four searches concurrently
and use up to 32 Clearra workers in aggregate. This is per-instance serial
execution, not a global FIFO or lock. Cloud Run does not offer a 16-vCPU
instance; 8 vCPU is the per-instance ceiling for this service.

The 900-second service timeout admits the 15-minute forward-search policy. The
Discord Gateway still submits an absolute deadline capped at 840 seconds, which
leaves one minute for the synchronous response and final Discord edit.

Set these non-secret runtime values:

```text
CLEARRA_EXECUTABLE=/usr/local/bin/clearra
CLEARRA_SEARCH_TIMEOUT_MS=170000
CLEARRA_REVERSE_SEARCH_TIMEOUT_MS=300000
CLEARRA_FORWARD_SEARCH_TIMEOUT_MS=900000
CLEARRA_EXPECTED_VCPUS=8
CLEARRA_SEARCH_WORKERS_PER_SESSION=8
CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1
CLEARRA_MAX_CONCURRENT_JOBS=1
CLEARRA_MAX_OUTPUT_BYTES=4194304
```

One automatic job receives the service's explicit eight-worker allocation.
Keep `CLEARRA_EXPECTED_VCPUS=8` and the numeric worker bound aligned with
`--cpu=8`. Startup CPU boost is enabled, while a startup-time runtime probe can
still differ from the configured limit; the candidate's Node probe was observed
reporting nine logical processors. Deriving either authority from that
observation is not release-safe. The expected-vCPU setting binds Node's
partition and Rust's resource lease to the deployed CPU limit; the native
runtime still validates that ceiling before creating workers.
`CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1` is an explicit native execution
authority: the job service must preserve it as `--use-all-cpu-threads` even when
the Node startup probe sees nine processors, while the separate numeric worker
bound remains eight. Set
`CLEARRA_USE_ALL_LOGICAL_PROCESSORS=0` only when deliberately reserving one
processor. Caller-supplied worker switches are stripped and cannot exceed the
service policy.

## Authentication boundary

The current Oracle executor sends an application bearer in `Authorization`; it
does not mint a Google identity token. The service therefore requires public
Cloud Run platform invocation, while the application bearer protects `/jobs`.
`/health` is the only unauthenticated application endpoint.

Store the same job bearer in two managed locations:

- a Google Secret Manager Secret bound to Cloud Run as `CLEARRA_JOB_TOKEN`;
- the dedicated OCI Vault job-bearer Secret loaded into Oracle memory at
  startup.

Do not put the value in a deploy command, environment file, image, source file,
or log. The Cloud Run runtime service account needs Secret Accessor on only the
one Google Secret; Oracle's instance principal needs `read secret-bundles` on
the dedicated OCI Vault job-bearer Secret.

The tracked `prepare-cloud-runtime-service-account.mjs` helper is the only
approved bootstrap for the Cloud runtime identity. It reads IAM and Secret
metadata, never a Secret version payload. It creates `clearra-current-job` only
when the account is absent, then re-observes ambiguous create/binding results.
The runtime account must retain zero project-level roles and exactly one
unconditional Secret binding: `roles/secretmanager.secretAccessor` on
`clearra-job-token`. Any access to `discord-bot-token` or another Secret fails
closed. The helper also requires the active caller to be able to submit the
Cloud Build as `clearra-build`, administer the public Cloud Run service, read
the `clearra` Artifact Registry repository, and act as both build and runtime
service accounts. It never grants caller authority automatically.

The universal Secret boundary includes the global catalog plus every supported regional
Secret catalog. The helper obtains the authoritative, nonempty, unique
location set with `gcloud secrets locations list`, accepts only exact
`projects/<project-ID-or-number>/locations/<same-location>` resources, and lists
every location. Global Secret Manager calls are pinned per subprocess to
`https://secretmanager.googleapis.com/`; each regional list and IAM-policy read
is pinned to its exact `https://secretmanager.LOCATION.rep.googleapis.com/`
endpoint with `CLOUDSDK_API_ENDPOINT_OVERRIDES_SECRETMANAGER`. It never changes
the parent process or persistent gcloud configuration. A regional Secret named
`clearra-job-token` is still non-job authority; only the global managed Secret
is eligible for the runtime binding. An empty, malformed, duplicate, unreadable,
wrong-project, wrong-location, or partially enumerated catalog fails closed.

This repository's Cloud project is intentionally parentless. The helper reads
`gcloud projects get-ancestors` before and after IAM preparation and accepts
exactly one `project` row for `clearra-cloud`. It also requires an empty
`PRINCIPAL_ACCESS_BOUNDARY` target-binding search for the exact numeric project.
A new folder/organization parent, a PAB binding, an unreadable search, or drift
during the operation fails closed. Under that mechanically pinned boundary, the
installed GA `gcloud policy-intelligence troubleshoot-policy iam` command is the
effective allow/deny authority. The job Secret must be `CAN_ACCESS`; every
non-job Secret must be `CANNOT_ACCESS`. Identity mismatch, `UNKNOWN`, an
evaluation error, or a missing allow/deny explanation aborts deployment. The
same checks detect inherited and Google-group access that is invisible in a
Secret's direct policy.

Immediately after the runtime account is observed or created, the helper
freshly re-enumerates the complete inventory and rejects drift from its initial
snapshot. The effective authority check then runs before any Secret binding write.
If the direct global job binding is absent, its pre-binding result must be
`CANNOT_ACCESS` too; an inherited or group grant is rejected instead of being
combined with a new direct binding. If the exact direct job binding already
exists, it must already be `CAN_ACCESS`. Every global and regional non-job
Secret must be `CANNOT_ACCESS` in both cases. Only then may the helper add the
one exact job binding. Afterward it freshly re-enumerates all locations and all
global/regional catalogs, requires exact equality with the pre-binding snapshot,
checks every direct and effective permission, and re-enumerates once more to
seal against catalog drift during validation.

For least-privilege bootstrap, grant the caller Cloud Build Editor and Cloud Run
Admin on the project, Artifact Registry Reader on the `clearra` repository, and
Service Account User on the exact build and runtime service accounts. Metadata
preflight also needs project Secret Manager Viewer and Service Account Viewer;
project Security Reviewer is an accepted consolidated read grant and supplies
the Artifact Registry repository-policy read. Effective access evaluation needs
project Security Reviewer, Deny Reviewer, and Service Usage Consumer. If a
policy contains a group or domain, the operator additionally needs the relevant
Google Workspace `groups.read` or domain-admin visibility; otherwise the
result is `UNKNOWN` and fails. Principal-set policies additionally need Browser.
The exact empty-PAB search needs a custom read role containing only
`resourcemanager.projects.searchPolicyBindings`; Project IAM Admin or Owner also
works but carries mutation authority and is not the least-privilege choice.

If the runtime account does not exist, the caller additionally needs Service
Account Creator and project-level Service Account User for the one-time
creation. If the job binding is absent, prefer a custom role containing only
`secretmanager.secrets.getIamPolicy` and
`secretmanager.secrets.setIamPolicy`, bound on `clearra-job-token` alone.
Secret Manager Admin on that exact Secret or Owner also works but is broader.
The helper evaluates the caller's effective `setIamPolicy` permission before
the write and never grants caller authority. Run the helper before the
exact-source build and again immediately before deployment so intervening IAM,
ancestry, PAB, or effective Secret-access drift fails before Cloud Run mutation.

`CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED=1` is restricted to a loopback
listener for local smoke tests. It is not a Cloud Run setting.

## Deployment template

Use the immutable image tag produced above and bind the existing managed Secret
by name. This template intentionally sets both service-level and revision-level
minimum/maximum values:

```powershell
$projectId = gcloud config get-value project
$sourceCommit = "<same-full-40-character-accepted-commit>"
$serviceName = "clearra-current-job"
$jobBearerSecretVersion = "<numeric-enabled-Secret-version>"
$candidateSmokeReportPath = "<new-absolute-canonical-candidate-smoke-report-path>"
$oracleRollbackCaptureEvidencePath = "<new-absolute-Oracle-rollback-capture-evidence-path>"
$oracleObservationEvidencePath = "<new-absolute-Oracle-observation-evidence-path>"
$priorRuntimeAuthorityKind = 'clearra.rollback.legacy-health-no-runtime.v1'
$oracleCandidateReleaseId = "<immutable-candidate-Oracle-release-ID>"
$oracleCandidateReleaseSha256 = "<candidate-Oracle-release-tree-SHA-256>"
$oracleRemoteWrapper = Join-Path (Get-Location) 'scripts/release/oracle/invoke-release-deploy-v080.ps1'
$oracleIdentityFile = '<approved-Oracle-identity-file>'
$deploymentVerifiedAfter = [DateTime]::UtcNow.ToString("o")
$deploymentNonce = [Convert]::ToHexString(
  [Security.Cryptography.RandomNumberGenerator]::GetBytes(32)
).ToLowerInvariant()
$oracleCandidateProofPath = "/run/clearra-deploy/clearra-oracle-candidate-$deploymentNonce.json"
$oracleRollbackProofPath = "/run/clearra-deploy/clearra-oracle-rollback-$deploymentNonce.json"
# The approved Oracle wrapper creates this runtime-only directory as root:root
# mode 0700 and rejects either nonce-bound proof path if it already exists.
# It is an authenticated private transport and maps each operation only to the
# tracked files in `/opt/clearra/releases/$scriptReleaseId`:
# `capture-rollback-authority` runs
# `apps/clearra-discord-bot/scripts/capture-oracle-rollback-authority.mjs`;
# `verify-candidate` activates the exact candidate, obtains a real `/path`, then
# runs `produce-oracle-deployment-proof.mjs candidate` and
# `verify-oracle-candidate-proof.mjs`; `restore-prior-and-verify` runs
# `restore-oracle-release`, obtains a fresh real `/path`, then runs
# `produce-oracle-deployment-proof.mjs rollback` and
# `verify-oracle-rollback-proof.mjs`. It passes only non-secret exact arguments
# plus the nonce and never prints credentials, settings contents, or job data.
# Before accepting any operation, the remote launcher and every mapped helper
# must be root:root mode 0755 regular non-symlink files. An ubuntu-owned,
# group/other-writable, or mode-0666 launcher is stale authority and must fail
# before capture, candidate activation, or service mutation.
# The explicit legacy authority kind is a one-time v0.7.4 migration bridge. It
# accepts only an exact `v0.7.4-<sha7>` Oracle release paired with
# `clearra-current-job-v075-<same-sha7>` and exactly the legacy
# `status`/`activeJobs`/`workerLimit` health profile. It never synthesizes a
# runtime identity. Future v2 rollbacks must instead explicitly use
# `clearra.rollback.runtime-identity.v1`; missing identity never falls back.
# Before every operation, the trusted wrapper independently computes the exact
# script-release tree digest (without executing candidate-supplied code), rejects
# dangling or out-of-tree links, and compares it with `--script-release-sha256`.

if ($sourceCommit -cnotmatch '^[0-9a-f]{40}$') {
  throw "sourceCommit must be the full lowercase accepted commit SHA"
}
node apps/clearra-discord-bot/scripts/prepare-cloud-runtime-service-account.mjs `
  --project $projectId
if ($LASTEXITCODE -ne 0) { throw "Cloud runtime IAM preflight drifted before deploy" }
$serviceBefore = gcloud run services describe $serviceName `
  --project=$projectId --region=asia-northeast1 --format=json | ConvertFrom-Json
$priorTraffic = @($serviceBefore.status.traffic | Where-Object { [int]$_.percent -eq 100 })
if ($priorTraffic.Count -ne 1 -or -not $priorTraffic[0].revisionName) {
  throw "Cloud Run must have exactly one prior 100-percent revision before candidate deployment"
}
$priorRevision = [string]$priorTraffic[0].revisionName

# The authenticated private wrapper runs the tracked capture helper as root on
# Oracle before either Cloud or Oracle mutation. It returns only non-secret
# release/settings/runtime-authority digests and the root-owned settings backup
# path.
$priorCaptureJson = & $oracleRemoteWrapper `
  -Operation capture-rollback-authority `
  -ScriptReleaseId $oracleCandidateReleaseId `
  -ScriptReleaseSha256 $oracleCandidateReleaseSha256 `
  -PriorRevision $priorRevision `
  -PriorRuntimeAuthorityKind $priorRuntimeAuthorityKind `
  -DeploymentNonce $deploymentNonce `
  -EvidenceOutput $oracleRollbackCaptureEvidencePath `
  -IdentityFile $oracleIdentityFile
if ($LASTEXITCODE -ne 0) { throw "prior Oracle rollback authority capture failed" }
try { $priorCapture = $priorCaptureJson | ConvertFrom-Json } catch {
  throw "prior Oracle rollback authority capture returned invalid JSON"
}
if ($priorCapture.priorRevision -cne $priorRevision -or
    $priorCapture.deploymentNonce -cne $deploymentNonce -or
    $priorCapture.priorOracleReleaseId -cnotmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$' -or
    $priorCapture.priorOracleRelease -cne "/opt/clearra/releases/$($priorCapture.priorOracleReleaseId)" -or
    $priorCapture.priorOracleReleaseSha256 -cnotmatch '^[0-9a-f]{64}$' -or
    $priorCapture.priorOracleSettingsBackup -cnotmatch '^/etc/clearra-gateway/settings\.pre-v0\.8\.0-[0-9a-f]{64}$' -or
    $priorCapture.priorOracleSettingsSha256 -cnotmatch '^[0-9a-f]{64}$' -or
    $priorCapture.priorRuntimeAuthorityKind -cne $priorRuntimeAuthorityKind -or
    $priorCapture.priorRuntimeAuthoritySha256 -cnotmatch '^[0-9a-f]{64}$' -or
    $priorCapture.priorJobUrl -cnotmatch '^https://[^/]+/jobs$') {
  throw "prior Oracle rollback authority capture is incomplete"
}
$priorOracleRelease = [string]$priorCapture.priorOracleRelease
$priorOracleReleaseId = [string]$priorCapture.priorOracleReleaseId
$priorOracleReleaseSha256 = [string]$priorCapture.priorOracleReleaseSha256
$priorOracleSettingsBackup = [string]$priorCapture.priorOracleSettingsBackup
$priorOracleSettingsSha256 = [string]$priorCapture.priorOracleSettingsSha256
$priorRuntimeAuthorityKind = [string]$priorCapture.priorRuntimeAuthorityKind
$priorRuntimeAuthoritySha256 = [string]$priorCapture.priorRuntimeAuthoritySha256
$priorJobUrl = [string]$priorCapture.priorJobUrl

# Bracket the stable-URL health observation with the exact immutable revision.
# Any concurrent traffic drift invalidates the captured authority before deploy.
$serviceAfterCapture = gcloud run services describe $serviceName `
  --project=$projectId --region=asia-northeast1 --format=json | ConvertFrom-Json
$priorTrafficAfterCapture = @($serviceAfterCapture.status.traffic | Where-Object {
  [int]$_.percent -eq 100
})
if ($priorTrafficAfterCapture.Count -ne 1 -or
    $priorTrafficAfterCapture[0].revisionName -cne $priorRevision) {
  throw "prior Cloud revision changed during rollback authority capture"
}

# The tracked helper resolves the mutable source tag to one Artifact Registry
# image@sha256 before mutation, deploys only that digest, pins the numeric Secret
# version, and independently reads back service/revision identity, resources,
# Secret reference, and exact tagged zero-traffic isolation.
$candidateJson = & node scripts/release/cloud/candidate-release-v080.mjs deploy `
  --project $projectId `
  --source-commit $sourceCommit `
  --prior-revision $priorRevision `
  --job-bearer-secret-version $jobBearerSecretVersion
if ($LASTEXITCODE -ne 0) { throw "canonical zero-traffic candidate deployment failed" }
try { $candidate = $candidateJson | ConvertFrom-Json } catch {
  throw "canonical zero-traffic candidate deployment returned invalid JSON"
}
$candidateRevision = "clearra-current-job-v080-$($sourceCommit.Substring(0, 7))"
$candidateTag = "candidate-$($sourceCommit.Substring(0, 7))"
if ($candidate.contract -cne 'clearra.cloud.zero-traffic-candidate.v1' -or
    $candidate.sourceCommit -cne $sourceCommit -or
    $candidate.projectId -cne $projectId -or
    $candidate.region -cne 'asia-northeast1' -or
    $candidate.service -cne $serviceName -or
    $candidate.priorRevision -cne $priorRevision -or
    $candidate.candidateRevision -cne $candidateRevision -or
    $candidate.candidateTag -cne $candidateTag -or
    $candidate.candidateUrl -cnotmatch '^https://[^/]+\.run\.app$' -or
    $candidate.imageDigest -cnotmatch '^asia-northeast1-docker\.pkg\.dev/[^/]+/clearra/clearra-current-job@sha256:[0-9a-f]{64}$' -or
    [string]$candidate.jobBearerSecretVersion -cne $jobBearerSecretVersion) {
  throw "canonical zero-traffic candidate authority is incomplete"
}
$candidateUrl = [string]$candidate.candidateUrl
$candidateImage = [string]$candidate.imageDigest
$candidateImageDigest = $candidateImage.Substring($candidateImage.LastIndexOf('@') + 1)
$oracleCandidateSettingsSha256 = (& node `
  scripts/release/oracle/candidate-settings-v080.mjs `
  --source-commit $sourceCommit `
  --candidate-url $candidateUrl `
  --hash-only).Trim()
if ($LASTEXITCODE -ne 0 -or
    $oracleCandidateSettingsSha256 -cnotmatch '^[0-9a-f]{64}$') {
  throw "canonical Oracle candidate settings authority failed"
}
$health = Invoke-RestMethod -Method Get -Uri "$candidateUrl/health"
if ($health.runtime.sourceCommit -cne $sourceCommit -or
    $health.runtime.engineBuildId -cne $sourceCommit -or
    $health.runtime.contractSchemaVersion -cne 'clearra.search.contract.v2' -or
    $health.runtime.supplySemanticsId -cne 'clearra.supply.projected-terminal-lookahead.v1' -or
    $health.runtime.artifactSchemaVersion -cne 'clearra.solution-data.v1') {
  throw "candidate health runtime identity mismatch"
}

# The same helper now creates one ephemeral Cloud Run Job with the same immutable
# image, injects only the same numeric managed Secret version, invokes the tagged
# zero-traffic `/jobs` endpoint, binds the completed execution-specific log
# attestation, seals the control-plane hashes, and removes the Job. No bearer is
# read by or exported to the operator process.
node scripts/release/cloud/candidate-release-v080.mjs smoke `
  --project $projectId `
  --source-commit $sourceCommit `
  --prior-revision $priorRevision `
  --job-bearer-secret-version $jobBearerSecretVersion `
  --image-digest $candidateImage `
  --candidate-url $candidateUrl `
  --output $candidateSmokeReportPath
if ($LASTEXITCODE -ne 0) { throw "managed zero-traffic candidate smoke failed" }

# Confirm Ready/startup/error logs, resource/spec parity, and an unchanged IAM
# policy. This is an authenticated remote boundary, not a local `sudo`: the
# approved wrapper connects to Oracle without printing credentials, atomically
# activates the immutable candidate release/settings, waits for a real `/path`,
# and invokes the tracked candidate producer and one-shot consumer as root. The
# producer independently observes the active symlink/tree digest, settings
# digest and all five expected settings, current PID/cwd, READY record, and a
# fresh successful Gateway operation newer than `$deploymentVerifiedAfter`.
& $oracleRemoteWrapper `
  -Operation verify-candidate `
  -ScriptReleaseId $oracleCandidateReleaseId `
  -ScriptReleaseSha256 $oracleCandidateReleaseSha256 `
  -Proof $oracleCandidateProofPath `
  -SourceCommit $sourceCommit `
  -CandidateUrl $candidateUrl `
  -CandidateRevision $candidateRevision `
  -OracleReleaseId $oracleCandidateReleaseId `
  -OracleReleaseSha256 $oracleCandidateReleaseSha256 `
  -OracleSettingsSha256 $oracleCandidateSettingsSha256 `
  -DeploymentNonce $deploymentNonce `
  -VerifiedAfter $deploymentVerifiedAfter `
  -IdentityFile $oracleIdentityFile
$candidateOracleExit = $LASTEXITCODE
if ($candidateOracleExit -ne 0) {
  # Cloud still serves the prior revision. Restore and freshly verify the exact
  # captured Oracle authority before aborting this deployment.
  $preCutoverRollbackVerifiedAfter = [DateTime]::UtcNow.ToString("o")
  & $oracleRemoteWrapper `
    -Operation restore-prior-and-verify `
    -ScriptReleaseId $oracleCandidateReleaseId `
    -ScriptReleaseSha256 $oracleCandidateReleaseSha256 `
    -PriorRelease $priorOracleRelease `
    -PriorReleaseId $priorOracleReleaseId `
    -PriorReleaseSha256 $priorOracleReleaseSha256 `
    -PriorSettingsBackup $priorOracleSettingsBackup `
    -PriorSettingsSha256 $priorOracleSettingsSha256 `
    -PriorRuntimeAuthorityKind $priorRuntimeAuthorityKind `
    -PriorRuntimeAuthoritySha256 $priorRuntimeAuthoritySha256 `
    -PriorJobUrl $priorJobUrl `
    -PriorRevision $priorRevision `
    -Proof $oracleRollbackProofPath `
    -DeploymentNonce $deploymentNonce `
    -VerifiedAfter $preCutoverRollbackVerifiedAfter `
    -IdentityFile $oracleIdentityFile
  if ($LASTEXITCODE -ne 0) {
    throw "candidate Oracle failed and exact prior Oracle restore was not verified; keep the service stopped for manual recovery"
  }
  throw "candidate Oracle verification failed; exact prior Oracle authority was restored and Cloud traffic was not changed"
}

gcloud run services update-traffic $serviceName `
  --project=$projectId `
  --region=asia-northeast1 `
  --to-revisions="$candidateRevision=100"
if ($LASTEXITCODE -ne 0) { throw "candidate traffic cutover failed" }

$serviceAfter = gcloud run services describe $serviceName `
  --project=$projectId --region=asia-northeast1 --format=json | ConvertFrom-Json
$activeAfter = @($serviceAfter.status.traffic | Where-Object { [int]$_.percent -eq 100 })
$candidateTaggedAfter = @($serviceAfter.status.traffic | Where-Object {
  $_.tag -eq $candidateTag -and $_.revisionName -eq $candidateRevision -and $_.url -eq $candidateUrl
})
if ($activeAfter.Count -ne 1 -or $activeAfter[0].revisionName -ne $candidateRevision -or
    $candidateTaggedAfter.Count -ne 1) {
  throw "candidate did not become the sole 100-percent revision with its exact tagged URL preserved"
}
```

Both pairs are required. Setting only `--max=4` can leave the active revision's
`maxScale` at its lower default, making three—not four—the effective cap.
Likewise, set both minimum flags to make scale-to-zero explicit at both levels.
After every candidate deploy, inspect the service and candidate revision
separately and verify concurrency 1, effective min 0, explicit max 4, 8 vCPU,
and 16 GiB before the explicit traffic mutation. Cloud Run may canonicalize an
explicit zero minimum by omitting every recognized minimum field; that omission
alone is the default-zero authority. If any minimum field is present, every one
must be exactly zero. At least one maximum field must be present, and every
present maximum must be exactly four; null, malformed, nonzero, or conflicting
duplicates fail closed. A candidate failure leaves `$priorRevision`
serving 100 percent. Keep Oracle pinned to the exact tagged
`$candidateUrl/jobs` through command sync and the pre-sync rollback window;
stable-URL rebinding is forbidden during that window. If a post-cutover gate
fails **before command sync**, restore both exact authorities in this order:

```powershell
$rollbackVerifiedAfter = [DateTime]::UtcNow.ToString("o")
gcloud run services update-traffic $serviceName `
  --project=$projectId `
  --region=asia-northeast1 `
  --to-revisions="$priorRevision=100"
if ($LASTEXITCODE -ne 0) { throw "prior Cloud revision rollback failed" }

$serviceRolledBack = gcloud run services describe $serviceName `
  --project=$projectId --region=asia-northeast1 --format=json | ConvertFrom-Json
$activeRolledBack = @($serviceRolledBack.status.traffic | Where-Object { [int]$_.percent -eq 100 })
if ($activeRolledBack.Count -ne 1 -or $activeRolledBack[0].revisionName -ne $priorRevision) {
  throw "prior Cloud revision did not become the sole 100-percent revision"
}

# Through the same authenticated remote boundary, restore the captured release
# and settings with the tracked helper. The wrapper then waits for a real
# bounded `/path`, runs the trusted rollback producer, and consumes its root-only
# proof. Failure leaves the Oracle service stopped instead of running mixed or
# unverified state.
& $oracleRemoteWrapper `
  -Operation restore-prior-and-verify `
  -ScriptReleaseId $oracleCandidateReleaseId `
  -ScriptReleaseSha256 $oracleCandidateReleaseSha256 `
  -PriorRelease $priorOracleRelease `
  -PriorReleaseId $priorOracleReleaseId `
  -PriorReleaseSha256 $priorOracleReleaseSha256 `
  -PriorSettingsBackup $priorOracleSettingsBackup `
  -PriorSettingsSha256 $priorOracleSettingsSha256 `
  -PriorRuntimeAuthorityKind $priorRuntimeAuthorityKind `
  -PriorRuntimeAuthoritySha256 $priorRuntimeAuthoritySha256 `
  -PriorJobUrl $priorJobUrl `
  -PriorRevision $priorRevision `
  -Proof $oracleRollbackProofPath `
  -DeploymentNonce $deploymentNonce `
  -VerifiedAfter $rollbackVerifiedAfter `
  -IdentityFile $oracleIdentityFile
if ($LASTEXITCODE -ne 0) {
  throw "prior Oracle rollback was not freshly verified; keep the service stopped for manual recovery"
}
```

After global command synchronization this rollback recipe is no longer
authorized. Reverting then requires independently accepted backward-compatible
command-schema evidence and an exact command-catalog restore; do not improvise
that mutation during an incident.

## Protocol

```text
GET    /health
POST   /jobs
GET    /jobs/:id
DELETE /jobs/:id
```

`POST /jobs` requires:

```text
Authorization: Bearer <managed job bearer>
Idempotency-Key: <same value as body.id>
Content-Type: application/json
```

The service re-applies the Discord command policy, removes caller-controlled
worker and output switches, injects its local worker ceiling, and starts Clearra
with `shell: false`. It enforces the absolute deadline and a bounded combined
stdout/stderr allowance. `POST /jobs` remains open until Clearra finishes and
returns a terminal result. `GET /jobs/:id` and `DELETE /jobs/:id` are best-
effort compatibility endpoints for requests routed to the same instance; they
are not a durable distributed queue.

Oracle caller settings are:

```text
NODE_ENV=production
CLEARRA_JOB_URL=https://<clearra-current-job service>/jobs
CLEARRA_EXPECTED_JOB_SOURCE_COMMIT=<full-40-character-git-commit>
CLEARRA_EXPECTED_ENGINE_BUILD_ID=<same-full-40-character-git-commit>
CLEARRA_EXPECTED_JOB_CONTRACT_REVISION=clearra.search.contract.v2
CLEARRA_EXPECTED_SUPPLY_SEMANTICS_ID=clearra.supply.projected-terminal-lookahead.v1
CLEARRA_EXPECTED_ARTIFACT_SCHEMA_VERSION=clearra.solution-data.v1
CLEARRA_WORKER_AUTHORITY=remote
CLEARRA_MAX_CONCURRENT_REMOTE_JOBS=1
CLEARRA_SEARCH_TIMEOUT_MS=180000
CLEARRA_REVERSE_SEARCH_TIMEOUT_MS=300000
CLEARRA_FORWARD_SEARCH_TIMEOUT_MS=900000
CLEARRA_INTERACTION_DEADLINE_MS=840000
```

The job bearer is supplied by the Oracle Vault wrapper, not the settings file.
An external job URL must use HTTPS, contain no URL credentials, and be paired
with the bearer. With remote worker authority, Oracle's two logical processors
do not cap Cloud Run's eight-worker job.

## Exact-SHA command synchronization

Command synchronization is the final public mutation. It must run from a fresh
temporary extraction of the commit-byte archive for the same accepted commit as
the healthy Oracle/runtime release. It must not use the checkout, `.`, a prior
extraction, or a Cloud Build step that discards the pre-mutation catalog. The
tracked producer writes the canonical source catalog first, persists and
`fsync`s an independent GET of the exact prior writable catalog immediately
before its one possible bulk PUT, and seals an independent post-write GET.
The retired `cloudbuild-command-sync.yaml` path is not release-authoritative
because it cannot return this durable prior snapshot and producer report.
The fresh extraction downloads the exact run/attempt-bound accepted CTK3
distribution and canonical acceptance evidence, verifies both against the
source commit, and seals them with the canonical catalog in one non-secret
command-sync authority. It never rebuilds CTK3 during command synchronization.
Perform this only after the runtime identity and bounded smoke job checks have
passed. Reuse the already initialized evidence directory outside the source tree;
it must remain available through final manifest validation:

```powershell
$projectId = gcloud config get-value project
$sourceCommit = "<same-full-40-character-accepted-commit>"
$repository = "daejunnom/Clearra"
$serviceName = "clearra-current-job"
$applicationId = "1533373054309371924"
$acceptedRunId = "<canonical-successful-workflow-dispatch-run-id>"
$acceptedRunAttempt = "<exact-positive-run-attempt>"
$evidenceDirectory = "<same-retained-absolute-release-evidence-directory>"
$archiveRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("clearra-command-sync-" + [Guid]::NewGuid().ToString("N"))
$archivePath = Join-Path $archiveRoot "source.tar.gz"
$sourceContext = Join-Path $archiveRoot "source"
$catalogPath = Join-Path $evidenceDirectory "discord-command-catalog.json"
$priorCatalogPath = Join-Path $evidenceDirectory "discord-command-catalog-prior.json"
$syncReportPath = Join-Path $evidenceDirectory "discord-command-catalog-sync.json"
$syncAuthorityPath = Join-Path $evidenceDirectory "discord-command-sync-authority.json"
$acceptedCtk3ArtifactName = "ctk3-accepted-$sourceCommit-run-$acceptedRunId-attempt-$acceptedRunAttempt"
$acceptedCtk3Dist = Join-Path $sourceContext "packages/ctk3/dist"
$canonicalAcceptanceDirectory = Join-Path $evidenceDirectory "canonical-acceptance-evidence"
$canonicalAcceptanceEvidencePath = Join-Path $canonicalAcceptanceDirectory "clearra-canonical-acceptance-evidence.v1.json"

if ($sourceCommit -cnotmatch '^[0-9a-f]{40}$' -or
    $acceptedRunId -cnotmatch '^[1-9][0-9]{0,19}$' -or
    $acceptedRunAttempt -cnotmatch '^[1-9][0-9]{0,19}$' -or
    $applicationId -notmatch '^\d{17,20}$' -or
    -not (Test-Path -LiteralPath $evidenceDirectory -PathType Container)) {
  throw "command-sync source/run/application/evidence authority is invalid"
}
$resolvedCommit = (git rev-parse "$sourceCommit`^{commit}").Trim()
if ($LASTEXITCODE -ne 0 -or $resolvedCommit -cne $sourceCommit) {
  throw "sourceCommit does not resolve exactly to the accepted commit"
}
$activeService = gcloud run services describe $serviceName `
  --project=$projectId --region=asia-northeast1 --format=json | ConvertFrom-Json
$activeTraffic = @($activeService.status.traffic | Where-Object { [int]$_.percent -eq 100 })
if ($activeTraffic.Count -ne 1 -or -not $activeService.status.url) {
  throw "command sync requires one active runtime revision"
}
try {
  New-Item -ItemType Directory -Path $sourceContext -Force | Out-Null
  node scripts/release/create-exact-source-archive.mjs `
    --source-commit $sourceCommit `
    --output $archivePath
  if ($LASTEXITCODE -ne 0) { throw "exact source archive failed" }
  tar -xzf $archivePath -C $sourceContext
  if ($LASTEXITCODE -ne 0) { throw "command-sync source extraction failed" }

  if ((Test-Path -LiteralPath $acceptedCtk3Dist) -or
      -not (Test-Path -LiteralPath $canonicalAcceptanceEvidencePath -PathType Leaf)) {
    throw "command-sync accepted CTK3 output must be absent and acceptance authority must already exist"
  }
  New-Item -ItemType Directory -Path $acceptedCtk3Dist | Out-Null
  gh run download $acceptedRunId `
    --repo $repository `
    --name $acceptedCtk3ArtifactName `
    --dir $acceptedCtk3Dist
  if ($LASTEXITCODE -ne 0) { throw "accepted CTK3 artifact download failed" }

  Push-Location $sourceContext
  try {
    node apps/clearra-discord-bot/scripts/verify-accepted-source.mjs `
      --source-commit $sourceCommit `
      --repository $repository `
      --active-health-url ([string]$activeService.status.url)
    if ($LASTEXITCODE -ne 0) { throw "accepted runtime preflight failed before command sync" }

    npm ci --ignore-scripts
    if ($LASTEXITCODE -ne 0) { throw "command-sync dependency install failed" }
    node scripts/tools/accepted-ctk3-dist.mjs `
      --verify $acceptedCtk3Dist `
      --expected-source-commit $sourceCommit `
      --expected-run-id $acceptedRunId `
      --expected-run-attempt $acceptedRunAttempt
    if ($LASTEXITCODE -ne 0) { throw "command-sync accepted CTK3 authority failed" }

    node apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs `
      canonical --source-commit $sourceCommit --output $catalogPath
    if ($LASTEXITCODE -ne 0) { throw "canonical Discord catalog production failed" }

    node scripts/release/discord-command-sync-authority.mjs `
      --source-commit $sourceCommit `
      --repository $repository `
      --version 0.8.0 `
      --base-path /Clearra `
      --accepted-run-id $acceptedRunId `
      --accepted-run-attempt $acceptedRunAttempt `
      --accepted-ctk3-dist $acceptedCtk3Dist `
      --canonical-acceptance-evidence $canonicalAcceptanceEvidencePath `
      --catalog $catalogPath `
      --output $syncAuthorityPath
    if ($LASTEXITCODE -ne 0) { throw "Discord command sync authority production failed" }
    $syncAuthorityFileSha256 = (Get-FileHash `
      -Algorithm SHA256 `
      -LiteralPath $syncAuthorityPath).Hash.ToLowerInvariant()

    # DISCORD_TOKEN must already exist only in this process environment, injected
    # by the approved Secret Manager wrapper or masked prompt. Never pass it as
    # an argument, write it to a file, or print the environment.
    node apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs `
      sync --source-commit $sourceCommit --application-id $applicationId `
      --catalog $catalogPath --prior-output $priorCatalogPath `
      --sync-authority $syncAuthorityPath `
      --sync-authority-file-sha256 $syncAuthorityFileSha256 `
      --output $syncReportPath
    if ($LASTEXITCODE -ne 0) { throw "Discord catalog sync or readback failed" }
  } finally {
    Pop-Location
  }
}
catch {
  # Keep the exact extracted tool and every already-written non-secret evidence
  # file for investigation or a conditional restore. Do not retry the PUT.
  throw
}
```

The prior snapshot is durable before the PUT begins. If any later gate fails
before tag/release publication, restore it only with the exact post-sync digest
from the sealed sync report. The helper independently GETs the live catalog,
compares its current digest, and refuses the mutation if another actor changed
it; after the one possible PUT it
requires an exact prior-catalog readback. Use a new output filename:

```powershell
$syncReport = Get-Content -LiteralPath $syncReportPath -Raw | ConvertFrom-Json
$restoreReportPath = Join-Path $evidenceDirectory "discord-command-catalog-restore.json"
Push-Location $sourceContext
try {
  node apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs `
    restore --source-commit $sourceCommit --application-id $applicationId `
    --prior-snapshot $priorCatalogPath `
    --expected-current-digest ([string]$syncReport.current_after_sha256) `
    --output $restoreReportPath
  if ($LASTEXITCODE -ne 0) {
    throw "Discord catalog conditional restore/readback failed"
  }
} finally {
  Pop-Location
}
```

Do not remove `$archiveRoot` or the evidence directory until the 1,200-second
observation, final manifest materialization, and validator have all succeeded.
The token remains environment-only and is never a catalog/report field.

## 1,200-second four-surface observation

After command sync, use `scripts/release/observe-production-surfaces.mjs` as the
sole report producer. Its probe spec is non-secret canonical JSON with exactly
four SHA-256-bound adapters: `discord`, `oracle`, `cloud`, and `pages`. Each
Node adapter must return one canonical `clearra.production-surface-probe.v1`
object per invocation. The Oracle PowerShell owner returns the closed
`clearra.oracle.candidate-observation.v1` readback, which the orchestrator
normalizes without inventing a public endpoint. Authentication values are
inherited from the environment only; they are forbidden in the authority,
spec, adapter output, observation report, journal, and manifest.

- Discord performs a new localized global-command GET and returns the exact
  application, source-catalog digest, readback digest, sync-report digest,
  sorted `type:name` set, and count. Its identity also preserves the accepted
  run ID/attempt, accepted CTK3 manifest SHA-256, canonical acceptance report
  and raw-file SHA-256, command-catalog raw-file SHA-256, and command-sync
  authority report and raw-file SHA-256.
- Oracle uses the approved SSH read-only ops wrapper, not an invented public
  health endpoint. It returns active release/tree/settings SHA-256, pinned PID,
  boot ID, unchanged monotonic process-start value, READY state, the fixed
  `VerifiedAfter` authority, the latest qualifying successful `/path`, and a
  strictly increasing read-only observation timestamp. The first sample accepts
  the action-time-confirmed candidate `/path` as its baseline. Every later
  sample requires a successful `/path` newer than the preceding remote
  observation, and the observer fails that sample immediately if the operation
  or observation clock regresses.
- Cloud reads both stable and tagged `/health` identities and independently
  reads the active revision, immutable image digest, 100-percent traffic, CPU,
  memory, concurrency, min/max instances, and startup CPU boost. It must also
  consume the sealed `clearra.cloud.candidate-smoke.v1` report that independently
  proved this same image/revision/tag at zero traffic and ran one managed-secret
  authenticated bounded `/jobs` smoke without recording the bearer or result.
- Pages accepts only a sealed deployment-report path, its raw-file SHA-256, and
  a timeout. The adapter derives source/engine/version, deployment ID, artifact
  authority, base path, and URL from that report, then performs cache-busted
  public identity reads and an authenticated Pages deployment-status API read
  for every sample. Both readbacks are sealed independently.

Create the non-secret `clearra.production-observation-probe-authority.v1` JSON
from the exact deployment journal values. Its Discord authority contains the
catalog, sync report, command-sync authority paths and all three raw file hashes.
Its Cloud authority contains project/region/service/revision/tag/image plus the
candidate-smoke path/hash. Its Pages authority contains only
`deployment_report_path`, `deployment_report_file_sha256`, and
`timeout_seconds`; every other Pages value is derived from the sealed report.
Its Oracle authority contains the wrapper path/hash with exact release,
settings, URL, revision, nonce, and verified-after values. It never contains
`DISCORD_TOKEN` or `CLEARRA_ORACLE_IDENTITY_FILE`; those are runtime environment
inputs only. The production authority requires `interval_seconds` to be exactly
`1200`, and the report requires the same exact 1,200-second duration, exactly
two samples, sample 0 at `started_at`, and sample 1 at `ended_at`. After the
start sample succeeds, perform one freshly confirmed real `/path` during the
window; the end sample must observe that operation. The tracked materializer
verifies every local regular non-link file and emits the only accepted
hash-bound spec:

```powershell
$probeAuthorityPath = Join-Path $evidenceDirectory "production-observation-authority.json"
$probeSpecPath = Join-Path $evidenceDirectory "production-observation-probes.json"
$observationReportPath = Join-Path $evidenceDirectory "production-observation.json"
node scripts/release/materialize-production-probe-spec.mjs `
  --authority $probeAuthorityPath `
  --output $probeSpecPath
if ($LASTEXITCODE -ne 0) { throw "production probe-spec materialization failed" }

node scripts/release/observe-production-surfaces.mjs `
  --source-commit $sourceCommit `
  --probe-spec $probeSpecPath `
  --output $observationReportPath
if ($LASTEXITCODE -ne 0) { throw "four-surface production observation failed" }

# Persist one additional typed Oracle read-only observation for direct
# deployment-stage consumption. The output path must be absolute, new, and
# beneath a regular non-link directory chain.
$oracleObservationJson = & $oracleRemoteWrapper `
  -Operation observe-candidate `
  -ScriptReleaseId $oracleCandidateReleaseId `
  -ScriptReleaseSha256 $oracleCandidateReleaseSha256 `
  -SourceCommit $sourceCommit `
  -CandidateUrl $candidateUrl `
  -CandidateRevision $candidateRevision `
  -OracleReleaseId $oracleCandidateReleaseId `
  -OracleReleaseSha256 $oracleCandidateReleaseSha256 `
  -OracleSettingsSha256 $oracleCandidateSettingsSha256 `
  -DeploymentNonce $deploymentNonce `
  -VerifiedAfter $deploymentVerifiedAfter `
  -EvidenceOutput $oracleObservationEvidencePath `
  -IdentityFile $oracleIdentityFile
if ($LASTEXITCODE -ne 0) { throw "durable Oracle observation failed" }
```

The production observation CLI has no duration override; it always observes
for at least 1,200 seconds, records the exact probe-spec and all four adapter
SHA-256 values, rehashes every adapter before every invocation, and closes the
last sample at the report end time. Tests use injected probes and a short fake
clock instead.

## Final-source staged evidence and publication closure

The final journal accepts no hand-written event payloads. It admits exactly
three source-bound stage reports, in `acceptance`, `deployment`, `publication`
order, and appends each complete stage as one atomic replacement batch. The
acceptance phase above has already run before mutation. Phase 2 runs only after
command synchronization, the full 1,200-second observation, and the durable
Oracle observation have succeeded. Phase 3 runs
only after the exact annotated tag workflow and its separate publication
finalizer have both completed successfully.

All remaining output paths below are new regular non-link files under the one
retained evidence directory. The Pages capture and deployment reports are the exact downloaded
run/attempt-bound workflow artifacts; they are not operator-transcribed JSON.
The publication resolver uses the authenticated `gh` CLI internally and accepts
no token, artifact ID, artifact name, or digest argument.

```powershell
$releaseTag = "v0.8.0"
$sourceRoot = (Get-Location).Path
$attemptJournal = Join-Path $evidenceDirectory "final-source-attempt.jsonl"
$finalManifest = Join-Path $evidenceDirectory "final-source-revalidation.json"
$acceptanceStageEvidencePath = Join-Path $evidenceDirectory "final-source-acceptance-stage.json"
$deploymentStageEvidencePath = Join-Path $evidenceDirectory "final-source-deployment-stage.json"
$publicationStageEvidencePath = Join-Path $evidenceDirectory "final-source-publication-stage.json"
$pagesDeploymentAuthorityPath = Join-Path $evidenceDirectory "pages-deployment-authority.json"
$pagesRollbackCapturePath = Join-Path $evidenceDirectory "pages-rollback-capture-authority.json"
$publicationResolutionDirectory = Join-Path $evidenceDirectory "release-publication-final"

# Re-read the already appended acceptance-stage bytes after the long deployment
# window instead of relying on an in-memory digest.
$acceptanceStageEvidenceFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $acceptanceStageEvidencePath
).Hash.ToLowerInvariant()

# Phase 2: execute after the full observation and durable Oracle observation.
$pagesDeploymentAuthorityFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $pagesDeploymentAuthorityPath
).Hash.ToLowerInvariant()
$pagesRollbackCaptureFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $pagesRollbackCapturePath
).Hash.ToLowerInvariant()
$catalogFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $catalogPath
).Hash.ToLowerInvariant()
$priorCatalogFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $priorCatalogPath
).Hash.ToLowerInvariant()
$syncAuthorityFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $syncAuthorityPath
).Hash.ToLowerInvariant()
$syncReportFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $syncReportPath
).Hash.ToLowerInvariant()
$candidateSmokeReportFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $candidateSmokeReportPath
).Hash.ToLowerInvariant()
$oracleRollbackCaptureEvidenceFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $oracleRollbackCaptureEvidencePath
).Hash.ToLowerInvariant()
$oracleObservationEvidenceFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $oracleObservationEvidencePath
).Hash.ToLowerInvariant()
$probeSpecFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $probeSpecPath
).Hash.ToLowerInvariant()
$observationReportFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $observationReportPath
).Hash.ToLowerInvariant()

node scripts/release/final-source-stage-evidence.mjs deployment `
  --expected-source-commit $sourceCommit `
  --pages-deployment-authority $pagesDeploymentAuthorityPath `
  --pages-deployment-authority-file-sha256 $pagesDeploymentAuthorityFileSha256 `
  --pages-rollback-capture $pagesRollbackCapturePath `
  --pages-rollback-capture-file-sha256 $pagesRollbackCaptureFileSha256 `
  --discord-catalog $catalogPath `
  --discord-catalog-file-sha256 $catalogFileSha256 `
  --discord-prior-snapshot $priorCatalogPath `
  --discord-prior-snapshot-file-sha256 $priorCatalogFileSha256 `
  --discord-command-sync-authority $syncAuthorityPath `
  --discord-command-sync-authority-file-sha256 $syncAuthorityFileSha256 `
  --discord-catalog-sync-report $syncReportPath `
  --discord-catalog-sync-report-file-sha256 $syncReportFileSha256 `
  --cloud-candidate-smoke-report $candidateSmokeReportPath `
  --cloud-candidate-smoke-report-file-sha256 $candidateSmokeReportFileSha256 `
  --oracle-rollback-capture $oracleRollbackCaptureEvidencePath `
  --oracle-rollback-capture-file-sha256 $oracleRollbackCaptureEvidenceFileSha256 `
  --oracle-observation $oracleObservationEvidencePath `
  --oracle-observation-file-sha256 $oracleObservationEvidenceFileSha256 `
  --production-probe-spec $probeSpecPath `
  --production-probe-spec-file-sha256 $probeSpecFileSha256 `
  --production-observation-report $observationReportPath `
  --production-observation-report-file-sha256 $observationReportFileSha256 `
  --output $deploymentStageEvidencePath
if ($LASTEXITCODE -ne 0) { throw "deployment stage evidence failed" }
$deploymentStageEvidenceFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $deploymentStageEvidencePath
).Hash.ToLowerInvariant()

node scripts/release/final-source-attempt-journal.mjs append-stage `
  --journal $attemptJournal `
  --stage-evidence $deploymentStageEvidencePath `
  --stage-evidence-file-sha256 $deploymentStageEvidenceFileSha256
if ($LASTEXITCODE -ne 0) { throw "deployment stage journal append failed" }

# Phase 3: fill these from the one successful v0.8.0 tag workflow attempt.
$publicationRunId = "<successful-tag-workflow-run-id>"
$publicationRunAttempt = "<successful-tag-workflow-run-attempt>"
node scripts/release/release-publication-evidence.mjs resolve `
  --repository $repository `
  --tag $releaseTag `
  --source-commit $sourceCommit `
  --workflow-run-id $publicationRunId `
  --workflow-run-attempt $publicationRunAttempt `
  --output-directory $publicationResolutionDirectory
if ($LASTEXITCODE -ne 0) { throw "publication final authority resolution failed" }

$releasePublicationReceiptPath = Join-Path `
  $publicationResolutionDirectory `
  "clearra-release-publication-receipt.v1.json"
$releasePublicationEvidencePath = Join-Path `
  $publicationResolutionDirectory `
  "clearra-release-publication-evidence.v1.json"
$releasePublicationFinalAuthorityPath = Join-Path `
  $publicationResolutionDirectory `
  "clearra-release-publication-final-authority.v1.json"
$releasePublicationReceiptFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $releasePublicationReceiptPath
).Hash.ToLowerInvariant()
$releasePublicationEvidenceFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $releasePublicationEvidencePath
).Hash.ToLowerInvariant()
$releasePublicationFinalAuthorityFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $releasePublicationFinalAuthorityPath
).Hash.ToLowerInvariant()
$canonicalAcceptanceEvidenceFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $canonicalAcceptanceEvidencePath
).Hash.ToLowerInvariant()

node scripts/release/final-source-stage-evidence.mjs publication `
  --expected-source-commit $sourceCommit `
  --release-publication-evidence $releasePublicationEvidencePath `
  --release-publication-evidence-file-sha256 $releasePublicationEvidenceFileSha256 `
  --release-publication-final-authority $releasePublicationFinalAuthorityPath `
  --release-publication-final-authority-file-sha256 $releasePublicationFinalAuthorityFileSha256 `
  --release-publication-receipt $releasePublicationReceiptPath `
  --release-publication-receipt-file-sha256 $releasePublicationReceiptFileSha256 `
  --canonical-acceptance-evidence $canonicalAcceptanceEvidencePath `
  --output $publicationStageEvidencePath
if ($LASTEXITCODE -ne 0) { throw "publication stage evidence failed" }
$publicationStageEvidenceFileSha256 = (
  Get-FileHash -Algorithm SHA256 -LiteralPath $publicationStageEvidencePath
).Hash.ToLowerInvariant()

node scripts/release/final-source-attempt-journal.mjs append-stage `
  --journal $attemptJournal `
  --stage-evidence $publicationStageEvidencePath `
  --stage-evidence-file-sha256 $publicationStageEvidenceFileSha256
if ($LASTEXITCODE -ne 0) { throw "publication stage journal append failed" }

node scripts/release/final-source-attempt-journal.mjs materialize `
  --journal $attemptJournal `
  --output $finalManifest `
  --source-root $sourceRoot `
  --acceptance-stage-evidence $acceptanceStageEvidencePath `
  --acceptance-stage-evidence-file-sha256 $acceptanceStageEvidenceFileSha256 `
  --deployment-stage-evidence $deploymentStageEvidencePath `
  --deployment-stage-evidence-file-sha256 $deploymentStageEvidenceFileSha256 `
  --publication-stage-evidence $publicationStageEvidencePath `
  --publication-stage-evidence-file-sha256 $publicationStageEvidenceFileSha256 `
  --canonical-acceptance-evidence $canonicalAcceptanceEvidencePath `
  --canonical-acceptance-evidence-file-sha256 $canonicalAcceptanceEvidenceFileSha256 `
  --pages-deployment-authority $pagesDeploymentAuthorityPath `
  --pages-deployment-authority-file-sha256 $pagesDeploymentAuthorityFileSha256 `
  --pages-rollback-capture $pagesRollbackCapturePath `
  --pages-rollback-capture-file-sha256 $pagesRollbackCaptureFileSha256 `
  --discord-catalog $catalogPath `
  --discord-catalog-file-sha256 $catalogFileSha256 `
  --discord-prior-snapshot $priorCatalogPath `
  --discord-prior-snapshot-file-sha256 $priorCatalogFileSha256 `
  --discord-command-sync-authority $syncAuthorityPath `
  --discord-command-sync-authority-file-sha256 $syncAuthorityFileSha256 `
  --discord-catalog-sync-report $syncReportPath `
  --discord-catalog-sync-report-file-sha256 $syncReportFileSha256 `
  --cloud-candidate-smoke-report $candidateSmokeReportPath `
  --cloud-candidate-smoke-report-file-sha256 $candidateSmokeReportFileSha256 `
  --oracle-rollback-capture $oracleRollbackCaptureEvidencePath `
  --oracle-rollback-capture-file-sha256 $oracleRollbackCaptureEvidenceFileSha256 `
  --oracle-observation $oracleObservationEvidencePath `
  --oracle-observation-file-sha256 $oracleObservationEvidenceFileSha256 `
  --production-probe-spec $probeSpecPath `
  --production-probe-spec-file-sha256 $probeSpecFileSha256 `
  --production-observation-report $observationReportPath `
  --production-observation-report-file-sha256 $observationReportFileSha256 `
  --release-publication-evidence $releasePublicationEvidencePath `
  --release-publication-evidence-file-sha256 $releasePublicationEvidenceFileSha256 `
  --release-publication-final-authority $releasePublicationFinalAuthorityPath `
  --release-publication-final-authority-file-sha256 $releasePublicationFinalAuthorityFileSha256 `
  --release-publication-receipt $releasePublicationReceiptPath `
  --release-publication-receipt-file-sha256 $releasePublicationReceiptFileSha256
if ($LASTEXITCODE -ne 0) { throw "final-source materialization failed" }
```

`materialize` reopens every original producer, verifies every supplied raw-file
SHA-256, reconstructs all three stage reports, compares their event bytes with
the journal, and invokes the final validator as a library before it creates the
manifest. `validate-final-source-revalidation.mjs` is intentionally not an
operator CLI; direct execution is non-authoritative and must fail closed.

## Compatibility negative test

The pinned v0.5.1 build is retained as an explicit negative compatibility test:

```powershell
$projectId = gcloud config get-value project
$image = "asia-northeast1-docker.pkg.dev/$projectId/clearra/clearra-job:v0.5.1"
$buildServiceAccount = "projects/$projectId/serviceAccounts/clearra-build@$projectId.iam.gserviceaccount.com"
$cliSha256 = "<verified-64-character-release-asset-sha256>"
$sourceCommit = "<full-40-character-wrapper-source-commit>"
$engineBuildId = "<full-40-character-v0.5.1-engine-commit>"
$archiveRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("clearra-legacy-job-" + [Guid]::NewGuid().ToString("N"))
$archivePath = Join-Path $archiveRoot "source.tar.gz"
$configContext = Join-Path $archiveRoot "config"

if ($sourceCommit -cnotmatch '^[0-9a-f]{40}$') {
  throw "sourceCommit must be the full lowercase wrapper commit SHA"
}
$resolvedCommit = (git rev-parse "$sourceCommit`^{commit}").Trim()
if ($LASTEXITCODE -ne 0 -or $resolvedCommit -cne $sourceCommit) {
  throw "sourceCommit does not resolve exactly to the wrapper commit"
}

try {
  New-Item -ItemType Directory -Path $configContext -Force | Out-Null
  node scripts/release/create-exact-source-archive.mjs `
    --source-commit $sourceCommit `
    --output $archivePath
  if ($LASTEXITCODE -ne 0) { throw "exact source archive failed" }
  tar -xzf $archivePath -C $configContext `
    apps/clearra-discord-bot/cloudbuild-job-service.yaml
  if ($LASTEXITCODE -ne 0) { throw "legacy build config extraction failed" }

  $buildConfig = Join-Path $configContext "apps/clearra-discord-bot/cloudbuild-job-service.yaml"
  gcloud builds submit `
    --project=$projectId `
    --region=asia-northeast1 `
    --service-account=$buildServiceAccount `
    --config=$buildConfig `
    --substitutions="_IMAGE=$image,_CLEARRA_VERSION=0.5.1,_CLEARRA_CLI_SHA256=$cliSha256,_SOURCE_COMMIT=$sourceCommit,_ENGINE_BUILD_ID=$engineBuildId" `
    $archivePath
  if ($LASTEXITCODE -ne 0) { throw "legacy Cloud Build submission failed" }
} finally {
  if (Test-Path -LiteralPath $archiveRoot) {
    Remove-Item -LiteralPath $archiveRoot -Recurse -Force
  }
}
```

This build must first verify the release asset digest and then fail at the
finesse capability probes, so it must not publish an image. A later released
CLI may use this Docker path only after both probes pass and only under a
separate, truthful contract revision. Rebuilding a release image does not make
it current source; never repoint the Oracle production URL to an unverified
compatibility artifact.

## Health, cutover, and rollback

Before Oracle cutover, verify that `/health.runtime` exactly matches all five
identity fields, then submit one bounded authenticated smoke job without
printing request headers or results containing sensitive input. Confirm the
candidate revision's resource and scale annotations as well as the application-
level one-job limit.

The v2 identity wire is intentionally fail-closed and is not compatible with
the prior three-field caller. Preserve service availability with this order:
deploy and validate the tagged zero-traffic candidate; atomically activate the
new Oracle release against that tagged candidate URL with all five expected
fields; verify Gateway readiness and one bounded end-to-end job; only then move
the default Cloud service traffic to the candidate. Keep Oracle on the exact
tagged URL through command synchronization and the pre-sync rollback window;
stable-URL rebinding is forbidden until global command synchronization has
succeeded and the rollback recipe above is no longer authorized. Only then may
the stable service URL be selected after its `/health.runtime` identity is
rechecked. A
pre-traffic Oracle failure restores the old Oracle while the old Cloud revision
still serves 100 percent. Keep both prior releases available for rollback. The
Discord application's Interactions Endpoint remains empty throughout. Do not
combine rollback with Secret rotation or deletion.

GIF requests are not part of `clearra.job.v1`. Oracle retains the bounded
worker-thread renderer and the render/search delivery race; moving it here would
mix Discord media behavior with the CPU tier and is not approved by the current
evidence.
