# Optional Clearra remote job service

This document describes the retained `clearra.job.v1` compatibility seam. It is
not the active Discord slash-command deployment. The active path is the single
Cloud Run interaction container described in [README.md](./README.md): Discord
calls that service directly, Clearra runs in-process there, and the same service
edits the deferred interaction.

`Dockerfile.job-service` intentionally packages the released v0.5.1 Linux CLI.
It does not contain post-v0.5.1 source changes and must not be used as the active
slash-command computation image. The interaction `Dockerfile` builds the current
checkout with the release feature contract.

The job-service code remains available for these future, explicit tests:

- ordinary text commands proxied by the Oracle Gateway;
- a separately tested Cloud Run interaction-to-job-service split;
- protocol compatibility with another bounded `clearra.job.v1` runner.

None of those paths is enabled by default. In particular, Oracle does not relay
or edit active slash-command interactions.

## Build and push for an explicit compatibility test

Use the selected Tokyo region, `asia-northeast1`:

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

`CLEARRA_CLI_SHA256` may be supplied as a Docker build argument when a trusted
release checksum is available. The build performs a CLI smoke check. This
artifact remains version-pinned by design; rebuilding it does not make it a
current-source image.

## Approved compatibility-test service shape

If the compatibility split is tested, use the same per-instance resource
boundary as the interaction service. Each instance remains serial, while Cloud
Run may route work to four instances:

```text
Region:                 asia-northeast1 (Tokyo)
Request concurrency:    1
Minimum instances:      0
Maximum instances:      4
CPU per instance:       8 vCPU
Memory per instance:    16 GiB
CPU allocation:         instance-based / no CPU throttling
Startup CPU boost:      enabled
Container port:         8080
```

Set these runtime values:

```text
CLEARRA_EXECUTABLE=/usr/local/bin/clearra
CLEARRA_JOB_TOKEN=<shared opaque bearer token>
CLEARRA_SEARCH_TIMEOUT_MS=170000
CLEARRA_SEARCH_WORKERS_PER_SESSION=auto
CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1
CLEARRA_MAX_CONCURRENT_JOBS=1
CLEARRA_MAX_OUTPUT_BYTES=4194304
```

One automatic job receives eight workers from the eight vCPUs visible inside its
container. At maximum scale, four instances can run four jobs with 32 workers in
aggregate. This is per-instance serial execution, not a global serial lock, and
each process owns its own retained job state. Set
`CLEARRA_USE_ALL_LOGICAL_PROCESSORS=0` to reserve one processor per instance.
The hard ceiling rejects a per-job allocation above the visible processor count.
Cloud Run's per-instance CPU maximum is eight vCPUs, so a single 16-vCPU job
instance is not available.

The current remote executor sends the application bearer token in
`Authorization`; it does not yet mint a Google identity token for a private
Cloud Run service. A Cloud Run compatibility deployment must therefore be
reachable at the platform invocation layer while the application token gates
`/jobs`. Do not document or deploy it as IAM-private until the caller adds a
separate Google ID-token header. `/health` is the only externally usable
unauthenticated application endpoint. `CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED=1` is
restricted to a loopback listener and is for local smoke tests only.

## Explicit caller settings

An Oracle text-command proxy or a deliberately split interaction service uses:

```text
CLEARRA_JOB_URL=https://<tested-service>/jobs
CLEARRA_JOB_TOKEN=<same opaque bearer token>
CLEARRA_WORKER_AUTHORITY=remote
CLEARRA_MAX_CONCURRENT_REMOTE_JOBS=4
CLEARRA_SEARCH_TIMEOUT_MS=180000
```

With `CLEARRA_JOB_URL` configured, the remote instance owns the worker count.
The Oracle machine's logical processor count must not cap Cloud Run. Setting
`CLEARRA_WORKER_AUTHORITY=gateway` is valid only when the HTTP runner deliberately
shares the caller's CPU allocation.

## Protocol

```text
GET    /health
POST   /jobs
GET    /jobs/:id
DELETE /jobs/:id
```

`POST /jobs` requires:

```text
Authorization: Bearer <CLEARRA_JOB_TOKEN>
Idempotency-Key: <same value as body.id>
Content-Type: application/json
```

The service re-applies the Discord command policy, removes caller-controlled
worker and output switches, injects its local worker ceiling, and starts Clearra
with `shell: false`. The request carries an absolute deadline and a bounded
combined stdout/stderr allowance.

The bundled implementation keeps `POST /jobs` open until Clearra finishes and
returns a terminal result. `GET /jobs/:id` and `DELETE /jobs/:id` are
best-effort compatibility endpoints for requests routed to the same instance.

## Result-delivery boundary

For a future Oracle text proxy, Oracle may receive the bounded result and send a
new Discord message. It must enforce a separate input-length limit and must not
become the active slash-command hop. For a deliberately split Cloud Run test,
the interaction service—not Oracle—continues to own the Discord deferred reply.

Renderer requests are not part of `clearra.job.v1`. A future bounded small-image
request may be owned by Oracle; large rendering can be considered for a separate
Cloud Run boundary only after load testing. No image command is currently
advertised.
