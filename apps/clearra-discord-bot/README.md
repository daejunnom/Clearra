# Clearrabot

Clearrabot exposes the represented Sfinder-compatible command contracts as
Discord slash commands. The active production path is one public Cloud Run
interaction service, autoscaled from zero to four instances, that verifies
Discord requests, runs the current Clearra CLI in the same container, and edits
the deferred Discord interaction directly.

## Active Discord surface

The registered commands are `/path`, `/percent`, `/chance`, `/minimals`,
`/score`, `/score-minimals`, `/saves`, `/best-save`, `/cover`, `/setup`,
`/congruent`, `/congruent-cover`, `/setup-cover`, `/cover-percent`,
`/special-cover`, `/spin-cover`, `/spin`, `/cat-finder`, `/pc-setup`,
`/best-setup`, `/dpc-finder`, `/verify`, and `/help`. `/help` accepts an
optional command name in `arguments`; without it, the command lists the active
groups. Search commands use structured primary inputs instead of one raw argv
string. For example:

```text
/path field:<CTK3 or Fumen> next:<pattern> options:"clear=4 hold=use"
/cover base:<CTK3 or Fumen> target:<CTK3 or Fumen> next:<pattern> options:"hold=use"
/spin-cover field:<CTK3 or Fumen> next:<pattern> options:"type=TSD"
/pc-setup remaining:<unordered piece inventory>
/verify scope:<pc|setup|cover|build|kicks>
```

Board options accept raw CTK3/Fumen or a URL containing one of those values.
Each value must decode to one operation-free, static, 10-column page. CTK3 is
read directly with the npm `ctk3` package and is never re-encoded as Fumen;
Fumen is decoded independently at the Discord boundary. For both formats every
non-empty color becomes the same occupancy bit. Ordinary field inputs use a
canonical Board64 mask. `/cover` accepts `base` plus a non-overlapping `target`
delta containing only cells to add, uses canonical 24-row masks, and compiles to
the existing build-probability request with `next`.

Search results expose solution documents only as CTK3. Generated pieces
preserve their tetromino colors, while occupancy inherited from the input board
is encoded as `G`. The active Discord result path does not emit Fumen. The
legacy raw CLI Fumen form of `clearra sfinder cover` remains available outside
the slash ingress and keeps its existing exact colored-solution boundary.
Optional `options` text is a command-specific, space-separated `key=value`
allow-list. It cannot replace primary board/`next` inputs or select workers,
files, custom profiles, or output formats.

There is no active `/clearra` catch-all command and no active `/view` command.
Prefix forms such as `$...` and `>...`, ordinary-message command detection,
automatic document viewing, and Gateway-delivered slash commands are disabled.
The compute ingress now uses CTK3/Fumen normalization, but the renderer and GIF
implementation remain dormant: no registered command can invoke image output.

The compatibility boundary owns worker, output-format, tablebase, and
dependency-DAG policy. It rejects native file/output paths, custom WGSL, custom
kick JSON, and Sfinder contracts without a typed Clearra representation. Every
accepted command reaches Clearra as an argument array through `shell: false`.
The occupancy projection and two-field `/cover` routing are ingress changes
only; PC/build engines and pruning are unchanged.

## Active topology

```text
Discord slash command
  -> public Cloud Run POST /interactions
  -> Discord Ed25519 verification and immediate deferred ACK
  -> bounded per-instance serial Clearra CLI execution in the same container
  -> Discord interaction webhook edit
```

The Oracle Gateway is not in the slash-command path. With no
`CLEARRA_JOB_URL`, Cloud Run uses `ClearraDirectExecutor` and the source-built
`/usr/local/bin/clearra`. Each instance executes one search at a time, with its
own bounded pending queue. Cloud Run may create up to four instances, so the
service is not globally serial: at full scale it can run four searches in
parallel, one per instance, without making two CPU-heavy searches compete for
the same instance's eight vCPUs.

`CLEARRA_JOB_URL` remains an explicit remote-execution seam for later testing;
it is not the Cloud Run default. The older `clearra.job.v1` service and Oracle
proxy boundary are documented in
[CLOUD_RUN_JOB_SERVICE.md](./CLOUD_RUN_JOB_SERVICE.md).

## Discord timing and lifecycle boundary

Discord requires the initial interaction response within three seconds. The
HTTP adapter therefore validates the signature and returns interaction response
type `5` (deferred channel message) before starting the search. The interaction
token remains usable for 15 minutes. Clearra caps
`CLEARRA_INTERACTION_DEADLINE_MS` at 14 minutes so completion and error edits do
not intentionally consume the full token window.

The default search limit is three minutes and the default total interaction
deadline is four minutes, including time spent in the local pending queue. The
queue defaults to eight waiting searches per instance; at four instances there
can therefore be up to four active searches and 32 instance-local queued
searches. Cloud Run routing does not provide a global queue order. Expired,
cancelled, or overflow work fails closed instead of starting after its useful
Discord lifetime.

The computation continues after the HTTP ACK has ended. The Cloud Run service
therefore requires instance-based CPU allocation (`--no-cpu-throttling`). The
approved minimum of zero permits scale-to-zero and therefore accepts cold-start
latency. An in-memory background task is not a durable queue: Cloud Run can still
terminate an instance. A future durable delivery design must add an explicit
queue/idempotency contract rather than silently treating the current process as
durable.

## One-shot slash-command registration

Register commands from a trusted local terminal, once per catalog change. On
Windows PowerShell 5.1 or newer, use the masked compatibility wrapper:

```powershell
$env:DISCORD_APPLICATION_ID = "1533373054309371924"
try {
  npm run register:commands:windows --workspace @clearra/discord-bot
} finally {
  Remove-Item Env:DISCORD_APPLICATION_ID -ErrorAction SilentlyContinue
}
```

At the `Discord bot token (Developer Portal > Bot > Token):` prompt, enter the
bot token from the application's **Bot** page. It is not the application ID,
public key, Discord password, or Google account password. The wrapper uses
`Read-Host -AsSecureString`, exposes the converted value only to the child Node
process, then removes it from the process environment. Never paste the token
into chat, source code, shell history, or a committed file.

Windows PowerShell 5.1 does not support `Read-Host -MaskInput`. If the literal
prompt `-MaskInput Discord bot token:` appears, cancel it and use the wrapper
above. PowerShell 7 supports `-MaskInput`, but the wrapper remains the preferred
version-independent path.

Discord displays a newly generated bot token only once. Reuse the current token
from the owner's password manager or secret store when one exists. **Reset
Token** creates a new token and invalidates the old one; update every existing
Oracle Gateway or other deployment that still uses the old token before relying
on a reset token.

Do not enable registration in the deployed service. Keep
`CLEARRA_REGISTER_COMMANDS=0` (the default) and do not deploy `DISCORD_TOKEN` to
Cloud Run. Normal HTTP interaction handling needs only the public application
key; Discord's interaction ID and token authorize the deferred response edit.
The bot token is required locally only for the one-shot registration request.

## Cloud Run configuration

Required for the deployed interaction service:

```text
CLEARRA_DISCORD_INGRESS=cloud-run
DISCORD_PUBLIC_KEY=<Discord application public key>
CLEARRA_REGISTER_COMMANDS=0
```

`DISCORD_APPLICATION_ID` may also be supplied as non-secret metadata. Normal
requests carry the application ID, so the runtime does not require it.

`DISCORD_PUBLIC_KEY` is public application metadata, not an SSH key. GitHub or
deployment SSH private keys must never be mounted in the container or used to
verify Discord requests.

Recommended initial execution settings:

```text
CLEARRA_MAX_CONCURRENT_SEARCHES=1
CLEARRA_MAX_PENDING_SEARCHES=8
CLEARRA_SEARCH_TIMEOUT_MS=180000
CLEARRA_INTERACTION_DEADLINE_MS=240000
CLEARRA_SEARCH_WORKERS_PER_SESSION=auto
CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1
CLEARRA_MAX_OUTPUT_BYTES=4194304
CLEARRA_JOB_TERMINATION_GRACE_MS=2000
CLEARRA_DISCORD_INTERACTION_PATH=/interactions
CLEARRA_MAX_INTERACTION_BODY_BYTES=1048576
```

Automatic Discord execution uses every logical processor visible in the Cloud
Run container. The approved eight-vCPU instance therefore runs one search with
eight workers. At the four-instance maximum, the service can run four searches
and up to 32 search workers in aggregate. `CLEARRA_USE_ALL_LOGICAL_PROCESSORS=0`
reserves one processor per instance. Explicit worker values above the
container-visible logical processor count are rejected. Users cannot override
the service policy with `--workers`, `--auto-workers`, `--cpu-threads`, or
`--use-all-cpu-threads`.

The HTTP adapter listens on `0.0.0.0:$PORT`, exposes `GET /health`, and accepts
Discord requests at `POST /interactions`. It verifies the raw body with
`X-Signature-Ed25519`, `X-Signature-Timestamp`, and the Discord public key before
parsing JSON. Invalid signatures receive HTTP 401; Discord PING receives PONG.
`/healthz` remains a local compatibility alias, but external Cloud Run probes
must use `/health` because Cloud Run reserves some paths ending in `z`.

The service must be publicly reachable because Discord cannot attach a Google
Cloud IAM identity token. Request authenticity is enforced at the application
boundary by the Discord Ed25519 signature.

## Tokyo build and deployment shape

The selected region is Tokyo, `asia-northeast1`. Build the current source with
the supplied Cloud Build configuration:

```powershell
$projectId = gcloud config get-value project
$buildServiceAccount = "projects/$projectId/serviceAccounts/clearra-build@$projectId.iam.gserviceaccount.com"

gcloud builds submit `
  --project=$projectId `
  --region=asia-northeast1 `
  --service-account=$buildServiceAccount `
  --config=apps/clearra-discord-bot/cloudbuild-interaction.yaml `
  --substitutions=_REGION=asia-northeast1,_REPOSITORY=clearra,_IMAGE_NAME=clearra-interaction,_TAG=latest `
  .
```

The interaction Dockerfile uses the Rust 1.96 Bookworm image and the same
`wasm-cpu-runtime,webgpu-search` features as the Linux CLI release contract. It
builds the current checkout rather than downloading the v0.5.1 release binary.
The root `.gcloudignore` deliberately reuses root-anchored `.gitignore` rules.
Do not replace them with global `**/target` or `**/coverage` exclusions:
`clearra-spin/src/target` and several `src/coverage` directories are tracked
Rust/C source modules, not generated output.

Keep the Cloud Run service shape fixed during initial testing:

```text
Region:                 asia-northeast1 (Tokyo)
Ingress:                all; public invocation
Container port:         8080
Request concurrency:    1
Minimum instances:      0
Maximum instances:      4
CPU per instance:       8 vCPU
Memory per instance:    16 GiB
CPU allocation:         instance-based / no CPU throttling
Startup CPU boost:      enabled
```

Cloud Run's per-instance CPU maximum is eight vCPUs. A single 16-vCPU instance
is not available; use the approved 0--4 instances at eight vCPUs each.
Concurrency 1 and `CLEARRA_MAX_CONCURRENT_SEARCHES=1` make
execution serial only within each instance, not across the service. Each
instance owns its own in-memory queue and there is no global FIFO ordering.

A deployment template is:

```powershell
$projectId = gcloud config get-value project
$publicKey = Read-Host "Discord application public key"
$image = "asia-northeast1-docker.pkg.dev/$projectId/clearra/clearra-interaction:latest"
$runtimeServiceAccount = "clearra-interaction@$projectId.iam.gserviceaccount.com"

gcloud run deploy clearra-interaction `
  --project=$projectId `
  --region=asia-northeast1 `
  --image=$image `
  --service-account=$runtimeServiceAccount `
  --ingress=all `
  --no-invoker-iam-check `
  --port=8080 `
  --concurrency=1 `
  --min=0 `
  --max=4 `
  --max-instances=4 `
  --cpu=8 `
  --memory=16Gi `
  --no-cpu-throttling `
  --cpu-boost `
  --timeout=60s `
  --set-env-vars="CLEARRA_DISCORD_INGRESS=cloud-run,CLEARRA_REGISTER_COMMANDS=0,CLEARRA_MAX_CONCURRENT_SEARCHES=1,CLEARRA_MAX_PENDING_SEARCHES=8,CLEARRA_SEARCH_TIMEOUT_MS=180000,CLEARRA_INTERACTION_DEADLINE_MS=240000,CLEARRA_SEARCH_WORKERS_PER_SESSION=auto,CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1,DISCORD_PUBLIC_KEY=$publicKey"
```

Both maximum flags are intentional: `--max=4` is the service-wide cost ceiling,
while `--max-instances=4` prevents Cloud Run's lower revision default from
silently limiting the active revision to three instances.

Set the deployed URL plus `/interactions` as the Discord application's
Interaction Endpoint URL. Do not point Discord at the Oracle Gateway or the
optional `/jobs` service.

## Deferred text and image paths

Ordinary text commands are intentionally inactive. After separate load and
security testing, they may use either the Oracle Gateway as a proxy or a
dedicated Cloud Run interaction ingress. Adding them must not reroute the active
slash catalog through Oracle.

Image rendering is also intentionally unavailable as a Discord command. A
future small-image path may send a length-bounded render request directly to the
Oracle Gateway. Large render requests must be bounded separately and can move
to Cloud Run if Oracle load testing shows that isolation is needed. Existing
renderer code is dormant capability, not an advertised endpoint.

## Local and optional remote execution

Outside Cloud Run, the process defaults to Gateway mode, but the current Gateway
ingress accepts neither slash commands nor ordinary messages. It is retained as
an integration seam for future text/image proxy work.

For a deliberate remote job-service test, set:

```text
CLEARRA_JOB_URL=https://<tested-service>/jobs
CLEARRA_JOB_TOKEN=<shared opaque bearer token>
CLEARRA_WORKER_AUTHORITY=remote
CLEARRA_MAX_CONCURRENT_REMOTE_JOBS=4
```

When this URL is absent in Cloud Run, direct execution is mandatory. When it is
present, the job service owns worker allocation and the interaction service
retains only the bounded remote-request slot.
