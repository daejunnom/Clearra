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
`--cpu=8`: Cloud Run startup CPU boost can temporarily make the host report more
logical processors than the steady-state Linux affinity ceiling, so deriving
either authority from startup visibility is not release-safe. The expected-vCPU
setting binds Node's partition and Rust's resource lease to the deployed CPU
limit; the native runtime still validates that ceiling before creating workers.
`CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1` is an explicit native execution
authority: the job service must preserve it as `--use-all-cpu-threads` even when
startup boost makes Node see nine processors, while the separate numeric worker
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
$tag = "source-$sourceCommit"
$image = "asia-northeast1-docker.pkg.dev/$projectId/clearra/clearra-current-job:$tag"
$serviceName = "clearra-current-job"
$revisionSuffix = "v080-" + $sourceCommit.Substring(0, 7)
$candidateTag = "candidate-" + $sourceCommit.Substring(0, 7)
$runtimeServiceAccount = "clearra-current-job@$projectId.iam.gserviceaccount.com"
$jobBearerSecret = "clearra-job-token"
$priorRuntimeAuthorityKind = 'clearra.rollback.legacy-health-no-runtime.v1'
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
  --operation capture-rollback-authority `
  --script-release-id $oracleCandidateReleaseId `
  --script-release-sha256 $oracleCandidateReleaseSha256 `
  --prior-revision $priorRevision `
  --prior-runtime-authority-kind $priorRuntimeAuthorityKind `
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
  --set-env-vars="CLEARRA_EXECUTABLE=/usr/local/bin/clearra,CLEARRA_SEARCH_TIMEOUT_MS=170000,CLEARRA_REVERSE_SEARCH_TIMEOUT_MS=300000,CLEARRA_FORWARD_SEARCH_TIMEOUT_MS=900000,CLEARRA_EXPECTED_VCPUS=8,CLEARRA_SEARCH_WORKERS_PER_SESSION=8,CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1,CLEARRA_MAX_CONCURRENT_JOBS=1,CLEARRA_MAX_OUTPUT_BYTES=4194304"
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
    --prior-runtime-authority-kind $priorRuntimeAuthorityKind `
    --prior-runtime-authority-sha256 $priorRuntimeAuthoritySha256 `
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
  --prior-runtime-authority-kind $priorRuntimeAuthorityKind `
  --prior-runtime-authority-sha256 $priorRuntimeAuthoritySha256 `
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
commit-byte archive context of the same accepted commit as the healthy
Oracle/runtime release. It must use the canonical archive helper and submit the
verified `.tar.gz` itself, not inherit the operator's Git line-ending settings,
use a checkout directory, reuse a prior extracted context, or submit `.`.
Perform this only after the runtime identity and bounded smoke job checks have
passed:

```powershell
$projectId = gcloud config get-value project
$sourceCommit = "<same-full-40-character-accepted-commit>"
$repository = "daejunnom/Clearra"
$serviceName = "clearra-current-job"
$archiveRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("clearra-command-sync-" + [Guid]::NewGuid().ToString("N"))
$archivePath = Join-Path $archiveRoot "source.tar.gz"
$configContext = Join-Path $archiveRoot "config"

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
  New-Item -ItemType Directory -Path $configContext -Force | Out-Null
  node scripts/release/create-exact-source-archive.mjs `
    --source-commit $sourceCommit `
    --output $archivePath
  if ($LASTEXITCODE -ne 0) { throw "exact source archive failed" }
  tar -xzf $archivePath -C $configContext `
    apps/clearra-discord-bot/cloudbuild-command-sync.yaml
  if ($LASTEXITCODE -ne 0) { throw "command-sync config extraction failed" }

  $syncConfig = Join-Path $configContext "apps/clearra-discord-bot/cloudbuild-command-sync.yaml"
  gcloud builds submit `
    --project=$projectId `
    --region=asia-northeast1 `
    --config=$syncConfig `
    $archivePath
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
