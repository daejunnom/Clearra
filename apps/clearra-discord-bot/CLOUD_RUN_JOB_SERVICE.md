# Clearra Cloud Run job service

`Dockerfile.job-service` packages the released Linux Clearra CLI with a small
HTTP implementation of `clearra.job.v1`. The Oracle-hosted Clearrabot keeps the
Discord Gateway connection and submits only validated argument arrays. The job
service derives `--auto-workers` from the CPU limit visible inside Cloud Run,
so the Oracle host CPU count does not cap Clearra search parallelism.

## Build and push

From the repository root, build directly or use the supplied Cloud Build file:

```bash
PROJECT_ID="$(gcloud config get-value project)"
IMAGE="us-central1-docker.pkg.dev/${PROJECT_ID}/clearra/clearra-job:v0.5.1"

gcloud builds submit \
  --region=us-central1 \
  --config=apps/clearra-discord-bot/cloudbuild-job-service.yaml \
  --substitutions=_IMAGE="${IMAGE}",_CLEARRA_VERSION=0.5.1 \
  .
```

`CLEARRA_CLI_SHA256` can be supplied as a Docker build argument when a release
checksum is available. The build runs a CLI smoke check before producing the
runtime image.

## Cloud Run service

Recommended beta settings:

```text
CPU:                    6 vCPU
Memory:                 24 GiB
Request concurrency:    1
Minimum instances:      0
Maximum instances:      4
Request timeout:        240 seconds
Startup CPU boost:      enabled
Billing:                request-based
Container port:         8080
```

The service intentionally keeps `POST /jobs` open until Clearra finishes. This
keeps request-based CPU allocation active, avoids instance-local polling state,
and lets the existing `ClearraJobExecutor` consume an immediate terminal
`clearra.job.v1` response. `GET /jobs/:id` and `DELETE /jobs/:id` remain
best-effort compatibility endpoints for requests routed to the same instance.

Set these runtime values:

```text
CLEARRA_EXECUTABLE=/usr/local/bin/clearra
CLEARRA_JOB_TOKEN=<shared opaque bearer token>
CLEARRA_SEARCH_TIMEOUT_MS=170000
CLEARRA_SEARCH_WORKERS_PER_SESSION=auto
CLEARRA_USE_ALL_LOGICAL_PROCESSORS=0
CLEARRA_MAX_CONCURRENT_SEARCHES=1
CLEARRA_MAX_OUTPUT_BYTES=4194304
```

The public Cloud Run URL must still require the application bearer token at the
job-service layer. `/healthz` is the only unauthenticated endpoint. For local
smoke tests only, `CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED=1` bypasses the
bearer check.

## Oracle Gateway settings

```text
CLEARRA_JOB_URL=https://<cloud-run-service>/jobs
CLEARRA_JOB_TOKEN=<same opaque bearer token>
CLEARRA_WORKER_AUTHORITY=remote
CLEARRA_MAX_CONCURRENT_REMOTE_JOBS=4
CLEARRA_SEARCH_TIMEOUT_MS=180000
```

When `CLEARRA_JOB_URL` is explicitly configured, `remote` is the default worker
authority. The Gateway no longer injects a worker limit derived from the Oracle
machine. Set `CLEARRA_WORKER_AUTHORITY=gateway` only when the HTTP job service
runs on the same CPU allocation and the older host-owned worker policy is
intentional.

## Endpoints

```text
GET    /healthz
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

The service re-applies the Discord command policy, strips caller-controlled
worker and output switches, and injects the local Cloud Run worker ceiling
before starting Clearra with `shell: false`.

## Result delivery boundary

This service returns bounded stdout and stderr through `clearra.job.v1`; the
Oracle Gateway still edits the deferred Discord interaction. Direct Cloud Run
to Discord attachment delivery is a separate protocol extension and is not
implicitly enabled by this container.
