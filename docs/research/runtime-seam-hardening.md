# Five-Tool Runtime Seam Hardening

Date: 2026-08-02

This record covers the runtime boundaries shared by PC search, build-probability,
setup search, damage search, and spin search. It deliberately does not change
candidate generation, exact merge rules, or pruning.

## Accepted Changes

- A browser run is accepted exactly once. Duplicate run requests are rejected,
  worker-construction and post-message failures become terminal diagnostics, and
  elapsed timers start only after the controller accepts the request.
- Non-success `final_response` messages are failures rather than completed jobs.
  Clearing or switching a workspace releases retained terminal/result payloads.
- Cancelling setup path expansion converts the active `loading` card into a
  terminal cancelled state before its worker is released. Native setup progress
  now reports geometry, graph, coverage, and finalization boundaries without
  changing the search graph.
- Damage and spin workspaces have separate keyed lifecycles. Switching tools
  cannot reuse the prior tool's controller, result, or worker tree. Native jobs
  receive cancellation and remain polled until their terminal event so they are
  not orphaned.
- Damage/spin combo and B2B inputs now match the host `u16` contract
  (`0..=65,535`). Forward `workers_used` reports actual finalized participants.
- The earlier nine-worker browser execution cap was superseded by the common
  worker policy: automatic execution uses `max(1, L-1)`, explicit full-CPU mode
  uses `L`, and every runtime has an `L` hard ceiling. Nine remains only the
  eager-prewarm ceiling, so it bounds idle memory without limiting a foreground
  search. Setup path-detail expansion still uses one worker.
- Distributed verifier initialization is bounded, active cooperative work emits
  heartbeats, stalled transports fail closed through the existing recovery
  policy, and the last verifier snapshot survives the final merge progress
  transition. One verifier-client watchdog scan now owns all pending deadlines,
  instead of allocating and clearing a timer for every candidate batch. Artifact
  fetch/import/compile/instantiate paths also have explicit deadlines.
- Interrupted optional prewarm releases the shared verifier pool. GPU warmup is
  single-flight and generation guarded, so cancellation or two host loops cannot
  complete into the wrong WASM warmup state.
- Browser benchmark runners reject a root without `index.html` before launching
  Chrome. A static WASM asset directory can no longer look like an indefinitely
  loading benchmark until the case timeout expires.

## Rejected or Deferred

- Dispatching batches as soon as the first verifier became ready was removed.
  The existing two-run large benchmark showed no meaningful improvement and the
  state machine added lifecycle risk. Verifier initialization may overlap
  coordinator geometry, but all selected verifiers are ready before the first
  batch is assigned.
- A persistent desktop setup graph/executor cache was not added. The measured
  retained footprint was about 330 MiB and safe invalidation would require a
  broader ownership and concurrency contract. Setup phase progress was added so
  the current exact work is observable instead.
- PC and build-probability search/pruning internals were left unchanged. The
  changes here are resource, lifecycle, input-contract, and presentation seams.

## Contract Evidence

- UI model tests: 9 passed.
- Browser worker lifecycle, worker-budget, verifier-pool, artifact-deadline, and
  distributed-runner contracts: passed through the repository's in-memory
  esbuild test path.
- Forward exactness tests: 21 passed, including serial/parallel equality and
  actual-participant accounting.
- Web-command compatibility tests: 68 passed.
- Setup-related `clearra-core-executor` native test executables compile.
- Web TypeScript, desktop in-memory Svelte/TypeScript compile, Rust formatting,
  and the affected Rust crate checks pass.

## Final Same-Snapshot Benchmark

The browser product harness was rebuilt after the watchdog change. Both rows use
source snapshot `1c6f8e5ef94cbe331fd0053b1416d09db39832ceec71d61ba3b40041bf57bd84`
and WASM SHA-256
`399e9408973135a0d385aaf88ab952075e8d13742621d5bae3dcca3563f31a33`.

| Rank | Total WASM workers | Run 1 | Run 2 | Mean |
|---:|---:|---:|---:|---:|
| 1 | 11, now the `L-1` default on this host | 30,637.730 ms | 30,568.405 ms | 30,603.067 ms |
| 2 | 9, historical constrained policy | 32,136.685 ms | 32,724.280 ms | 32,430.482 ms |

The historical nine-worker policy is 5.97% slower on this 12-logical-core host while
limiting the total WASM worker-instance count by 18.18%. Both policies return
3,018 unique solutions with normalized hash `cts1:4a1f5df1599fc97a`, cover all
1,814,400 patterns, report peak engine CPU bytes of 348,445,702, and do not
truncate. Browser-process memory sampling was unavailable, so the worker-count
reduction is the memory-risk basis; it is not presented as a measured browser
RSS reduction. The retained comparison prevents repeating the experiment; it
does not reintroduce nine as an execution cap.

Raw reports are under
`%LOCALAPPDATA%/Clearra/reports/runtime-seam-hardening-20260802` with phases
`seam-hardening-watchdog-final9` and `seam-hardening-watchdog-final11`.

## Live 4194 Regression

Port 4194 remained owned by the existing process throughout this work. The
following web flows reached terminal UI states without restarting that process:

- Damage: `O`, height 4, hold off; completed with nine legal placement paths.
- Spin: `T`, height 4, hold off; completed all four phases with zero matching
  spins. Switching from Damage started with a fresh idle result.
- PC: `IIOOO`, two lines, hold off; completed with four solutions.
- Build probability: an `O` 2-by-2 target, hold off; completed at 100% with two
  tilings.
- Setup: `IOTS`, maximum one setup piece; completed in 29.1 seconds with 77
  setups. Cancelling a lazy PC-path detail load removed the loading state and
  left the card in the terminal `cancelled / retry` state.

Entering 65,536 for Damage's initial combo shows the 0--65,535 validation error
and disables Run, confirming the UI-to-host `u16` boundary.

## Discord Modal, PC Auto-Height, and Cloud Run CPU Follow-up

Recorded on 2026-08-02 so these boundary changes are not reapplied in later
optimization passes:

- Missing slash-command boards now receive a stateless Discord Modal before any
  work is queued. It accepts a top-first, case-insensitive 10-column text grid
  of one through four rows as well as the existing CTK3/Fumen inputs. `/cover`
  keeps its two independent board inputs. Modal submissions return to the same
  typed parser; unknown, duplicate, oversized, or malformed inputs fail closed.
- Discord's native Modal component set has no pointer/canvas surface, so a
  draggable 4x10 editor was not imitated with buttons. The existing web board
  editor already owns pointer capture and drag painting; a future Discord
  Activity or bounded external-editor link can reuse that implementation.
- PC target auto-selection is a Discord orchestration rule, not a new pruning
  rule. For each target `L` in `2, 4, 6`, it requires the highest occupied cell
  to fit and computes `missing = 10L - popcount(field)`. The target is scheduled
  only when `missing > 0`, `missing % 4 == 0`, and the normalized queue-pattern
  length is at least `missing / 4`. Empty-field queues of 14 pieces therefore
  schedule 2L/4L; 15 or more schedule 2L/4L/6L. Feasible targets run in order in
  one search slot and publish each result before starting delivery of the next.
- Explicit `lines` follows the GUI's manual dimension contract and accepts every
  integer from 1 through 6, including 1L/3L/5L. Only omitted-line auto mode is
  intentionally restricted to the requested 2L/4L/6L candidate set.
- The compatibility layer still derives `--pieces` from that exact missing-cell
  count. Existing hold terminal lookahead means an empty initial hold does not
  add one to the required queue length. PC candidate generation, exact merge,
  and pruning are unchanged.
- The Cloud Run interaction revision is configured for 8 vCPU and 16 GiB, but
  Node reported eight processors while Rust's quota-aware standard probe
  reported six. Only a process identified as Cloud Run by `K_SERVICE` passes
  its observed count internally; ordinary quota-limited Linux containers retain
  Rust's quota-aware limit. Rust accepts a higher hard ceiling only after
  validating the current Linux affinity list, which is re-probed at worker-pool
  boundaries rather than cached for the process lifetime. Every secondary
  worker recap uses that same hard ceiling; a failed validation falls back to
  the standard probe. No external
  `CLEARRA_EXPECTED_VCPUS` deployment setting is needed.
- The Discord JavaScript suite passes all 95 tests. Rust formatting, the
  core-domain CPU-capacity tests, PC graph/core-executor checks, and the
  odd-/six-line compatibility regressions pass. Windows Application Control
  blocked execution of a freshly rebuilt PC graph test binary and affected
  supply/WebGPU executable or dependency build-script paths; those environmental
  blocks are recorded rather than treated as search results or bypassed.

No Cloud Run deployment, slash-command registration, commit, push, or live
4194-process restart was performed in this follow-up. The 6/7/8-worker Cloud Run
timing comparison remains deferred until an explicitly authorized canary; no
unmeasured eight-worker speedup is claimed here.

## Oracle Single-Image and Result-Delivery Follow-up

Recorded on 2026-08-02 to prevent the renderer and message-boundary work from
being rediscovered or reapplied:

- The Oracle host was reached with the user-prepared OpenSSH key path without
  reading the key. It exposes two logical processors, about one GiB of RAM,
  Node 22, npm 10, systemd, and outbound Discord API access. No remote file,
  service, firewall rule, or port was changed.
- Oracle message ingress is a separate default-off Gateway boundary. Enabling
  it requires an explicit Discord channel allow-list; the Gateway requests
  message intents only while rendering or text proxying is enabled. External
  bots/webhooks are ignored, while ClearraBot's own interaction-webhook result
  is the sole bot-authored exception.
- One accepted Fumen/CTK3 document produces one GIF attachment. CTK3 page count
  is inspected before decoding, and source length, attachment bytes, decoded
  pages, GIF frames/output bytes, message concurrency, pending work, message ID
  duplication, and per-user request rate all have independent bounds.
- A curated `$...` or `>...` text command renders its input first, sends heavy
  execution only to an explicitly configured current-source Cloud Run `/jobs`
  endpoint, edits Oracle's own preview to a terminal state, then sends the
  result as a separate reply. The worker does not call Discord. Gateway slash
  ingress stays disabled and Cloud Run continues to own slash interactions.
- The v0.5.1 download-based job image remains a pinned compatibility artifact.
  A separate current-source Docker/Cloud Build path was added for Oracle text
  proxy tests so new arguments are never sent to the old binary by accident.
- Build/cover/setup/forward CTK3 semantics were already correct: initial field
  cells are `G` and replayed placements retain piece identity. The only defect
  was palette drift. Oracle's GIF now uses the GUI result colors and flat
  occupied cells. No canonical key, search, pruning, or engine data changed.
- Fumen Bot's immediate visual-response pattern was used as a UX reference, but
  its unbounded multi-document behavior and follow-up-only interaction behavior
  were not copied. Oracle preserves existing attachments when it settles its
  own preview and uses a separate result reply for reliable message ownership.
- The GIF output writer now grows a typed byte buffer instead of retaining every
  output byte as a JavaScript number. After one warm-up, a deterministic
  128-frame/10x20 benchmark produced the same 676,571-byte GIF in 235.705 ms and
  252.677 ms. Observed heap deltas were about 3.82/3.84 MB; array-buffer deltas
  were about 3.48/1.71 MB and RSS deltas 7.26/1.63 MB. These are process samples,
  not peak-RSS guarantees, so the hard byte/page/memory limits remain required.

The Discord suite exercises render-first ordering, retained-attachment PATCH,
separate result delivery, Cloud Run CTK3 self-message rendering, terminal error
updates, pre-decode limits, palette pixels, queue bounds, and REST PATCH shape.
No build, upload, deployment, command registration, commit, push, or 4194
process restart was performed in this follow-up.

## Discord Guided-Input and 24-Row Field Follow-up

Recorded on 2026-08-03 so the command-surface work is not rediscovered or
reapplied in later optimization passes:

- Every represented search command now has a stateless `clearra:search:v2`
  guided Modal. A complete slash invocation still takes the direct deferred
  path; an invocation missing any required runtime input opens the form without
  queuing work. The former `clearra:board:v1` submission route remains readable
  during a rolling deployment, but new forms always emit v2.
- Forms carry the inputs owned by each typed contract: fields, `next`, PC row
  count, built-in kick table, hold policy, T-spin target, remaining inventory,
  and verification scope as applicable. Finite values use Discord string
  selects and are revalidated against the same allow-list on submit. The Modal
  submit is translated back into ordinary slash options and reaches the same
  parser as a direct command.
- PC fields retain the GUI's four-row starting template, allow one through six
  rows, and keep explicit odd heights. Other board-backed commands use the
  GUI's eight-row starting template and allow one through 24 rows. `/cover`
  retains separate `base` and `target` editors. Rows beyond the initial template
  are typed or pasted into the scrolling text area.
- Native Discord Modals cannot contain previous/next buttons, and a Modal
  submission cannot answer with another Modal. A process-local page token was
  intentionally not added because Cloud Run can scale to zero or route the next
  interaction to another instance. A draggable field still requires the
  existing web editor, a Discord Activity, or another bounded external surface.
- The Discord parser lowers non-PC board inputs to a canonical 60-hex-digit
  Board240 `--board-mask-v1`; the compatibility layer now carries that mask into
  build, forward/spin, and damage requests without truncating rows 7–24. The PC
  path retains its Board64 boundary. CTK3 is decoded directly and Fumen remains
  independently supported; both are projected to colorless occupancy only at
  this input boundary.
- Every rule-aware command accepts exactly the built-in `srs-plus`, `srs`,
  `srs-x`, or `jstris-180` profiles and lowers the selection through the common
  `--rule` boundary. Discord and Sfinder compatibility default to `srs-plus`;
  custom kick JSON remains intentionally unavailable. Native GUI and direct
  Clearra command defaults were already SRS+, so this closes the two remaining
  Discord/compatibility exceptions without changing explicit Jstris requests.
- This follow-up changes command ingress, typed mask transport, help text, and
  validation only. PC/build enumeration, candidate generation, exact merge,
  scoring policy, and pruning were not changed.
- The complete Discord suite passes all 122 tests. The 20 focused
  `sfinder_compat` Rust tests, Rust formatting, the web-command test-target
  compile, and whitespace validation also pass; existing unrelated Rust
  dead-code/unused warnings remain warnings only.

Deployment recorded on the same date:

- Cloud Build `9df8bca3-d438-463c-9dbf-2774e462c342` produced the unique
  `clearra-interaction:discord-srsplus-9b15857b923f-20260803005118` image.
  Revision `clearra-interaction-00008-qog` was first deployed with zero traffic;
  its tagged `/health` returned `{"status":"ok"}` before traffic moved to it.
  The public service then reported 100% on that revision, its normal `/health`
  remained healthy, an unsigned interaction still failed with HTTP 401, and no
  revision ERROR logs were present. The 8-vCPU, 16-GiB, concurrency-one,
  scale-to-four shape and existing environment-variable names were preserved.
- The Oracle host had no prior `/opt/clearra`, systemd Gateway unit, or
  credential file. The current JavaScript release, including the formerly
  untracked Oracle ingress source, was packaged with production dependencies,
  verified, installed at
  `/opt/clearra/releases/oracle-srsplus-9b15857b923f-20260803005118`, and linked
  as `/opt/clearra/current`. The verified `clearra-gateway.service` unit is
  installed but deliberately disabled/inactive because
  `/etc/clearra-gateway/credentials` does not exist. No insecure placeholder
  credential was created and no current-source `/jobs` service was deployed.
- The owner completed the masked local registration prompt. Its isolated status
  marker reported success for the 23-command global bulk replacement, so the
  deployed option catalog is no longer pending registration.
- The existing local listener on `127.0.0.1:4194` was observed and left
  untouched. No commit or push was performed.

## Oracle Automatic Guild Coverage and Global Command Sync Follow-up

Recorded on 2026-08-03 so the former per-channel activation requirement is not
reintroduced:

- Renderer-only Oracle ingress no longer needs a central guild/channel list.
  ClearraBot's own CTK3 result bypasses the optional user channel restriction;
  guild user input must carry a structured mention of the bot, while DMs remain
  explicit direct invocations. A lookalike mention string, ambient user message,
  external bot, or external webhook is rejected. `$`/`>` text proxying remains
  default-off and still requires a non-empty channel allow-list.
- Renderer-only Gateway identification requests `GUILD_MESSAGES` and
  `DIRECT_MESSAGES`, but not the privileged `MESSAGE_CONTENT` intent. Text mode
  adds that privileged intent. This uses Discord's documented exemptions for
  app-authored messages, DMs, and messages that mention the app; it does not
  broaden ambient message collection.
- Pending self results and user messages now have separate bounded queues. A
  completed active render drains the self queue first, preventing user traffic
  from starving automatic slash-result images without preempting active work.
- Global command registration now validates the returned command type/name set
  against the whole local catalog before reporting success. The separate
  `cloudbuild-command-sync.yaml` consumes the bot token only from a dedicated
  Secret Manager-backed build step. The interaction runtime remains token-free.
  Releases must still deploy and health-check the runtime before this one global
  sync; Discord then propagates it to all installed guilds without per-guild
  registration.
- The Developer Portal Guild Install defaults were saved with
  `applications.commands` plus `bot`, and only View Channel, Send Messages,
  Embed Links, Attach Files, Read Message History, and Send Messages in Threads.
  Administrator, management permissions, and Message Content Intent remain off.
  Existing command-only installations still require one guild-admin
  reauthorization because Discord cannot add a bot member or permissions without
  consent; later code/global-command updates do not.
- The Developer Portal identifies both the application and bot user as
  `ClearraBot`. Its interaction endpoint remains
  `https://clearra-interaction-50060711800.asia-northeast1.run.app/interactions`;
  that configured hostname returns `200` from `/health` and rejects an unsigned
  interaction with `401`. Presence, Server Members, and Message Content intents
  remain disabled.
- The Discord suite passes all 132 tests, including partial `MESSAGE_UPDATE`
  hydration, self-result deduplication, and the OCI Vault deployment boundary.
  The Oracle source was installed as
  `/opt/clearra/releases/oracle-auto-9b15857b923f-20260803013956` and atomically
  linked as `/opt/clearra/current` after remote syntax checks. That installed
  release remains the rollback baseline while the tracked Vault wrapper and
  renderer update await instance-side deployment and verification. Port 4194
  was untouched.

## Discord Credential Infrastructure Handoff

Recorded on 2026-08-03 so credential containers and IAM grants are not recreated:

- Google Cloud project `clearra-cloud` has the automatic-replication Secret
  Manager Secret `discord-bot-token`; version 1 is enabled. Its payload was not
  read during verification.
- Service account
  `clearra-command-sync@clearra-cloud.iam.gserviceaccount.com` exists. It has
  `roles/secretmanager.secretAccessor` on only `discord-bot-token`,
  project-level `roles/logging.logWriter`, and
  `roles/storage.objectViewer` on only the
  `gs://clearra-cloud_cloudbuild` source bucket. The bucket-scoped grant is
  required for a user-specified Cloud Build service account to read submitted
  source; none of these grants should be repeated or broadened. Regional build
  `bd9dbf31-0aa2-4407-8063-d7fd595a8c5f` completed successfully and verified
  the exact 23-command global Discord catalog against Discord's response.
- OCI tenancy `stemxstudioproject` now has active non-private Vault
  `clearra-oracle-vault` in `us-ashburn-1`. Its HSM-backed AES-256 key is
  `clearra-oracle-secrets-key`; identifiers are intentionally omitted here.
  Cross-region Secret replication is off and no private Vault was created.
- Default-domain dynamic group `ClearraOracleGateway` exists. Its sole matching
  rule targets the known Oracle instance directly; it
  must not be widened to a compartment-wide rule.
- Secret `clearra-discord-bot-token` is active with version 1 current. Its
  payload was not opened during verification. Active policy
  `ClearraOracleGatewaySecretRead` grants the dynamic group
  `read secret-bundles` for only that Secret.
- Active policy `ClearraOracleGatewayRunCommand`
  grants `use instance-agent-command-execution-family` only where the requested
  instance equals the target instance. Three non-secret, read-only preflight
  commands were accepted but received no response. The target is Canonical
  Ubuntu 22.04, which is absent from OCI's documented Run Command supported-image
  list, and its reported Oracle Cloud Agent plugin set does not include Compute
  Instance Run Command. Do not recreate the commands or broaden the policy. The
  Gateway deployment therefore used the separately secured SSH path.

## OCI Instance Principal Runtime Bridge

Recorded on 2026-08-03 so a plaintext Oracle credential file is not
reintroduced:

- The tracked Oracle service no longer requires
  `/etc/clearra-gateway/credentials`. Its optional
  `/etc/clearra-gateway/settings` contains only the non-secret
  `CLEARRA_DISCORD_SECRET_OCID`; the Secret value stays in OCI Vault.
- The tracked `clearra-gateway-vault-run` wrapper requests only the Secret's
  `CURRENT` bundle with OCI CLI Instance Principal authentication, decodes the
  returned base64 value in memory, and exports it only as the Node process's
  `DISCORD_TOKEN`. It writes and prints no credential material and fails before
  Node startup when the OCID, OCI CLI call, encoded value, or decoded value is
  invalid.
- The systemd boundary now enables bounded single-image rendering, keeps text
  commands disabled, applies `UMask=0077`, and disables core dumps. Existing
  filesystem, memory, task, and privilege limits remain intact; port 4194 is
  still unrelated and untouched.
- This bridge does not widen OCI Secret access. The instance dynamic group has
  only `read secret-bundles` scoped to the one Discord token Secret OCID, plus
  the unused self-target-only Run Command family grant retained for diagnosis.
- The host now has isolated OCI CLI `3.89.3` at `/opt/oci-cli-3.89.3` and only a
  `/usr/local/bin/oci` link. An `ubuntu`-user Instance Principal dry run decoded
  the `CURRENT` bundle to `/dev/null` successfully without printing its payload,
  length, or error body.
- Runtime archive
  `c3a64acc4eb4336b3429ba10c78416de7ea8b2dc761c4712d9b7f20f2d09fe59`
  contains only `package.json`, `src`, and `deploy/oracle`. It produced release
  `/opt/clearra/releases/oracle-vault-c3a64acc4eb4-20260802183326`; the prior
  release and pre-deployment unit remain available for rollback.
- `clearra-gateway.service` is enabled and active. Its current Invocation emitted
  exactly one `Oracle Gateway connected as ClearraBot` readiness record, stayed
  at zero restarts across two samples, and used about 27.8 MiB. The settings file
  is root-owned mode `0644` and contains only the Secret OCID; no credential file
  was created. The local port 4194 listener remained present throughout.

## Gateway Interaction and Split-Compute Cutover Follow-up

Recorded on 2026-08-03 to supersede the ownership and default-state statements
in the earlier Oracle follow-ups without reapplying their engine, palette,
Modal, queue, or pruning changes:

- Oracle now owns Gateway `INTERACTION_CREATE`, response type 9 Modals, response
  type 5 deferred ACKs, interaction-token retention, original-response edits,
  `$`/`>` messages, and one bounded GIF worker thread. The outgoing Discord
  Interaction Endpoint URL must be empty while this path is active.
- `clearra-current-job` owns only current-source heavy computation. Oracle sends
  curated arguments, a deadline, idempotency, and the separate job bearer; no
  Discord credential or webhook URL crosses that seam and the worker never
  calls Discord. The v0.5.1 job image remains compatibility-only.
- Search and rendering start concurrently after ACK. Render-first delivery
  posts the preview and retains its attachment in the final edit; search-first
  delivery waits for bounded rendering and sends one combined reply. Image
  failure is non-fatal to search, and channel-edit failure has a separate-reply
  fallback. Regression coverage exercises both orderings for slash and text
  paths.
- GIF encoding is serialized and bounded in a Node worker thread with a timeout,
  pending limit, and worker resource limits. Live image evidence located the
  observed display failure at Discord's media-proxy layer rather than Clearra
  computation or Oracle GIF generation, so the renderer remains on Oracle.
- The committed service explicitly enables all-channel `$`/`>` handling with
  `CLEARRA_ORACLE_ALLOW_ALL_TEXT_CHANNELS=1`; startup otherwise requires an
  allow-list. This expansion requires the Developer Portal's Message Content
  Intent and does not bypass parser, size, queue, duplicate, bot/webhook, worker,
  or output restrictions.
- Oracle now loads two OCI Vault Secrets: the Discord bot token and the Cloud Run
  job bearer. Only their identifiers are stored in the root-owned settings file.
  The wrapper obtains each `CURRENT` bundle through Instance Principal, decodes
  it in memory, exports it only to Node, and writes or logs no value. Vault read
  policy must remain scoped to exactly those two Secrets.
- The active compute target is Tokyo (`asia-northeast1`), 8 vCPU, 16 GiB,
  concurrency 1, min 0, and max 4. Both service-level `--min=0`/`--max=4` and
  revision-level `--min-instances=0`/`--max-instances=4` are required. A describe
  check found a service cap of four but an active-revision `maxScale` of three
  after a deploy that set only the service maximum; the next code deploy must
  correct and then verify both levels before claiming four-instance capacity.
- Cutover order is current-job deploy and authenticated health/smoke check,
  Oracle release and Gateway readiness, Message Content Intent verification,
  then clearing the old Interaction Endpoint URL. Verify `/help`, Modal submit,
  search, GIF retention, and `$`/`>` before removing rollback assets.
- Rollback restores the retained `clearra-interaction` Endpoint URL first, then
  restores the prior Oracle release/service state. The previous interaction
  service, previous job revision, and prior Oracle release remain available
  until stability is established. Routing rollback does not rotate or delete
  Secrets, and port 4194 remains untouched.

### Gateway cutover completion

Verified later on 2026-08-03; this completion record supersedes the pending
scale and routing observations immediately above and must not be replayed as a
new deployment task:

- `clearra-current-job` revision `clearra-current-job-00003-brl` is ready in
  `asia-northeast1` with 8 vCPU, 16 GiB, concurrency 1, CPU throttling disabled,
  startup CPU boost enabled, revision `maxScale=4`, and service `maxScale=4`.
  Both minimum settings remain zero. `/health` returns 200 and an unauthenticated
  `/jobs` request returns 401.
- The Discord Interaction Endpoint URL was cleared and stayed empty after a
  Developer Portal reload. The retained `clearra-interaction` service received
  no request during the final cutover verification window, confirming that the
  observed slash traffic used Gateway delivery rather than the rollback HTTP
  service.
- Oracle release `/opt/clearra/releases/oracle-gateway-20260803-052403` is the
  active `/opt/clearra/current` target. `clearra-gateway.service` is enabled and
  active with zero restarts, about 30 MiB resident memory, and no invocation
  error. Message Content Intent is enabled for the explicitly configured
  all-channel text-command mode.
- Live Discord checks completed `/help`, the guided `/path` Modal, its submitted
  one-row PC search, `$path`, and `>path`. They returned the same 100% result,
  CTK3 output, and Oracle preview. Both render-first edit-in-place and
  search-first single-send orderings were observed, including a Cloud Run cold
  start. The newest Modal result's GIF displayed in Discord; older failures were
  independently decodable from their CDN objects and remain consistent with a
  transient Discord media-proxy/cache failure rather than Oracle computation.
- `$help` and `>help` now use the same help formatter as `/help`. A text PC
  request that omits `--lines` retains every slash-generated automatic target
  and executes those targets serially instead of selecting one or rejecting the
  request. The Oracle message-ingress concurrency slot is retained until the
  complete render/search/send operation settles; a background begin hook can no
  longer bypass the pending-queue bound.
- Live text auto-target verification returned exactly the expected 2L and 4L
  replies. Integrated `Automatic PC target` followups are explicitly excluded
  from legacy self-result rendering, preventing a redundant bot-to-bot image
  reply while retaining old interaction-webhook rendering for non-integrated
  CTK3 results.
- The final Discord test suite passes 154/154, including worker bounds,
  attachment retention, Modal validation, interaction deduplication, text input
  normalization, automatic text target series, full-operation ingress bounds,
  self-result deduplication, and both delivery race orderings. The existing
  local listener on `127.0.0.1:4194` remained present and was not modified.

This follow-up changes the runtime seam and delivery ownership only. PC/build
search generation, exact merge, scoring policy, and pruning are unchanged.

### Command cache and settings-authority follow-up

Recorded later on 2026-08-03 so the desktop-cache workaround and authorization
boundary are not replaced with broader or repeated registration:

- A Discord desktop client retained an older global command list after the API
  catalog had already registered and verified successfully. One client reload
  (`Ctrl+R`) exposed all 24 commands immediately. Discord documents read-repair
  for a stale command a client can invoke, but provides no bot API to invalidate
  a client's missing-new-command list.
- Global synchronization now performs
  `GET with_localizations=true -> exact no-op`, or one bulk `PUT` followed by
  bounded GET readback of the same catalog, command IDs, and versions. A stale
  readback never retries the PUT. Do not replace this with delete/recreate,
  guild mirroring, server-ID loops, or repeated overwrites.
- `/language show` remains public. Channel mutations require Discord
  `MANAGE_CHANNELS`; server mutations require `MANAGE_GUILD`; Discord
  `ADMINISTRATOR` satisfies either. The authenticated application owner is
  resolved lazily to an immutable user ID only when a valid mutation lacks those
  native permissions, then cached for at most five minutes. Expiry requires a
  fresh privileged-path lookup and refresh failure is fail-closed. Startup,
  read-only, normal commands, native managers, and configured administrators do
  not perform the ownership request. The owner receives separate bot
  administrator authority even without a server role. Optional extra admins are
  accepted only as validated snowflakes, never usernames.
- Settings interactions are deferred ephemerally. Registration-wide default
  permissions are intentionally not applied to the mixed read/write command:
  they cannot express subcommand-specific access and would defeat the
  application-owner override without per-guild OAuth permission management.
- Interaction edits and followups route through the signed interaction's own
  `application_id`, with the configured ID only as a fallback. A stale static
  ID therefore cannot strand an already-deferred command, and this correction
  requires no application-metadata request on ordinary command paths.
- Ambiguous command bulk PUTs, interaction followups, and channel-message POSTs
  are never transport-replayed. Command sync resolves an ambiguous single PUT
  through bounded GET readback and refuses to erase unmanaged USER/MESSAGE
  commands. Signed HTTP interaction deliveries claim their interaction ID
  before background execution, so retransmission cannot duplicate a mutation or
  search.
- These changes affect synchronization and administrative ingress only. Search
  generation, PC/build implementation, pruning, scoring, worker selection, and
  port 4194 are unchanged.
- Verification completed with all 189 Discord-bot tests passing. Oracle release
  `/opt/clearra/releases/oracle-command-auth-20260803-093711` became the active
  `/opt/clearra/current` target with `clearra-gateway.service` active,
  `NRestarts=0`, and a successful `ClearraBot` Gateway READY. Tokyo Cloud Build
  `32829a33-7e57-4992-a540-be44057e38b3` then performed an exact 24-command GET
  verification and reported `Verified unchanged`, so it issued no catalog
  overwrite. The existing local listener at `127.0.0.1:4194` remained owned by
  PID 7276 throughout.
- A post-cutover Discord check confirmed that the Korean client correctly shows
  the registered `/경로`/`필드`/`넥스트` localizations while the channel's English
  Clearra preference opens the subsequent `path search form` in English. This is
  the intended separation between Discord client localization and ClearraBot
  response/Modal I18N. A one-row PC Modal run rendered the GIF first, then
  edited the same interaction to a 100% one-solution result while retaining the
  GIF and adding a one-page CTK3 file.
- The recorded full-feature server-install permission set is `VIEW_CHANNEL`,
  `SEND_MESSAGES`, `ATTACH_FILES`, `READ_MESSAGE_HISTORY`, and
  `SEND_MESSAGES_IN_THREADS` (`274878008320`). Bot Administrator, Manage Server,
  Manage Channels, Manage Messages, and Manage Webhooks are intentionally not
  requested; language-mutation management rights belong to the invoking member,
  and Message Content remains a separate Gateway intent.

### Administrative admission and GIF compatibility follow-up

Recorded later on 2026-08-03. This section supersedes the earlier public
`/language` surface described above; do not reintroduce that mixed command or
repeat the GIF diagnosis:

- The public `/language` command was replaced by two guild-only administrative
  commands that are deliberately absent from `/help`. `/channel-settings`
  registers `default_member_permissions="16"` (Manage Channels), while
  `/server-settings` registers `"32"` (Manage Guild). Discord client
  localization remains separate from the stored response/Modal language. Both
  commands are pinned to `contexts=[GUILD]` and
  `integration_types=[GUILD_INSTALL]`, so user-install contexts cannot inherit
  the management surface.
- Channel settings include language show/set/reset and command disable/enable.
  Server settings include language show/set/reset and pause/resume. Every slash
  response is ephemeral, and runtime authorization still checks the signed
  member permissions before using lazy bot-administrator authority. Ordinary
  admission, help, searches, rendering, native managers, and configured
  administrators perform no application-owner lookup.
- Discord's registration permission gate cannot grant the role-free application
  owner a global exception. Exact unlisted `$bot-control` and `>bot-control`
  forms therefore preserve the owner/configured-admin recovery path without
  per-guild command registration or OAuth permission overwrites. These tiny
  state operations bypass the heavy-message queue and user cooldown only after
  immutable-user-ID authorization. Failed authorization is silent; authorized
  requests use a separate one-active/two-pending recovery lane.
- A paused guild admits only server resume. A disabled channel admits only
  channel enable. The local, read-only admission check runs before a slash Modal
  opens, again on Modal submit, and before new `$`/`>` or user-document work is
  parsed, downloaded, or queued. Already admitted self-result postprocessing is
  allowed to finish to avoid stranding a deferred response.
- Access state is independent of locale state and uses JSON v1
  `{version, pausedGuilds, disabledChannels}` at
  `/var/lib/clearra-gateway/access-preferences.json`. Mutations serialize,
  replace the 0600 file atomically under the 0700 systemd state directory, and
  roll back both in-memory collections when persistence fails. Cross-guild
  channel removal/reassignment fails closed.
- The reported GIF problem had two cases. The live 865-byte 200x80 one-frame
  CDN attachment was intact, so that individual click failure was compatible
  with Discord client cache/display behavior. A separate nine-color 10x4 field
  crossed an LZW dictionary boundary and exposed a real encoder error: the code
  width increased one code too early, and the original test decoder contained a
  matching off-by-one that hid it. Both transitions were corrected. The 1,252-
  byte regression GIF and a larger multi-frame case now decode with Windows
  GDI+ validation enabled; attachment-ID retention did not need alteration.
- The full Discord-bot suite now contains 206 passing tests, including atomic
  access persistence/rollback, slash and Modal admission, exact recovery,
  authenticated bounded management recovery, native and bot-admin authority,
  GIF LZW boundary compatibility, and existing attachment retention. Search
  generation, PC/build pruning, Cloud Run worker selection, and port 4194 are
  unchanged.
- The earlier 203-test deployment record below predates the final management
  authentication/integration-surface hardening and is retained only as history;
  it must not be used as a rollback target. The same 203 tests passed inside the
  Oracle release after its final test sync;
  the original inactive pre-cutover run had already passed the then-current 202.
  `/opt/clearra/releases/oracle-admin-gif-20260803-102645` then became the
  active `/opt/clearra/current` target with MainPID 958131, `NRestarts=0`, and a
  successful `ClearraBot` Gateway READY. The state directory remained mode 0700.
  Tokyo Cloud Build `b2dc25e8-a180-4cb8-9d60-a2fb6920f90e` performed exactly
  one global catalog update and verified 25 commands, replacing `/language`
  with `/channel-settings` and `/server-settings`; do not repeat the overwrite
  merely to refresh a client cache. The existing local listener at
  `127.0.0.1:4194` remained PID 7276.
- The final security cutover used
  `/opt/clearra/releases/oracle-admin-gif-authz-20260803-104416`. All 206 tests
  passed inside that inactive release before `/opt/clearra/current` was switched.
  The restarted service reported MainPID 959496, `NRestarts=0`, and a successful
  `ClearraBot` Gateway connection; the state directory remained mode 0700.
  Tokyo Cloud Build `c3ee4372-87cc-4f32-8b85-0f8cda28ce90` succeeded and
  verified all 25 global commands after adding the explicit guild-install
  boundary. The local `127.0.0.1:4194` listener remained PID 7276.

### Multiline fields, automatic PC height, and color-preserving render follow-up

Recorded on 2026-08-03. This section supersedes the earlier Discord-only
2L/4L/6L automatic-candidate description and records the renderer work so it
is not repeated:

- Sfinder's reference `input/field.txt` is a top-first 10-column grid whose
  first line declares its height and whose cells use `O`/`_`. Discord already
  carries height separately, so its field value does not duplicate that first
  number. The displayed neutral grid syntax is `#` for occupied and `_` for
  empty. Older grid cells remain decoder-compatible but are intentionally not
  advertised.
- `$` and `>` catalog commands now preserve a quoted multiline field or one
  Discord triple-backtick block as a single bounded argument. An optional
  `text`, `txt`, or `field` fence label is removed. `$cover` and `>cover`
  accept two independent blocks for base and target. Empty or unterminated
  blocks fail closed before the command reaches an executor. CTK3 and v115
  Fumen remain direct decoders at this boundary; CTK3 is not converted through
  Fumen. Preview-only decoding mirrors every internally accepted legacy grid
  cell, and any later preview failure returns `null` so an authoritative text
  or slash search cannot be stranded without a terminal response.
- Omitted PC height now tests every candidate from 1L through 6L in ascending
  order. A candidate is retained only when it contains the occupied height,
  has a positive number of missing cells divisible by four, and the supplied
  queue can provide that many pieces. The exact set follows the field occupancy
  and queue length, so both odd and even targets are valid outcomes. Explicit
  1–6 values continue to work. User-facing text describes this general rule and
  must not present automatic selection as a fixed set of heights.
- `/render`, `$render`, and `>render` are renderer-only commands. They accept
  one 1–24-row colored grid, CTK3, v115 Fumen, or supported payload URL and
  never invoke the Clearra search executor or remote job. In a grid, `#`
  becomes neutral gray, `_`
  remains empty, and IOTSZJL keep their tetromino colors. CTK3/Fumen colors are
  preserved. The same bounded GIF worker, page/source limits, and attachment
  ceiling used by previews apply here.
- GIF cells now keep the empty grid while adjacent cells of one color share
  their internal edge and receive the GUI-style highlight/shadow only on the
  outer boundary. Gray occupancy keeps a cell-wise bevel. A page operation has
  a separate placement owner, so it retains its bevel even where it touches a
  same-color field cell. A color-only static viewer document has no placement
  identity, so separately placed touching pieces of the same color remain
  indistinguishable without changing the document format.
- Exact authenticated `$bot-control help` and `>bot-control help` forms expose
  the private management grammar to bot administrators, preferably in DM.
  Ordinary help, search, render, and malformed non-management traffic perform
  no application-owner lookup. Discord's picker cannot apply Clearra's
  internal administrator list; per-user slash visibility still requires a
  server-managed Integration override or an authorized user OAuth flow, never
  a bot token.
- PC/build search generation, pruning, worker selection, and port 4194 were not
  changed by this follow-up. The integrated Discord suite passed all 225 tests,
  including renderer-only executor isolation, color preservation, fenced text
  input, automatic odd-height candidates, management authorization, and GIF
  decoding. No deployment, global command overwrite, commit, or push was
  performed as part of this local implementation.

### Automatic-height terminology and Modal locale follow-up

Recorded later on 2026-08-03. This section supersedes the client/Modal language
separation in the administrative-admission follow-up and the automatic-height
examples above:

- Automatic PC target text now describes one bounded 1L-through-6L evaluation
  driven by field occupancy and queue length. User-facing help and current
  documentation no longer summarize that behavior as a 2L/4L/6L set. Exact
  empty-field fixture expectations remain in tests because they validate the
  arithmetic rather than define the available target heights.
- Modal and response locale resolution is now `explicit Modal selection ->
  channel -> server -> Discord interaction locale -> configured global default`.
  Unsupported interaction locales fall through instead of being mislabeled as
  English interaction preferences. Stored administrator choices therefore keep
  priority while an otherwise unconfigured Korean client opens a Korean Modal.
  Access-denial and terminal-failure fallbacks re-read a valid Modal selection
  as bounded presentation metadata as well; a malformed selector falls back
  safely without suppressing the response or changing admission.
- Korean Modal titles use the localized command name, and every `/verify` scope
  label is localized while stable internal option values remain unchanged.
- These changes affect only Discord presentation and locale selection. PC/build
  generation, pruning, execution order, worker selection, and port 4194 remain
  unchanged. The integrated Discord suite passes all 228 tests, including the
  interaction-locale fallback, stored-override priority, no-selector Modal
  submit, localized titles/scopes, and automatic-height wording regressions.

### Discord deployment, automatic-result labeling, and GUI palette verification

Recorded later on 2026-08-03. This is the completed deployment record for the
automatic-height and renderer follow-ups above; do not repeat its global command
overwrite or palette migration:

- GIF tetromino fills now use the main GUI field editor palette exactly: gray
  `#7b8581`, I `#55cbd3`, O `#f3cf4d`, T `#b66ad0`, S `#65c778`, Z `#e96e6e`,
  J `#628ae0`, and L `#ef9c4d`. Existing GUI-style exterior bevel and shared-edge
  handling remain intact. A decoded-GIF pixel regression verifies the emitted
  RGB values rather than only checking encoder input.
- Automatic PC height remains an occupancy- and queue-bounded 1L-through-6L
  search. The argument plan now carries its automatic origin independently of
  candidate count, so even a single feasible result is labeled, for example,
  `Automatic PC target: 1L`. Explicit `lines=1` remains an ordinary unlabeled
  single search. Slash commands and both `$` and `>` text ingress share this
  behavior.
- Tokyo Cloud Build `9786e638-2079-4501-84cf-88407ac0a1d0` succeeded and
  registered and verified 26 global commands, including `/render`. The later
  palette and result-label release changed no command schema, so no second
  global overwrite was performed.
- The final gateway release is
  `/opt/clearra/releases/oracle-discord-auto-palette-20260803-130738`; the prior
  verified release `/opt/clearra/releases/oracle-discord-i18n-20260803-123727`
  remains the immediate rollback target. Cloud Run job sizing and deployment
  were unchanged because this release is confined to Discord presentation and
  gateway-side rendering.
- Both the local suite and the inactive-release suite passed all 231 tests
  before cutover. After cutover, Chrome testing verified Korean localized
  `/path` and `/verify` Modals, selectable PC heights 1–6 including odd heights,
  the single-result automatic label with 100% coverage, and a downloadable
  200x80 `/render` GIF containing all eight GUI colors. The English `/path`
  Modal in the bot test channel is intentional: its stored channel-level English
  override outranks the server-level Korean setting; a channel without that
  override inherited Korean correctly.
- The running service points at the final release, is active with MainPID 964373
  and `NRestarts=0`, and reached its Discord READY boundary. The gateway settings
  file metadata is hardened to `0600 root:root`. The existing local listener at
  `127.0.0.1:4194` remains PID 7276 and was never stopped.

### Connected visible-garbage rendering follow-up

Recorded later on 2026-08-03. This supersedes the earlier statement that gray
occupancy keeps a cell-wise bevel:

- Visible base-field `G` cells now use the same color-and-placement-owner
  adjacency rule as I/O/T/S/Z/J/L cells. Orthogonally adjacent G cells therefore
  omit their shared grid edge and render as one connected gray region, while the
  region keeps its exterior GUI-style highlight and shadow. Diagonal cells do
  not connect, and empty cells retain the ordinary board grid.
- Placement ownership remains part of the merge key. An explicitly placed piece
  still keeps its boundary where it touches a same-colored existing field cell.
  CTK3's separate pending-garbage metadata is not a visible board cell and is not
  introduced into the renderer by this change; this follow-up concerns visible
  `page.cells` G values only.
- A decoded-GIF pixel regression covers both horizontal and vertical G seams,
  all four exterior edges, and an adjacent empty-grid cell. The local suite and
  the inactive server release each passed all 232 tests.
- The active release is
  `/opt/clearra/releases/oracle-discord-garbage-join-20260803-132127`, with
  `/opt/clearra/releases/oracle-discord-auto-palette-20260803-130738` retained as
  its immediate rollback. The service reached READY with MainPID 966108 and
  `NRestarts=0`. No global command synchronization or Cloud Run deployment was
  needed. The local `127.0.0.1:4194` listener remains outside this change.
- For Windows PowerShell OpenSSH deployment probes, do not pass a space-bearing
  `journalctl --since` timestamp through the remote-command argument boundary;
  quoting can be lost before the remote shell. Match the current systemd
  `MainPID` and `ClearraBot` READY log instead so an older READY line cannot be
  mistaken for the new process. The two timestamp-probe failures in this cutover
  correctly triggered the prepared rollback and did not represent application
  failures.

### Original GIF retrieval command follow-up

Recorded later on 2026-08-03. This section supersedes the exposed `/render`
command behavior described in the historical records above without rewriting
those records:

- Browser and Discord `Copy Image` operates on the displayed bitmap and copies
  only one rendered frame; it is not an original animated-file transfer. The
  public `/render` command, its Modal, text-command alias, help entry, and command
  registration are therefore removed. `/render-file` is the explicit original
  GIF retrieval path and does not run Clearra search or the GIF renderer.
- An explicit `/render-file image:` value accepts a current-channel Discord
  message ID or message link. With no explicit value, lookup reads at most five
  100-message history pages, prioritizes a recent preview owned by the requester,
  and then falls back to another recent valid Clearra preview in the channel.
  This keeps the implicit search bounded to 500 messages while retaining an
  exact-message route for older previews.
- A history or direct-message candidate is fetched again with a fresh Discord
  `GET` immediately before its attachment is downloaded. This matters because
  Discord attachment CDN URLs are signed and expire. Candidate metadata must
  identify a Clearra-authored `image/gif` within the configured size limit; the
  downloaded bytes are bounded again and must start with `GIF87a` or `GIF89a`.
  An unavailable implicit candidate is skipped so the next bounded candidate can
  be tried, while an unavailable explicit selection returns a terminal error.
- A successful retrieval is a Components V2 file-only message: flag `1 << 15`,
  one type-13 File component referencing
  `attachment://clearra-render-original.gif`, and the exact uploaded GIF bytes.
  It contains no ordinary content, embed, or `message_reference`, so it is not a
  reply and Discord presents a dedicated original-file control. The retrieval
  filename is deliberately outside the accepted source-preview filename set,
  preventing a later `/render-file` search from recursively downloading an
  earlier download response.
- `$render-file` and `>render-file` use the same selection, refresh, validation,
  and file-only output. A text invocation that replies to a preview may use that
  referenced message ID, but its successful output is still an independent
  no-reply File component, matching slash-command behavior.
- Automatic rendering remains at the message and command seams instead of an
  exposed render command. A standalone neutral field must be the entire plain or
  fenced input, contain exactly ten `#`/`_` cells per row, and contain from one
  through 24 rows; `#` becomes visible gray `G`, `_` remains empty, and top-first
  input is preserved. CTK3 and v115 Fumen documents retain their existing
  automatic color-preserving path. Fields embedded in slash, `$`, or `>` search
  commands are rendered inside that command's bounded preview path rather than
  being admitted as standalone fields.
- Local verification passes all 239 integrated Discord tests, including bounded
  500-message pagination, own-before-channel selection, fresh attachment
  refresh, signed-URL expiry fallback, GIF signature and size rejection,
  Components V2 file-only output, recursive filename exclusion, `$`/`>` parity,
  removed `/render` routes, and strict standalone-grid admission.

The API boundaries used here follow Discord's official
[Get Channel Messages and message metadata documentation](https://docs.discord.com/developers/resources/message#get-channel-messages),
[signed attachment CDN URL documentation](https://docs.discord.com/developers/reference#signed-attachment-cdn-urls),
and [Components V2 File component reference](https://docs.discord.com/developers/components/reference#file).

#### Deployment and live verification

Recorded on 2026-08-03 after the implementation above:

- Cloud Build `4c2ac63f-ebf6-469e-b5c0-355da0dbc8fa` produced the Tokyo
  `clearra-interaction` image. Revision
  `clearra-interaction-00010-cef` passed its tagged health check and now receives
  100% of untagged service traffic. It retains concurrency 1, an eight-vCPU/
  16-GiB instance shape, maximum scale 4, and the `DISCORD_TOKEN` Secret binding.
  No severity-ERROR entry was present for the revision during the post-cutover
  verification window.
- Global command synchronization build
  `207dd465-36da-44b3-b611-2009323ff43f` completed successfully and verified all
  26 commands in one global catalog. `/render-file` is present and `/render` is
  absent; no per-guild registration loop was introduced.
- The active Oracle release is
  `/opt/clearra/releases/oracle-discord-render-file-20260803-140403`. The service
  remained active at MainPID 967666 with `NRestarts=0` and no warning-or-higher
  journal entry in the final 20-minute check. The compute service remained on
  `clearra-current-job-00003-brl`; it was not rebuilt or shifted for this change.
- A real Chrome/Discord invocation of `/render-file` without `image` selected
  the requester's recent preview and returned only
  `clearra-render-original.gif` (11.54 KiB): no response text, embed, forward, or
  reply reference was visible. The browser's file-download action succeeded.
  `$render-file` then returned the same-sized GIF as a separate bot message with
  no reply relationship. Searching for the removed `/render` command offered
  only `/render-file`.
- Command-contained field ownership is covered by both slash and text
  integration tests: a CTK3 field supplied to `/path` produces exactly one
  `clearra-input-preview.gif` on that interaction, and a field supplied to
  `$path` starts the search/render race on the text command itself. Standalone
  detection is skipped whenever the parsed request is a search command.
- The local Discord suite and the inactive Oracle release each passed all
  239 tests. The unrelated local listener remained at `127.0.0.1:4194`, PID
  7276, throughout build, cutover, command synchronization, and browser testing.

### Reply target and Message-command GIF selection follow-up

Recorded later on 2026-08-03. This extends the original-file retrieval section
without repeating its existing refresh, MIME, size, signature, or file-only
delivery work:

- `$render-file` and `>render-file` now have an explicit tested selection order:
  a supplied link/ID wins, otherwise the command message's
  `message_reference.message_id` is used, and only a command with neither falls
  back to bounded recent history. The response remains an independent
  Components V2 file message and deliberately does not copy the command's reply
  reference.
- Slash interactions do not expose a reliable replied-to message. The exact
  slash-side selector is therefore one guild-only Message Application Command:
  default name `Get original GIF`, Korean localization `원본 GIF 받기`,
  `type=3`, `integration_types=[GUILD_INSTALL]`, and `contexts=[GUILD]`. It has
  no description or options and is absent from `/help` choices and Modal
  routing. Users select a preview with right-click/long-press, then
  `Apps -> ClearraBot -> Get original GIF`; `/render-file image:` remains the
  explicit link/ID fallback.
- Admission is type-aware and accepts only the exact registered type/name pair.
  The handler validates `target_id` against
  `data.resolved.messages[target_id]`, its ID, current channel, and any supplied
  guild ID, then deliberately reuses the existing authenticated fresh-message
  lookup. Partial resolved data and an old signed CDN URL are never treated as
  the final download authority.
- This is ordinary GIF retrieval, not management. Tests inject an administrator
  authority that would throw if called and prove zero owner/administrator
  lookups, zero Clearra executions, zero renderer calls, and zero history reads
  for both valid and malformed exact-target interactions. Server pause and
  channel disable still use the existing local access-preference gate.
- The integrated Discord suite passes all 244 tests. Coverage includes type-3
  catalog registration and synchronization, unknown-type rejection, Gateway
  and signed Cloud Run admission, exact target selection, malformed resolved
  data, text reply selection for both prefixes, URL refresh, file-only output,
  and the no-authority/no-compute boundary.

#### Deployment and live verification

- Cloud Build `66c9b53d-b19f-4bb3-a274-b7ba3b4dd723` produced the updated
  interaction image. The first no-traffic deploy request was rejected before a
  revision was created because Cloud Run limits the combined service and
  traffic-tag name to 46 characters; future tags for this service must stay
  short. Tag `gif-target-0803` succeeded, and revision
  `clearra-interaction-00012-pif` passed health and unsigned-request rejection
  checks before receiving 100% traffic.
- The active revision retains gen2, concurrency 1, eight vCPU, 16 GiB, CPU
  throttling disabled, startup CPU boost enabled, minimum scale 0 (the omitted
  default), and maximum scale 4. `clearra-current-job-00003-brl` was not rebuilt
  or shifted.
- Command synchronization build `66ca53ac-db65-4ea6-afd3-3195d6b1bfe9`
  completed successfully and re-read all 27 global Application Commands,
  including `Get original GIF`.
- The first live Message-command invocation exposed an independent deployment
  seam: the Developer Portal's Interaction Endpoint URL was empty, so Discord
  delivered interactions through the Oracle Gateway. The older active Gateway
  correctly did not admit the newly registered type-3 route and Discord showed
  a no-response result. The stable untagged service URL
  `https://clearra-interaction-piqxrlckbq-an.a.run.app/interactions` was then
  saved; Discord's signed endpoint verification reached the active revision and
  returned HTTP 200. A revision-tag URL is deliberately not stored, so normal
  Cloud Run traffic updates also update Discord delivery.
- A second live selection of the 15:20 preview through
  `Apps -> ClearraBot -> 원본 GIF 받기` returned only
  `clearra-render-original.gif` (988 bytes), and Chrome opened it as a 200x80
  GIF. A later `$render-file` sent while replying to that same preview returned
  the same 988-byte original as an independent, non-reply bot message. This
  differs from the 11.54-KiB file returned by an unqualified recent-history
  lookup and therefore proves that the reply target, rather than channel-latest
  fallback, was selected.
- Oracle was not redeployed: its active release
  `/opt/clearra/releases/oracle-discord-render-file-20260803-140403` already
  contains the reply-target precedence and passed the live text test. It remains
  active at MainPID 967666 with `NRestarts=0` and no warning-or-higher entry in
  the final check. The unrelated `127.0.0.1:4194` listener remains PID 7276.

The type-3 registration and interaction shape follow Discord's official
[Message Commands documentation](https://docs.discord.com/developers/interactions/application-commands#message-commands)
and [Interaction Data reference](https://docs.discord.com/developers/interactions/receiving-and-responding#interaction-data).

### Oracle-only Discord ingress decision

Recorded on 2026-08-04. This supersedes the split Discord ownership described in
earlier sections; those entries remain as deployment history and must not be
reapplied:

- Discord's Interactions Endpoint URL is intentionally empty. Slash commands,
  Modals, Message commands, `$`/`>` messages, standalone rendering, ACKs, and
  result delivery all enter through the Oracle Gateway.
- `clearra-current-job` remains the only active Cloud Run request tier. Oracle
  calls its authenticated `/jobs` endpoint for heavy slash and text searches;
  the payload remains free of Discord credentials and callback data.
- The separate interaction HTTP adapter/image and the unused generic Discord
  relay adapter are removed from the current source and test surface.
- Slash administration telemetry is written directly through
  `LocalUsagePublisher` into Oracle's bounded private store. The Pub/Sub
  publisher, Cloud Run telemetry relay, Oracle poller, transport key, and their
  deployment artifacts are retired.
- Runtime cleanup is deliberately separate from infrastructure deletion. First
  deploy and verify Oracle, then clear the Discord endpoint, then remove the two
  retired Cloud Run services and dedicated telemetry resources after a stability
  window. Do not remove or redeploy `clearra-current-job` during that cleanup.
