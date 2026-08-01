# Clearrabot

Clearrabot submits Clearra searches to a long-running HTTP job service and
renders Fumen or CTK3 documents without an external rendering service.

## Active Discord surface

Only the `/clearra` and `/view` slash commands are enabled. Prefix commands,
ordinary-message command detection, and automatic viewing of documents posted
as ordinary messages are disabled. The Gateway fallback requests zero intents
and applies the same slash-only policy.

`/clearra` accepts Clearra command text without the executable name. `/view`
accepts a Fumen or CTK3 value, a Clearra viewer URL, or a `.ctk3` attachment.
Search commands may also receive a `.ctk3` attachment as field input.

## Configuration

Required in every mode:

```text
DISCORD_TOKEN=...
```

For a remote job service, also configure:

```text
CLEARRA_JOB_URL=https://jobs.example.test/jobs
CLEARRA_JOB_TOKEN=...
```

When `CLEARRA_JOB_URL` is omitted, Clearrabot uses the local
`http://127.0.0.1:8787/jobs` endpoint.

Cloud Run HTTP interaction mode also requires:

```text
CLEARRA_DISCORD_INGRESS=cloud-run
DISCORD_PUBLIC_KEY=...
```

`DISCORD_PUBLIC_KEY` is the public key shown for the Discord application. It is
not an SSH key. GitHub or deployment SSH private keys must never be mounted in
the container or used to verify Discord requests.

`DISCORD_APPLICATION_ID` is optional during normal HTTP operation because each
interaction carries it. Provide it together with `CLEARRA_REGISTER_COMMANDS=1`
when this process should register the global slash commands. Command
registration defaults off on Cloud Run so scaled instances do not repeat it.

Optional settings:

```text
CLEARRA_REGISTER_COMMANDS=1
CLEARRA_WORKER_AUTHORITY=remote
CLEARRA_MAX_CONCURRENT_REMOTE_JOBS=4
CLEARRA_SEARCH_TIMEOUT_MS=180000
CLEARRA_JOB_TOKEN=...
CLEARRA_JOB_POLL_INTERVAL_MS=250
CLEARRA_JOB_CANCEL_TIMEOUT_MS=2000
CLEARRA_VIEWER_URL=https://daejunnom.github.io/Clearra/
CLEARRA_MAX_GIF_BYTES=25165824
CLEARRA_MAX_CTK3_FILE_BYTES=25165824
CLEARRA_DISCORD_INTERACTION_PATH=/interactions
CLEARRA_MAX_INTERACTION_BODY_BYTES=1048576
```

Local Gateway mode is selected outside Cloud Run by default. Set
`CLEARRA_DISCORD_INGRESS=gateway` explicitly when needed. Discord delivers
interactions either through the Gateway or through the configured HTTP
endpoint, so only one mode should be enabled for an application at a time.

## Cloud Run adapter

The HTTP adapter listens on `0.0.0.0:$PORT`, exposes `GET /healthz`, and accepts
Discord requests at `POST /interactions`. It verifies the raw request body with
`X-Signature-Ed25519`, `X-Signature-Timestamp`, and the Discord application
public key before parsing JSON. Invalid requests receive HTTP 401. Discord
PING receives PONG, while enabled slash commands receive an immediate deferred
response before the existing Clearra POST job is submitted.

Build the container from the repository root:

```powershell
docker build -f apps/clearra-discord-bot/Dockerfile -t clearrabot .
```

The Cloud Run service must be publicly reachable because Discord cannot attach
a Google IAM identity token. Request authenticity is provided by Discord's
Ed25519 signature. Use concurrency 1 for CPU-heavy sessions and instance-based
CPU allocation (`--no-cpu-throttling`) because direct mode polls and edits the
deferred interaction after the initial HTTP response. A minimum instance can
reduce cold starts but incurs cost.

Set the deployed URL plus the configured path as the application's Interaction
Endpoint URL, for example:

```text
https://clearrabot-PROJECT.REGION.run.app/interactions
```

Cloud Run may still terminate an instance after acknowledgement. The Clearra
job service remains deadline-bound and idempotent, but durable result delivery
should use the relay boundary below.

## Relay boundary

`DiscordRelayIngressAdapter` accepts versioned
`clearra.discord.relay.v1` envelopes. A relay must acknowledge the Discord
interaction first and send an event with:

```json
{
  "protocol": "clearra.discord.relay.v1",
  "deliveryId": "stable-delivery-id",
  "acknowledgement": "deferred",
  "event": {
    "kind": "discord.interaction.create",
    "payload": {}
  }
}
```

The adapter shares the exact slash-command ingress used by Cloud Run. The
protocol reserves `discord.message.create`, but its default message ingress is
disabled. A future Gateway relay can inject a separate message ingress without
expanding the direct Cloud Run endpoint or changing slash-command execution.

## Search resources

Each search is stopped after three minutes by default. When
`CLEARRA_JOB_URL` is configured, remote worker authority is the default:
Clearrabot limits outstanding requests with
`CLEARRA_MAX_CONCURRENT_REMOTE_JOBS`, while each job-service instance derives
its workers from the CPU limit visible inside that instance.

Set `CLEARRA_WORKER_AUTHORITY=gateway` only for a job service that shares the
Gateway's CPU allocation. In that mode, `CLEARRA_MAX_CONCURRENT_SEARCHES`,
`CLEARRA_SEARCH_WORKERS_PER_SESSION`, and
`CLEARRA_USE_ALL_LOGICAL_PROCESSORS` control the host-local budget. Discord
users cannot override either policy with `--workers` or `--cpu-threads`.

## Job service protocol

Clearrabot never starts a `clearra` process. It creates an idempotent job ID and
posts one JSON document to `CLEARRA_JOB_URL`:

```json
{
  "protocol": "clearra.job.v1",
  "id": "client-generated-id",
  "kind": "clearra.command",
  "arguments": ["pc", "--lines", "4", "--format", "text"],
  "deadlineUnixMs": 0,
  "maxOutputBytes": 4194304
}
```

The bundled Cloud Run job service keeps the POST request open and returns a
terminal result. Compatible external services may instead return state
`accepted` or `running`, in which case Clearrabot polls
`GET {CLEARRA_JOB_URL}/{id}`. Cancellation and timeout send
`DELETE {CLEARRA_JOB_URL}/{id}`. Repeated POSTs with the same ID must refer to
the same job, and the service must enforce `deadlineUnixMs` even if the bot
disconnects.

See [CLOUD_RUN_JOB_SERVICE.md](./CLOUD_RUN_JOB_SERVICE.md) for the bundled
service's container, authentication, and deployment settings.

For a local smoke test, start the job service on its loopback-only default
port in one terminal. Point `CLEARRA_EXECUTABLE` at the local Clearra CLI:

```powershell
$env:CLEARRA_LISTEN_HOST = "127.0.0.1"
$env:CLEARRA_JOB_SERVICE_ALLOW_UNAUTHENTICATED = "1"
$env:CLEARRA_EXECUTABLE = "C:\path\to\clearra.exe"
npm run start:job-service --workspace @clearra/discord-bot
```

Build CTK3 once, then start the local Gateway in another terminal:

```powershell
npm run build --workspace ctk3
npm start --workspace @clearra/discord-bot
```
