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
  checkout. `clearra-current-job` must use this image so post-v0.5.1 fixes,
  current command syntax, CTK3 output, and worker policy remain aligned with
  Oracle.
- `Dockerfile.job-service` and `cloudbuild-job-service.yaml` package the
  released v0.5.1 Linux CLI. That immutable artifact is compatibility-test only
  and must not receive active Oracle traffic. Its build now intentionally fails
  the required finesse capability gate, so it cannot become a healthy service
  revision by mistake.

The retired Discord interaction image is not a job-service artifact and must not
receive active traffic. Discord interactions are owned by Oracle Gateway.

## Build the current-source image

Build in Tokyo and use an immutable source-revision tag:

```powershell
$projectId = gcloud config get-value project
$tag = "source-<git-commit>"
$buildServiceAccount = "projects/$projectId/serviceAccounts/clearra-build@$projectId.iam.gserviceaccount.com"

gcloud builds submit `
  --project=$projectId `
  --region=asia-northeast1 `
  --service-account=$buildServiceAccount `
  --config=apps/clearra-discord-bot/cloudbuild-current-job-service.yaml `
  --substitutions="_REGION=asia-northeast1,_REPOSITORY=clearra,_IMAGE_NAME=clearra-current-job,_TAG=$tag" `
  .
```

The Docker build compiles `clearra-cli` in release mode with the current feature
contract and executes tiny `finesse search` and `finesse score` JSON probes. The
service repeats those probes before opening its listen port. Its final stage
contains the CLI, job-service and command-policy sources, production Node
dependencies, and built CTK3 package. Secrets are runtime bindings, never build
arguments or image contents.

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
$tag = "source-<git-commit>"
$image = "asia-northeast1-docker.pkg.dev/$projectId/clearra/clearra-current-job:$tag"
$runtimeServiceAccount = "clearra-current-job@$projectId.iam.gserviceaccount.com"
$jobBearerSecret = "<Google Secret Manager job-bearer Secret name>"

gcloud run deploy clearra-current-job `
  --project=$projectId `
  --region=asia-northeast1 `
  --image=$image `
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
```

Both pairs are required. Setting only `--max=4` can leave the active revision's
`maxScale` at its lower default, making three—not four—the effective cap.
Likewise, set both minimum flags to make scale-to-zero explicit at both levels.
After every deploy, inspect the service and active revision separately and
verify concurrency 1, min 0, max 4, 8 vCPU, and 16 GiB before cutover.

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
CLEARRA_JOB_URL=https://<clearra-current-job service>/jobs
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

## Compatibility negative test

The pinned v0.5.1 build is retained as an explicit negative compatibility test:

```powershell
$projectId = gcloud config get-value project
$image = "asia-northeast1-docker.pkg.dev/$projectId/clearra/clearra-job:v0.5.1"
$buildServiceAccount = "projects/$projectId/serviceAccounts/clearra-build@$projectId.iam.gserviceaccount.com"

gcloud builds submit `
  --project=$projectId `
  --region=asia-northeast1 `
  --service-account=$buildServiceAccount `
  --config=apps/clearra-discord-bot/cloudbuild-job-service.yaml `
  --substitutions="_IMAGE=$image,_CLEARRA_VERSION=0.5.1" `
  .
```

This build must fail at the finesse capability probes and therefore must not
publish an image. A later released CLI may use this Docker path only after both
probes pass. Rebuilding a release image does not make it current source; never
repoint the Oracle production URL to an unverified compatibility artifact.

## Health, cutover, and rollback

Before Oracle cutover, verify `/health`, then submit one bounded authenticated
smoke job without printing request headers or results containing sensitive
input. Confirm the active revision's resource and scale annotations as well as
the application-level one-job limit.

Deploy and validate this job tier before switching an Oracle release to it. Keep
the previous `clearra-current-job` revision available; a compute rollback moves
this service's traffic to that prior healthy revision. The Discord application's
Interactions Endpoint remains empty throughout compute cutover and rollback. Do
not combine rollback with Secret rotation or deletion.

GIF requests are not part of `clearra.job.v1`. Oracle retains the bounded
worker-thread renderer and the render/search delivery race; moving it here would
mix Discord media behavior with the CPU tier and is not approved by the current
evidence.
