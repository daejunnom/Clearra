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

## Build the current-source image

Build in Tokyo and use an immutable source-revision tag. The build context must
be a temporary archive of the exact commit that passed canonical acceptance.
Never submit the working tree (`gcloud builds submit ... .`): tracked dirty
changes and untracked files are outside the approved source identity.

```powershell
$projectId = gcloud config get-value project
$sourceCommit = "<full-40-character-git-commit>"
$repository = "daejunnom/Clearra"
$tag = "source-$sourceCommit"
$buildServiceAccount = "projects/$projectId/serviceAccounts/clearra-build@$projectId.iam.gserviceaccount.com"
$archiveRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("clearra-current-job-" + [Guid]::NewGuid().ToString("N"))
$archivePath = Join-Path $archiveRoot "source.tar"
$archiveContext = Join-Path $archiveRoot "context"

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

try {
  New-Item -ItemType Directory -Path $archiveContext -Force | Out-Null
  git archive --format=tar --output=$archivePath $sourceCommit
  if ($LASTEXITCODE -ne 0) { throw "git archive failed" }
  tar -xf $archivePath -C $archiveContext
  if ($LASTEXITCODE -ne 0) { throw "archive extraction failed" }

  $buildConfig = Join-Path $archiveContext "apps/clearra-discord-bot/cloudbuild-current-job-service.yaml"
  gcloud builds submit `
    --project=$projectId `
    --region=asia-northeast1 `
    --service-account=$buildServiceAccount `
    --config=$buildConfig `
    --substitutions="_REGION=asia-northeast1,_REPOSITORY=clearra,_IMAGE_NAME=clearra-current-job,_TAG=$tag,_SOURCE_COMMIT=$sourceCommit" `
    $archiveContext
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
CLEARRA_SEARCH_WORKERS_PER_SESSION=auto
CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1
CLEARRA_MAX_CONCURRENT_JOBS=1
CLEARRA_MAX_OUTPUT_BYTES=4194304
```

One automatic job receives all eight logical processors visible in the
container. The native runtime still validates the Linux processor-affinity hard
ceiling before creating workers. Set
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

`CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED=1` is restricted to a loopback
listener for local smoke tests. It is not a Cloud Run setting.

## Deployment template

Use the immutable image tag produced above and bind the existing managed Secret
by name. This template intentionally sets both service-level and revision-level
minimum/maximum values:

```powershell
$projectId = gcloud config get-value project
$sourceCommit = "<same-full-40-character-accepted-commit>"
$tag = "source-$sourceCommit"
$image = "asia-northeast1-docker.pkg.dev/$projectId/clearra/clearra-current-job:$tag"
$serviceName = "clearra-current-job"
$revisionSuffix = "v075-" + $sourceCommit.Substring(0, 7)
$candidateTag = "candidate-" + $sourceCommit.Substring(0, 7)
$runtimeServiceAccount = "clearra-current-job@$projectId.iam.gserviceaccount.com"
$jobBearerSecret = "<Google Secret Manager job-bearer Secret name>"
$oracleCandidateReleaseId = "<immutable-candidate-Oracle-release-ID>"
$oracleCandidateReleaseSha256 = "<candidate-Oracle-release-tree-SHA-256>"
$oracleCandidateSettingsSha256 = "<candidate-non-secret-settings-SHA-256>"
$oracleRemoteWrapper = "<approved authenticated Oracle remote wrapper>"
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
# Before every operation, the trusted wrapper independently computes the exact
# script-release tree digest (without executing candidate-supplied code), rejects
# dangling or out-of-tree links, and compares it with `--script-release-sha256`.

if ($sourceCommit -cnotmatch '^[0-9a-f]{40}$') {
  throw "sourceCommit must be the full lowercase accepted commit SHA"
}
$serviceBefore = gcloud run services describe $serviceName `
  --project=$projectId --region=asia-northeast1 --format=json | ConvertFrom-Json
$priorTraffic = @($serviceBefore.status.traffic | Where-Object { [int]$_.percent -eq 100 })
if ($priorTraffic.Count -ne 1 -or -not $priorTraffic[0].revisionName) {
  throw "Cloud Run must have exactly one prior 100-percent revision before candidate deployment"
}
$priorRevision = [string]$priorTraffic[0].revisionName

# The authenticated private wrapper runs the tracked capture helper as root on
# Oracle before either Cloud or Oracle mutation. It returns only non-secret
# release/settings/runtime digests and the root-owned settings backup path.
$priorCaptureJson = & $oracleRemoteWrapper `
  --operation capture-rollback-authority `
  --script-release-id $oracleCandidateReleaseId `
  --script-release-sha256 $oracleCandidateReleaseSha256 `
  --prior-revision $priorRevision `
  --deployment-nonce $deploymentNonce
if ($LASTEXITCODE -ne 0) { throw "prior Oracle rollback authority capture failed" }
try { $priorCapture = $priorCaptureJson | ConvertFrom-Json } catch {
  throw "prior Oracle rollback authority capture returned invalid JSON"
}
if ($priorCapture.priorRevision -cne $priorRevision -or
    $priorCapture.deploymentNonce -cne $deploymentNonce -or
    $priorCapture.priorOracleReleaseId -cnotmatch '^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$' -or
    $priorCapture.priorOracleRelease -cne "/opt/clearra/releases/$($priorCapture.priorOracleReleaseId)" -or
    $priorCapture.priorOracleReleaseSha256 -cnotmatch '^[0-9a-f]{64}$' -or
    $priorCapture.priorOracleSettingsBackup -cnotmatch '^/etc/clearra-gateway/settings\.pre-v0\.7\.5-[0-9a-f]{64}$' -or
    $priorCapture.priorOracleSettingsSha256 -cnotmatch '^[0-9a-f]{64}$' -or
    $priorCapture.priorRuntimeIdentitySha256 -cnotmatch '^[0-9a-f]{64}$' -or
    $priorCapture.priorJobUrl -cnotmatch '^https://[^/]+/jobs$') {
  throw "prior Oracle rollback authority capture is incomplete"
}
$priorOracleRelease = [string]$priorCapture.priorOracleRelease
$priorOracleReleaseId = [string]$priorCapture.priorOracleReleaseId
$priorOracleReleaseSha256 = [string]$priorCapture.priorOracleReleaseSha256
$priorOracleSettingsBackup = [string]$priorCapture.priorOracleSettingsBackup
$priorOracleSettingsSha256 = [string]$priorCapture.priorOracleSettingsSha256
$priorRuntimeIdentitySha256 = [string]$priorCapture.priorRuntimeIdentitySha256
$priorJobUrl = [string]$priorCapture.priorJobUrl

gcloud run deploy $serviceName `
  --project=$projectId `
  --region=asia-northeast1 `
  --image=$image `
  --revision-suffix=$revisionSuffix `
  --tag=$candidateTag `
  --no-traffic `
  --service-account=$runtimeServiceAccount `
  --ingress=all `
  --no-invoker-iam-check `
  --port=8080 `
  --concurrency=1 `
  --min=0 `
  --min-instances=0 `
  --max=4 `
  --max-instances=4 `
  --cpu=8 `
  --memory=16Gi `
  --no-cpu-throttling `
  --cpu-boost `
  --timeout=900s `
  --set-secrets="CLEARRA_JOB_TOKEN=${jobBearerSecret}:latest" `
  --set-env-vars="CLEARRA_EXECUTABLE=/usr/local/bin/clearra,CLEARRA_SEARCH_TIMEOUT_MS=170000,CLEARRA_REVERSE_SEARCH_TIMEOUT_MS=300000,CLEARRA_FORWARD_SEARCH_TIMEOUT_MS=900000,CLEARRA_SEARCH_WORKERS_PER_SESSION=auto,CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1,CLEARRA_MAX_CONCURRENT_JOBS=1,CLEARRA_MAX_OUTPUT_BYTES=4194304"
if ($LASTEXITCODE -ne 0) { throw "no-traffic candidate deployment failed" }

$candidateRevision = "$serviceName-$revisionSuffix"
$serviceCandidate = gcloud run services describe $serviceName `
  --project=$projectId --region=asia-northeast1 --format=json | ConvertFrom-Json
$candidateTraffic = @($serviceCandidate.status.traffic | Where-Object {
  $_.tag -eq $candidateTag -and $_.revisionName -eq $candidateRevision
})
$stillActive = @($serviceCandidate.status.traffic | Where-Object { [int]$_.percent -eq 100 })
if ($serviceCandidate.status.latestCreatedRevisionName -ne $candidateRevision -or
    $candidateTraffic.Count -ne 1 -or -not $candidateTraffic[0].url -or
    $stillActive.Count -ne 1 -or $stillActive[0].revisionName -ne $priorRevision) {
  throw "candidate identity or zero-traffic isolation check failed"
}
$candidateUrl = [string]$candidateTraffic[0].url
$health = Invoke-RestMethod -Method Get -Uri "$candidateUrl/health"
if ($health.runtime.sourceCommit -cne $sourceCommit -or
    $health.runtime.engineBuildId -cne $sourceCommit -or
    $health.runtime.contractSchemaVersion -cne 'clearra.search.contract.v2' -or
    $health.runtime.supplySemanticsId -cne 'clearra.supply.projected-terminal-lookahead.v1' -or
    $health.runtime.artifactSchemaVersion -cne 'clearra.solution-data.v1') {
  throw "candidate health runtime identity mismatch"
}

# Have the approved secret wrapper inject CLEARRA_CANDIDATE_JOB_TOKEN into this
# process environment only. The verifier uses the production executor to send
# one bounded authenticated request to "$candidateUrl/jobs", checks the exact
# source/engine/contract identity and result shape, and never prints the bearer,
# request body, or result.
if ([string]::IsNullOrWhiteSpace($env:CLEARRA_CANDIDATE_JOB_TOKEN)) {
  throw "candidate job bearer was not injected by the managed secret wrapper"
}
try {
  node apps/clearra-discord-bot/scripts/verify-cloud-run-candidate.mjs `
    --base-url $candidateUrl `
    --source-commit $sourceCommit
  if ($LASTEXITCODE -ne 0) { throw "candidate authenticated job smoke failed" }
} finally {
  Remove-Item Env:CLEARRA_CANDIDATE_JOB_TOKEN -ErrorAction SilentlyContinue
}

# Confirm Ready/startup/error logs, resource/spec parity, and an unchanged IAM
# policy. This is an authenticated remote boundary, not a local `sudo`: the
# approved wrapper connects to Oracle without printing credentials, atomically
# activates the immutable candidate release/settings, waits for a real `/path`,
# and invokes the tracked candidate producer and one-shot consumer as root. The
# producer independently observes the active symlink/tree digest, settings
# digest and all five expected settings, current PID/cwd, READY record, and a
# fresh successful Gateway operation newer than `$deploymentVerifiedAfter`.
& $oracleRemoteWrapper `
  --operation verify-candidate `
  --script-release-id $oracleCandidateReleaseId `
  --script-release-sha256 $oracleCandidateReleaseSha256 `
  --proof $oracleCandidateProofPath `
  --source-commit $sourceCommit `
  --candidate-url $candidateUrl `
  --candidate-revision $candidateRevision `
  --oracle-release-id $oracleCandidateReleaseId `
  --oracle-release-sha256 $oracleCandidateReleaseSha256 `
  --oracle-settings-sha256 $oracleCandidateSettingsSha256 `
  --deployment-nonce $deploymentNonce `
  --verified-after $deploymentVerifiedAfter
$candidateOracleExit = $LASTEXITCODE
if ($candidateOracleExit -ne 0) {
  # Cloud still serves the prior revision. Restore and freshly verify the exact
  # captured Oracle authority before aborting this deployment.
  $preCutoverRollbackVerifiedAfter = [DateTime]::UtcNow.ToString("o")
  & $oracleRemoteWrapper `
    --operation restore-prior-and-verify `
    --script-release-id $oracleCandidateReleaseId `
    --script-release-sha256 $oracleCandidateReleaseSha256 `
    --prior-release $priorOracleRelease `
    --prior-release-id $priorOracleReleaseId `
    --prior-release-sha256 $priorOracleReleaseSha256 `
    --prior-settings-backup $priorOracleSettingsBackup `
    --prior-settings-sha256 $priorOracleSettingsSha256 `
    --prior-runtime-identity-sha256 $priorRuntimeIdentitySha256 `
    --prior-job-url $priorJobUrl `
    --prior-revision $priorRevision `
    --proof $oracleRollbackProofPath `
    --deployment-nonce $deploymentNonce `
    --verified-after $preCutoverRollbackVerifiedAfter
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
separately and verify concurrency 1, min 0, max 4, 8 vCPU, and 16 GiB before
the explicit traffic mutation. A candidate failure leaves `$priorRevision`
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
  --operation restore-prior-and-verify `
  --script-release-id $oracleCandidateReleaseId `
  --script-release-sha256 $oracleCandidateReleaseSha256 `
  --prior-release $priorOracleRelease `
  --prior-release-id $priorOracleReleaseId `
  --prior-release-sha256 $priorOracleReleaseSha256 `
  --prior-settings-backup $priorOracleSettingsBackup `
  --prior-settings-sha256 $priorOracleSettingsSha256 `
  --prior-runtime-identity-sha256 $priorRuntimeIdentitySha256 `
  --prior-job-url $priorJobUrl `
  --prior-revision $priorRevision `
  --proof $oracleRollbackProofPath `
  --deployment-nonce $deploymentNonce `
  --verified-after $rollbackVerifiedAfter
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

Command synchronization is a release mutation and must use a fresh temporary
`git archive` context of the same accepted commit as the healthy Oracle/runtime
release. It must not use a checkout directory, reuse a prior extracted context,
or submit `.`. Perform this only after the runtime identity and bounded smoke
job checks have passed:

```powershell
$projectId = gcloud config get-value project
$sourceCommit = "<same-full-40-character-accepted-commit>"
$repository = "daejunnom/Clearra"
$serviceName = "clearra-current-job"
$archiveRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("clearra-command-sync-" + [Guid]::NewGuid().ToString("N"))
$archivePath = Join-Path $archiveRoot "source.tar"
$archiveContext = Join-Path $archiveRoot "context"

if ($sourceCommit -cnotmatch '^[0-9a-f]{40}$') {
  throw "sourceCommit must be the full lowercase accepted commit SHA"
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
node apps/clearra-discord-bot/scripts/verify-accepted-source.mjs `
  --source-commit $sourceCommit `
  --repository $repository `
  --active-health-url ([string]$activeService.status.url)
if ($LASTEXITCODE -ne 0) { throw "accepted runtime preflight failed before command sync" }

try {
  New-Item -ItemType Directory -Path $archiveContext -Force | Out-Null
  git archive --format=tar --output=$archivePath $sourceCommit
  if ($LASTEXITCODE -ne 0) { throw "git archive failed" }
  tar -xf $archivePath -C $archiveContext
  if ($LASTEXITCODE -ne 0) { throw "archive extraction failed" }

  $syncConfig = Join-Path $archiveContext "apps/clearra-discord-bot/cloudbuild-command-sync.yaml"
  gcloud builds submit `
    --project=$projectId `
    --region=asia-northeast1 `
    --config=$syncConfig `
    $archiveContext
  if ($LASTEXITCODE -ne 0) { throw "command-sync Cloud Build submission failed" }
} finally {
  if (Test-Path -LiteralPath $archiveRoot) {
    Remove-Item -LiteralPath $archiveRoot -Recurse -Force
  }
}
```

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
$archivePath = Join-Path $archiveRoot "source.tar"
$archiveContext = Join-Path $archiveRoot "context"

if ($sourceCommit -cnotmatch '^[0-9a-f]{40}$') {
  throw "sourceCommit must be the full lowercase wrapper commit SHA"
}
$resolvedCommit = (git rev-parse "$sourceCommit`^{commit}").Trim()
if ($LASTEXITCODE -ne 0 -or $resolvedCommit -cne $sourceCommit) {
  throw "sourceCommit does not resolve exactly to the wrapper commit"
}

try {
  New-Item -ItemType Directory -Path $archiveContext -Force | Out-Null
  git archive --format=tar --output=$archivePath $sourceCommit
  if ($LASTEXITCODE -ne 0) { throw "git archive failed" }
  tar -xf $archivePath -C $archiveContext
  if ($LASTEXITCODE -ne 0) { throw "archive extraction failed" }

  $buildConfig = Join-Path $archiveContext "apps/clearra-discord-bot/cloudbuild-job-service.yaml"
  gcloud builds submit `
    --project=$projectId `
    --region=asia-northeast1 `
    --service-account=$buildServiceAccount `
    --config=$buildConfig `
    --substitutions="_IMAGE=$image,_CLEARRA_VERSION=0.5.1,_CLEARRA_CLI_SHA256=$cliSha256,_SOURCE_COMMIT=$sourceCommit,_ENGINE_BUILD_ID=$engineBuildId" `
    $archiveContext
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
