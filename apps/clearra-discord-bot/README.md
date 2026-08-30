# Clearrabot

Clearrabot exposes the represented Sfinder-compatible command contracts as
Discord slash commands. Oracle Gateway is the single Discord-facing runtime: it
owns slash commands, Modals, Message commands, `$`/`>` text commands, standalone
renders, acknowledgements, and result delivery. Heavy searches are proxied to
Tokyo `clearra-current-job`; that service never receives the Discord token,
interaction token, webhook URL, or channel credential.

## Active Discord surface

The registered slash commands are `/render-file`, `/path`, `/percent`, `/chance`, `/minimals`,
`/score`, `/score-minimals`, `/saves`, `/best-save`, `/cover`, `/setup`,
`/congruent`, `/congruent-cover`, `/setup-cover`, `/cover-percent`,
`/special-cover`, `/spin-cover`, `/spin`, `/spin-structure`, `/score-finder`, `/damage`, `/pc-setup`,
`/best-setup`, `/dpc-finder`, `/finesse search`, `/finesse score`, and `/help`. `/help` accepts an
optional command name in `arguments`; without it, the command lists the active
groups. Search commands use structured primary inputs instead of one raw argv
string. For example:

The registered Message command is `Apps -> Get original GIF` (localized as
`앱 -> 원본 GIF 받기`). Right-click or long-press a Clearra preview message and
select it to retrieve that exact original GIF without copying a message link or
ID.

```text
/render-file [image:<same-channel Clearra preview message link or message ID>]
/path next:<pattern> field:<grid, CTK3, or Fumen> [lines:<1..6>] [kicktable:<built-in>] [options:<hold=use>]
/cover next:<pattern> base:<grid, CTK3, or Fumen> target:<grid, CTK3, or Fumen> [kicktable:<built-in>] [options:<hold=use>]
/spin-cover next:<pattern> field:<grid, CTK3, or Fumen> [kicktable:<built-in>] [options:<type=TSD>]
/spin-structure pieces:<unordered inventory> field:<grid, CTK3, or Fumen> [lines:<any|0..4|0+..4+>] [profile:<t-spins|t-spins-plus|all-mini|all-mini-plus|all-spin|all-spin-plus>] [kicktable:<built-in>]
/score-finder next:<exact queue> field:<grid, CTK3, or Fumen> [lines:<1..6>] [kicktable:<built-in>] [options:<initial_b2b=false>]
/damage next:<exact queue> field:<grid, CTK3, or Fumen> [kicktable:<built-in>]
/finesse search target:<target delta> next:<queue or pattern> base:<starting field> [kicktable:<built-in>] [options:<hold and knowledge>]
/finesse score document:<CTK3 or Fumen with operations> next:<queue or pattern> [kicktable:<built-in>] [options:<hold and knowledge>]
/pc-setup remaining:<unordered piece inventory> [priority:<all|build|pc>] [max-setup-pieces:<1..10>] [queue-knowledge:<full-queue|visible-7>] [next-cycle-remaining:<exact inventory>] [setup-length:<auto|longer|shorter>] [kicktable:<built-in>]
```

`/best-setup` and `/dpc-finder` expose the same seven setup-ranking options.
Their default priorities are respectively `build` and `pc`; `/pc-setup`
defaults to `all`. All three default to nine setup pieces, full-queue
knowledge, automatic setup length, and no next-cycle restriction. `all` ranks
joint build and conditional-PC coverage, while `build` and `pc` select one
metric. Automatic length favors longer setups for `all`/`build` and shorter
setups for `pc`. When supplied, the required next-cycle inventory length maps
current remaining counts `7,4,1,5,2,6,3` to `4,1,5,2,6,3,7`; at most one piece
kind may appear twice and no kind may appear three times.

Board options accept raw CTK3/Fumen, a URL containing one of those values, or a
plain text grid with exactly 10 columns. Grid rows are written top first; use
`#` for a filled cell and `_` for an empty cell. Both are easy keyboard inputs,
visually separate occupied space from the low empty marker, and carry no
tetromino color. Older grid spellings remain parser-compatible but are not part
of the displayed syntax. Piece letters and fixed queues are case-insensitive.
PC commands, including `/score-finder`, support every height from one through
six rows. Build-probability, forward-search, and `/spin-structure` board
commands support one through 24 input rows; the structure-search height is
validated separately by its engine. `/cover` keeps independent `base` and
`target` editors in one Modal. Longer fields scroll inside the text area, so
the full 24-row range remains available without retaining page state on the
Gateway.

Invoking any search command without all of its required runtime inputs opens a
stateless command-specific Modal. Besides fields, the forms expose `next`, PC
height, initial B2B state, the built-in kick table, hold policy, T-spin type, or
remaining pieces as applicable. A setup-ranking Modal
uses Discord's five-component limit for `remaining`, `kicktable`, `priority`,
`max-setup-pieces`, and `queue-knowledge`, in that order. The lower-priority
`next-cycle-remaining` and `setup-length` settings remain available through a
complete slash or text command. Discord does not permit buttons inside a
Modal or another Modal as a Modal-submit response, so native previous/next page
buttons and a draggable grid cannot be implemented in this surface. Dragging
remains available in Clearra's web board editor; a future Discord Activity or
bounded external editor can reuse it.

Except for `/finesse score`, each search CTK3/Fumen field must decode to one
operation-free, static, 10-column page. Finesse scoring instead requires an
operation on every page. The Discord boundary validates that document and
forwards only its canonical initial mask, height, and ordered placements; it
does not submit the source document as a calculation argument.
CTK3 is read directly with the npm `ctk3` package and is never re-encoded as
Fumen; Fumen is decoded independently at the Discord boundary. For both formats
every non-empty color becomes the same occupancy bit. PC field inputs use a
canonical Board64 mask. The non-PC target, spin, damage, and `/cover` fields use
canonical Board240 masks so rows 7–24 are not truncated. `/cover` accepts
`base` plus a non-overlapping `target` delta containing only cells to add and
compiles to the existing build-probability request with `next`.

There is no explicit image-generation command. CTK3, Fumen, and plain fields
accepted by a search command are rendered inside that command while its search
runs. The renderer preserves CTK3/Fumen colors, uses neutral gray for generic
occupancy, keeps the empty grid, and joins touching cells of the same tetromino
color with the GUI-style outer bevel. A page operation retains its own boundary
even when it touches the same field color. Distinct touching static pieces of
the same color cannot be separated when the source contains only cell colors
and no placement identity.

`/render-file` does not generate an image. With `image`, it accepts only a
Clearra preview message link or message ID from the current channel. Without
`image`, it reads at most the newest 500 messages in that channel and selects
the invoking user's newest valid preview first, then the channel's newest valid
preview. Before downloading, it retrieves the selected message again so an
expired signed attachment URL is replaced by the current URL. A successful
response exposes only the original GIF as a Discord File component: it has no
text content, embed, reply, forward, or `message_reference`. Deleted or
otherwise unavailable source attachments produce an error instead of a
substitute image.

For exact selection, the Message command is the primary slash-side UX:
right-click or long-press the preview, then choose `Apps -> Get original GIF`.
Discord does not reliably attach a replied-to message to a slash-command
interaction, so `/render-file image:` remains the explicit fallback rather than
pretending a slash command can inherit reply context.

`/path` and the other PC commands accept any explicit `lines` value from 1
through 6, including 1L/3L/5L as in the GUI's manual dimension input. When it is
omitted, the Discord boundary considers every height from 1L through 6L and
runs each valid target in ascending order. A target is valid only when
the highest occupied cell fits,
`10 * lines - occupied_blocks` is a positive multiple of four, and `next`
contains at least that many tetrominoes. The exact target set is therefore
derived from the occupied-cell count and available queue length; partially
occupied fields can select either odd or even heights. Results are executed
serially in one per-instance search slot and delivered as soon as each target
finishes. This selection mirrors the PC compatibility layer's exact piece
window; it does not alter search generation or pruning.

Search results expose solution documents only as CTK3. Generated pieces
preserve their tetromino colors, while occupancy inherited from the input board
is encoded as `G`. The active Discord result path does not emit Fumen. The
legacy raw CLI Fumen form of `clearra sfinder cover` remains available outside
the slash ingress and keeps its existing exact colored-solution boundary.
Human-readable CLI and Discord summary probabilities are multiplied by 100 and
shown with `%`. Structured JSON keeps canonical `0..1` numeric probabilities so
the GUI and CTK3 result formatter do not multiply an already converted value.
`kicktable` is available on each rule-aware Discord command and accepts exactly
`srs-plus`, `srs`, `srs-x`, or `jstris-180`; omission selects `srs-plus` across
the Discord and Sfinder-compatibility boundaries.
`options` is a command-specific choice: PC commands and `/cover` select
`hold=use|avoid`, while spin commands select `type=TSS|TSD|TST|ANY`. These
choices extend to finesse as `hold=use|avoid` plus
`knowledge=both|full-queue|visible-7`; omission selects
`hold=use knowledge=both`. The public `full-queue` value is lowered to the
engine's private wire value only after Discord validation.
Finesse reports expose an exact total for a fixed queue or policy-level average
inputs for patterns without expanding per-queue or per-solution lists. None of
these choices can replace primary field/`next` inputs or select workers, files,
custom profiles, or output formats.

There is no active `/clearra` catch-all command and no active `/view` command.
Oracle receives every slash, Modal, Message-command, `$...`, and `>...` event
through Discord Gateway. It can provide one bounded Fumen/CTK3 GIF for supported
inputs. The committed service makes all-channel text coverage explicit; these
message forms are not registered as slash commands and still pass through the
same curated command policy and hard limits.

`$render-file [image]` and `>render-file [image]` are the text equivalents of
`/render-file`; `image` has the same current-channel message-link-or-ID
contract. When either command is sent as a reply with no `image`, the replied-to
message is selected exactly; otherwise omission uses the same newest-500,
caller-first lookup. A successful text response is likewise a file-only message
without a reply reference.

A standalone message containing a valid CTK3 or Fumen document is rendered
automatically. Plain `#`/`_` fields auto-render only when the complete trimmed
message is exactly 10 columns by 1–24 rows and contains no character other than
`#`, `_`, and row breaks. The same strict field may occupy the whole triple-
backtick block, optionally labelled `text` or `field`; prose outside the block,
partial fences, spaces, extra columns, and a 25th row prevent automatic
rendering. Search-command fields continue to render within their own command,
not through a separate image command, and are never forwarded to ambient
standalone detection.

Text commands accept the same plain grid, CTK3, Fumen, and payload-URL field
values as slash commands. A quoted field can span multiple rows. Discord code
blocks are also preserved as one field argument, so a field can be pasted
without quotes; `$cover` and `>cover` accept two independent code blocks for
`base` and `target`. For example:

````text
$path --field ```text
__________
______####
``` --next I --lines 2

>spin-structure ```text
__________
____#_____
``` TIO 1+ all-mini srs-plus
````

For the `spin-structure` shorthand, positional values are `field`, `pieces`,
`lines`, `profile`, then `kicktable`. Named forms may use `--pieces` or
`--inventory`, and `--profile` or `--spin-profile`. Its generated CTK3 pages
retain a `Spin: Regular` or `Spin: Mini` page comment so the two result
partitions remain identifiable after download.

Two guild-only administrative commands are intentionally omitted from `/help`:

- `/channel-settings` requires the invoking member's **Manage Channels**
  permission by default. It shows, sets, or resets the current channel language
  and disables or enables Clearra in that channel.
- `/server-settings` requires **Manage Server** by default. It shows, sets, or
  resets the server language and pauses or resumes Clearra across the server.

Discord's `default_member_permissions` hides these commands from ordinary
members. Both are also pinned to the guild-install and guild-interaction
surfaces, and every response is ephemeral. A paused server admits only
`/server-settings resume`; a disabled channel admits only
`/channel-settings enable`. The gate runs before a Modal opens, again on Modal
submit, and before text/document work enters a queue. Work already admitted is
allowed to finish so a settings change cannot strand a deferred reply.

The immutable application owner and configured bot-administrator IDs retain a
role-independent, unlisted text recovery path: `$bot-control` or `>bot-control`
followed by `help`, `server resume`, `channel enable`, or the corresponding
management action. This path is parsed only for that exact prefix and never
appears in `/help`. An administrator can use `$bot-control help` or
`>bot-control help` in a DM with the bot to see the complete private syntax.
It enters a one-active/two-pending recovery lane only after bot-
administrator authorization; unauthorized attempts are discarded without a
reply. Ordinary help/search/render traffic performs no application-owner or
permission lookup. Access state is persisted atomically in the separately
configured `CLEARRA_DISCORD_ACCESS_STORE`.

The compatibility boundary owns worker, output-format, tablebase, and
dependency-DAG policy. It rejects native file/output paths, custom WGSL, custom
kick JSON, and Sfinder contracts without a typed Clearra representation. Every
accepted command reaches Clearra as an argument array through `shell: false`.
The occupancy projection and two-field `/cover` routing are ingress changes
only; PC/build engines and pruning are unchanged.

## Active topology

```text
Discord slash command or Modal submit
  -> Discord Gateway INTERACTION_CREATE on Oracle
  -> incomplete runtime input: Oracle returns a Modal without starting work
  -> complete input: Oracle defers the response
  -> Oracle renders the command field and calls clearra-current-job /jobs
     for heavy work
  -> Oracle edits the original interaction response

$... or >... Discord message
  -> Oracle applies the same typed command policy
  -> the command's CTK3/Fumen/plain field renders inside that command
  -> Oracle proxies heavy search to clearra-current-job /jobs
  -> Oracle creates or edits its own channel reply

/render-file
  -> Oracle performs the authenticated current-channel lookup
  -> selected message is fetched again and its original GIF is reattached

Apps -> Get original GIF on a preview message
  -> Oracle validates the selected message target
  -> selected message is fetched again and its original GIF is reattached

$render-file or >render-file
  -> when sent as a reply, Oracle selects that replied-to preview exactly
  -> otherwise Oracle performs the same lookup and reattachment contract

standalone CTK3/Fumen or strict 10-column #/_ field message
  -> Oracle validates and renders the complete message

Both render-file paths
  -> newest 500 messages maximum, caller's newest then channel newest
  -> file-only response with no reply or message reference
```

Only `clearra-current-job` remains in the active Cloud Run request topology.
Oracle uses it for heavy slash and text searches. A job request contains only
curated Clearra arguments, an idempotency key, a deadline, and the separate job
bearer; it contains no Discord token, interaction token, webhook URL, raw user
ID, or channel credential.

The current-source and released-v0.5.1 `clearra.job.v1` images remain separate;
the current-source `clearra-current-job` serves Oracle's active slash and text
paths.
The compute artifact, protocol, scaling, and authentication boundary are in
[CLOUD_RUN_JOB_SERVICE.md](./CLOUD_RUN_JOB_SERVICE.md).

The Discord application's **Interactions Endpoint URL must be empty**. An empty
value selects Gateway delivery. Do not replace it with an Oracle HTTP URL or the
`clearra-current-job` URL.

## Discord timing and lifecycle boundary

Discord requires the initial interaction response within three seconds. The
Oracle Gateway handler therefore returns response type `9` for an
incomplete-command Modal or type `5` (deferred channel message) before starting
a complete command or Modal submission. Opening a Modal starts no work. Clearra caps
`CLEARRA_INTERACTION_DEADLINE_MS` at 14 minutes so completion and error edits
retain one minute of safety inside Discord's 15-minute interaction-token
window.

The generic compatibility limit remains three minutes. Reverse PC searches use
five minutes; forward, build, spin, and setup-ranking searches use fifteen
minutes. The caller's absolute deadline always wins when it is earlier, so a
Discord interaction receives at most the 14-minute gateway budget. A complete
slash request starts the remote job and, when a static field is available, one
bounded worker-thread GIF concurrently. If rendering wins, Oracle posts the GIF
then retains its attachment in the final edit. If searching wins, Oracle waits
for bounded rendering and sends one combined response. A render failure never
cancels or invalidates the authoritative search.

The reverse class contains native PC/failed-queue/path/percent/replay work and
the represented Sfinder PC solution and scoring commands. The forward class
contains native build/coverage/damage/spin work plus represented Sfinder target,
coverage, and spin commands. Native setup/setup-finder and Sfinder
pc-setup/best-setup/dpc-finder are the setup-ranking subclass of forward work;
this distinct label drives setup-only progress notices without changing the
15-minute engine limit.

For slash and text commands, `clearra-current-job` keeps `POST /jobs` open until
Clearra finishes. Each instance accepts one request and has no durable
distributed queue. The approved minimum of zero accepts cold-start latency;
expired, cancelled, overflowed, or terminated work fails closed instead of
starting after its useful Discord lifetime.

## Global slash-command synchronization

The synchronizer first reads the global catalog with complete localization
dictionaries. An exact match is a no-op so routine releases do not churn
Discord command versions or client caches. A mismatch causes exactly one global
bulk overwrite, never one request per guild; the write response is verified,
then bounded readback polling must return the same command IDs, versions,
localizations, options, and choices before the sync reports success. A stale
readback never causes a second write. The transport also never replays an
ambiguous 5xx/network PUT: bounded GET readback decides whether that one write
landed. If an unmanaged USER or MESSAGE command exists, synchronization stops
instead of silently erasing it with Discord's all-types bulk-overwrite API.

Discord propagates a successful global update to every existing and future
installation. Its documented global-command read-repair handles commands a
client already knows about, but Discord exposes no API with which a bot can
invalidate a desktop client's local list of newly added commands. If one client
still shows an old list after API readback succeeds, retry an existing command
or reload that Discord client (`Ctrl+R` on desktop). Do not delete and recreate
commands, mirror them into every guild, or repeat the bulk overwrite; those
paths increase version churn, command duplication, and permission drift.

For a manual sync from a trusted local terminal, use the masked compatibility
wrapper on Windows PowerShell 5.1 or newer:

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
`CLEARRA_REGISTER_COMMANDS=0` (the default). The Discord bot token is required
by the isolated registration step and Oracle Gateway. Bind it from each
deployment's secret manager rather than a literal environment value.
`clearra-current-job` receives its separate application bearer, not the Discord
token. Discord interaction IDs/tokens and both secret values are never part of a
job request.

For a release, the old `cloudbuild-command-sync.yaml` registration entry point
is not sufficient release evidence because its ephemeral build discards the
exact pre-mutation catalog. Use the tracked
`scripts/discord-command-catalog-release.mjs` path from a fresh extraction of
the canonical commit-byte archive only after the new runtime is deployed,
healthy, and serving traffic. It writes a canonical source catalog, persists an
independent prior GET before the one possible PUT, and seals an independent
readback report. `DISCORD_TOKEN` is environment-only; never pass it as an
argument or persist it. A conditional restore is authorized only when a fresh
GET still equals the exact post-sync digest and must itself seal the restored
readback. The complete sync, restore, 1,200-second four-surface observation,
and final manifest commands are pinned in `CLOUD_RUN_JOB_SERVICE.md`. The
observation spec must come from
`scripts/release/materialize-production-probe-spec.mjs`; it hash-binds the
Discord/Cloud/Pages adapter, the separate Oracle read-only owner, and the sealed
zero-traffic managed-secret `/jobs` candidate-smoke report without placing
either credential in the spec or report. A release must preserve this order:

```text
Oracle release -> Gateway/Modal/job verification -> command sync
```

Do not rebuild or redeploy `clearra-current-job` as part of command sync.

## Command picker language and server installation

The registered command schema keeps English as its default and supplies Korean
`name_localizations` and `description_localizations` for commands, subcommands,
options, and choices. Discord therefore shows `/경로`, `필드`, or `넥스트`
when that user's Discord client language is Korean. The interaction payload
still carries the default names such as `path`, `field`, and `next`.

The stored administrator language settings override the user's command-picker
language. ClearraBot's guided Modal and responses use
`explicit -> channel -> server -> Discord interaction locale -> global English`
preference resolution. Thus a Korean client gets a Korean Modal when no stored
override exists, while an English-configured channel remains English.

The full Gateway text/image feature set needs only these bot-role permissions:

- `VIEW_CHANNEL`;
- `SEND_MESSAGES`;
- `ATTACH_FILES`;
- `READ_MESSAGE_HISTORY` (`/render-file` history lookup, message hydration, and
  replies);
- `SEND_MESSAGES_IN_THREADS` (same behavior in threads).

Their combined permission integer is `274878008320`. Install ClearraBot for a
server with:

<https://discord.com/oauth2/authorize?client_id=1533373054309371924&scope=bot%20applications.commands&permissions=274878008320&integration_type=0>

The bot does not need Administrator, Manage Server, Manage Channels, Manage
Messages, or Manage Webhooks. Manage Channels/Manage Server/Administrator are
checked on the member invoking `/channel-settings` or `/server-settings`, not
granted to the bot.
`MESSAGE_CONTENT` is a separate privileged Gateway intent required for `$` and
`>` channel text commands; it is not an OAuth permission bit.

## Settings authorization

The two management commands enforce Discord's resolved interaction permissions
at execution time in addition to their registration defaults:

- channel scope requires `MANAGE_CHANNELS`;
- server scope requires `MANAGE_GUILD`;
- `ADMINISTRATOR` satisfies either scope;
- the Discord application owner is also a ClearraBot administrator by immutable
  user ID, independent of roles in the current server.

The Discord runtime does not resolve application ownership at startup. A valid
management request first uses the permissions already carried by the Discord
interaction. Only when those native permissions are insufficient does it
lazily request Discord's authenticated current application object; the result
is cached for at most five minutes and refreshed only by another privileged
check. An expired value is never used after a failed refresh. Help, search,
rendering, access-state admission, invalid settings requests, DMs, and native-
permission successes make no ownership request. For an additional tightly
controlled operator, `CLEARRA_DISCORD_ADMIN_USER_IDS` accepts a comma/space-
separated list of immutable Discord user snowflakes; usernames are rejected.
Configured IDs are checked without a network request. All slash settings
responses, including failures and status reads, are ephemeral.

`/channel-settings` registers `default_member_permissions=16` and
`/server-settings` registers `default_member_permissions=32`, both with only the
guild interaction context and the guild-install integration type. Discord
therefore omits them from an ordinary member's picker and from user-installed
contexts. Because Discord's registration permission gate cannot grant an
application owner a special cross-server exception, the exact unlisted
`$bot-control`/`>bot-control` recovery path preserves that role-independent bot-
administrator authority without per-guild registration or OAuth overwrites.
Authorization happens before a bounded one-active/two-pending recovery lane;
failed authorization is silent.
Discord's picker cannot see Clearra's internal bot-administrator list. A bot
administrator who also needs the slash entries must be granted a user/role
override by a server administrator under **Server Settings -> Integrations ->
ClearraBot**; Discord does not accept a bot token for that overwrite. The DM
`$bot-control help` path remains available without that per-server override.
Search and help commands remain available through ordinary Discord channel and
application-command permissions.

Oracle owns the Discord token and performs the same lazy owner lookup for the
small set of operations that actually require it.

## Cloud Run configuration

One Tokyo Cloud Run service is active in the request path:

- `clearra-current-job` owns authenticated `/jobs` execution for Oracle's heavy
  slash and `$`/`>` searches. It does not receive the Discord token.

The job-service contract is:

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

The managed job bearer is bound separately as `CLEARRA_JOB_TOKEN`. Oracle loads
the matching bearer from OCI Vault; no plaintext value belongs in either
deployment command or settings file.

Run the tracked `prepare-cloud-runtime-service-account.mjs` helper before the
exact-source build and again immediately before Cloud Run deployment. It reads
only IAM and Secret metadata, never a Secret version payload. It creates the
dedicated `clearra-current-job` service account only when absent and requires
zero project-level roles. It permits exactly one unconditional Secret binding:
`roles/secretmanager.secretAccessor` on `clearra-job-token`. Authority over
`discord-bot-token` or any other Secret fails closed, so the job tier cannot
inherit the Gateway's Discord credential access.

The universal boundary is the global catalog plus every supported regional Secret
catalog. The helper discovers a nonempty, unique location set with
`gcloud secrets locations list`, validates exact project ID-or-number and
location resource names, and enumerates every location. It pins global calls to
`https://secretmanager.googleapis.com/` and each regional list/IAM read to
`https://secretmanager.LOCATION.rep.googleapis.com/` with the subprocess-only
`CLOUDSDK_API_ENDPOINT_OVERRIDES_SECRETMANAGER` override; it never changes the
parent environment or persistent gcloud configuration. Only the global
`clearra-job-token` is job authority. A regional Secret with that same leaf ID
remains a non-job Secret and must be inaccessible. Any empty, malformed,
duplicate, wrong-project/location, unreadable, or partial catalog fails closed.

The active caller must be able to submit Cloud Build as `clearra-build`,
administer the public Cloud Run service, read the `clearra` Artifact Registry
repository, and act as both exact service accounts. The least-privilege roles
are Cloud Build Editor and Cloud Run Admin on the project, Artifact Registry
Reader on the repository, and Service Account User on the build and runtime
accounts. Metadata reads additionally need project Secret Manager Viewer and
Service Account Viewer; project Security Reviewer is an accepted consolidated
read grant and supplies the repository-policy read that Artifact Registry
Reader does not. Effective evaluation needs project Security Reviewer,
Deny Reviewer, and Service Usage Consumer. Group/domain policies also require the
corresponding Google Workspace `groups.read` or domain-admin visibility, and
principal-set policies need Browser; missing visibility becomes `UNKNOWN` and
fails closed.

Enable `policytroubleshooter.googleapis.com` as a separately approved
prerequisite; the helper never enables APIs. It requires
`gcloud projects get-ancestors` to remain exactly one `clearra-cloud` project row
and requires an empty
`PRINCIPAL_ACCESS_BOUNDARY` binding search for its exact numeric project before
and after preparation. The PAB search needs a custom read role containing
`resourcemanager.projects.searchPolicyBindings`; Project IAM Admin or Owner is a
broader alternative. The installed GA
`gcloud policy-intelligence troubleshoot-policy iam` result must say
`CAN_ACCESS` for `clearra-job-token` and `CANNOT_ACCESS` for every other Secret,
with exact principal/resource/permission identity and complete allow/deny
explanations.
Inherited or group access, any `UNKNOWN`, and every API/evaluation error abort.

Immediately after observing or creating the runtime account, the helper freshly
re-enumerates the complete inventory and rejects drift from its initial snapshot.
It then performs the direct and effective global/regional checks. This occurs
before any Secret binding write. When the direct global job binding is absent,
even that Secret must initially be `CANNOT_ACCESS`; inherited job access is rejected. An
existing exact direct job binding must already be `CAN_ACCESS`, while every
non-job Secret remains `CANNOT_ACCESS`. After the one permitted write, the
helper freshly re-enumerates all locations and catalogs, requires them to equal
the pre-binding snapshot, checks every direct/effective permission, and seals
the result with one more fresh enumeration so catalog drift fails closed.

First creation additionally needs Service Account Creator plus project-level
Service Account User. For the single job-Secret policy write, prefer a custom
role containing only `secretmanager.secrets.getIamPolicy` and
`secretmanager.secrets.setIamPolicy` on `clearra-job-token`; exact-Secret Secret
Manager Admin or Owner is broader. The helper verifies the caller's effective
`setIamPolicy` permission before writing, never grants caller authority, and
re-observes ambiguous IAM mutations before it reports success. The operator who
runs the separately approved one-time API command needs
`serviceusage.services.enable`, normally Service Usage Admin
(`roles/serviceusage.serviceUsageAdmin`). The deployment
caller itself keeps Service Usage Consumer; the bootstrap helper never enables
the API or requires the broader role.

Run that prerequisite command before the accepted-source deployment window:

```powershell
$projectId = "clearra-cloud"
gcloud services enable policytroubleshooter.googleapis.com --project=$projectId
if ($LASTEXITCODE -ne 0) { throw "Policy Troubleshooter API prerequisite failed" }
```

Production job execution is pinned to one search with an explicit ceiling of
eight workers; it does not derive that ceiling from the processor count visible
to Node during startup. `CLEARRA_EXPECTED_VCPUS=8` binds both the Node worker
partition and Rust execution-resource admission to the revision's `--cpu=8`
authority. Startup CPU boost is enabled, and the candidate's Node startup probe
was observed reporting nine logical processors; that runtime observation is not
the configured CPU authority. `CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1` still
authorizes native Clearra to use the otherwise reserved final processor without
raising the independent `--auto-workers 8` ceiling. Native Clearra revalidates its
steady-state Linux affinity limit before creating the worker pool. A numeric
request above that hard limit is rejected rather than retried with an automatic
fallback. At four instances the service can run at most four searches and 32
workers in aggregate, although the current Oracle caller intentionally admits
only one remote job at a time. `CLEARRA_USE_ALL_LOGICAL_PROCESSORS=0` reserves
one processor per instance. Configurations with a different CPU limit or more
than one concurrent job require corresponding expected-vCPU and per-session
allocations.
Users cannot override the service policy with `--workers`, `--auto-workers`,
`--cpu-threads`, or `--use-all-cpu-threads`.

The service exposes `GET /health` and authenticated `POST /jobs`. The current
Oracle caller does not mint a Google identity token, so Cloud Run platform
invocation is public and the application bearer gates `/jobs`. The worker
re-applies command policy and starts Clearra with `shell: false`.
`/health` and every job response also expose the immutable full source commit,
engine build ID, and `clearra.search.contract.v2` revision. Oracle includes its
expected identity in every request and refuses a mismatched service before the
search starts.

## Tokyo job build and deployment shape

This section is a reference for a future job-service release. The existing job
revision remains active for Oracle slash and text commands until an independently
verified compute deployment replaces it.

The selected region is Tokyo, `asia-northeast1`. Build the current source with
the supplied Cloud Build configuration:

```powershell
$projectId = gcloud config get-value project
$sourceCommit = "<full-40-character-git-commit>"
$repository = "daejunnom/Clearra"
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
    --substitutions=_REGION=asia-northeast1,_REPOSITORY=clearra,_IMAGE_NAME=clearra-current-job,_TAG=source-$sourceCommit,_SOURCE_COMMIT=$sourceCommit `
    $archivePath
  if ($LASTEXITCODE -ne 0) { throw "Cloud Build submission failed" }
} finally {
  if (Test-Path -LiteralPath $archiveRoot) {
    Remove-Item -LiteralPath $archiveRoot -Recurse -Force
  }
}
```

The current-job Dockerfile uses the Rust 1.96 Bookworm image and the same
`wasm-cpu-runtime,webgpu-search` features as the Linux CLI release contract. It
builds the exact archived accepted commit rather than a mutable checkout or the
downloaded v0.5.1 release binary.
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
Concurrency 1 and `CLEARRA_MAX_CONCURRENT_JOBS=1` make
execution serial only within each instance, not across the service. Each
instance owns its own in-memory queue and there is no global FIFO ordering.

A deployment template is:

```powershell
$projectId = gcloud config get-value project
$sourceCommit = "<same-full-40-character-accepted-commit>"
$serviceName = "clearra-current-job"
$jobBearerSecretVersion = "<numeric-enabled-Secret-version>"
$candidateSmokeReportPath = "<new-absolute-canonical-candidate-smoke-report-path>"
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

# Before any mutation, the authenticated private wrapper runs the tracked
# capture helper as root on Oracle. It returns only non-secret release/settings
# digests, explicit prior runtime-authority kind/digest, prior job URL, and
# backup path.
$priorCaptureJson = & $oracleRemoteWrapper `
  -Operation capture-rollback-authority `
  -ScriptReleaseId $oracleCandidateReleaseId `
  -ScriptReleaseSha256 $oracleCandidateReleaseSha256 `
  -PriorRevision $priorRevision `
  -PriorRuntimeAuthorityKind $priorRuntimeAuthorityKind `
  -DeploymentNonce $deploymentNonce `
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
# producer observes the active symlink/tree digest, settings digest and all-five
# configuration, current PID/cwd, READY record, and fresh successful operation.
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

Both scale pairs are intentional. `--min=0` and `--max=4` set service-level
limits; `--min-instances=0` and `--max-instances=4` set revision-level limits.
Setting only `--max=4` can leave the active revision capped at three. Verify the
service and candidate revision separately before the explicit traffic mutation.
A candidate failure leaves `$priorRevision` at 100 percent. Keep Oracle pinned
to the exact tagged `$candidateUrl/jobs` through command sync and the pre-sync
rollback window; stable-URL rebinding is forbidden during that window. If a
post-cutover gate fails **before command sync**, restore both authorities:

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

The five-field v2 identity is deliberately incompatible with the previous
three-field caller. For a no-downtime migration, validate the tagged zero-
traffic candidate first, switch the new Oracle release to that candidate URL
with all five expected fields, and verify Gateway readiness plus one bounded
end-to-end job before the `update-traffic` command above. Keep Oracle on the
exact tag through command synchronization and the pre-sync rollback window;
stable-URL rebinding is forbidden in that window. If Oracle activation
fails before traffic moves, restore the old Oracle; the prior Cloud revision is
still serving 100 percent.

After global command synchronization, the rollback recipe above is no longer
authorized. Reverting then requires separately accepted backward-compatible
command-schema evidence and an exact command-catalog restore.

Keep `CLEARRA_EXPECTED_VCPUS=8` and
`CLEARRA_SEARCH_WORKERS_PER_SESSION=8` for the single-session 8-vCPU Cloud Run
service. Startup CPU boost is enabled, while startup-time runtime probes can
still differ from the configured CPU limit; the candidate's Node probe was
observed reporting nine. An allocation derived from that observation can
therefore over-request native workers. The explicit vCPU authority keeps Node
partitioning and Rust admission
on the deployed CPU limit, while the worker bound preserves all eight service
vCPUs and native Clearra independently validates the affinity ceiling at every
worker-pool boundary. `CLEARRA_USE_ALL_LOGICAL_PROCESSORS=1` must therefore
always preserve the native `--use-all-cpu-threads` authority; it is not
re-derived from Node's temporarily inflated startup count. Configurations with
a different CPU limit or multiple concurrent searches must use the corresponding
expected-vCPU and bounded per-session numbers.

Set Oracle's `CLEARRA_JOB_URL` to the `clearra-current-job` URL plus `/jobs`.
Leave the Discord application's **Interactions Endpoint URL empty** so all
application interactions arrive through Oracle Gateway. Never point it at the
job service.

## Oracle text and image path

Oracle owns slash, Modal, Message-command, `$`/`>` message ingress, and standalone
document rendering. For search commands, Oracle starts the bounded command-field
render and the proxied `clearra-current-job` search concurrently. Render-first
delivery posts the preview and later retains it in the final edit; search-first
delivery waits for the bounded render and posts one combined response. Rendering
failure cannot fail the search.

There is no explicit image-generation command. Static CTK3, v115 Fumen, and
plain fields are rendered inside their search command and never enter ambient
standalone detection. Standalone CTK3/Fumen
messages also render automatically; a standalone plain field must be the whole
plain message or whole fenced block and contain exactly 10 `#`/`_` cells per
row for 1–24 rows. A Fumen page comment is rendered in a bounded caption area
below the field on the matching GIF frame.

`/render-file [image:<same-channel message link|ID>]`, `Apps -> Get original
GIF`, `$render-file [image]`, and `>render-file [image]` are handled by Oracle. A
no-image text command sent as a reply selects that replied-to preview. All forms
only recover an existing preview GIF. Non-reply omission
scans at most 500 recent messages, preferring the caller's latest preview and
then the channel's latest. The selected message is fetched again before its
signed URL is downloaded, and success returns the GIF as a file-only, non-reply
message.

Oracle GIF encoding runs in a bounded worker thread rather than the Gateway
event loop for slash, text, and standalone fields. A live image-display failure
pointed to Discord's media-proxy layer rather than GIF generation or Clearra computation, so rendering
stays beside the Discord owner instead of moving to `clearra-current-job`.
Large-image rendering is still disabled pending separate load tests.

The committed unit enables text commands in all accessible channels with the
explicit `CLEARRA_ORACLE_ALLOW_ALL_TEXT_CHANNELS=1` safety switch. Unknown
commands, external bots/webhooks, oversized inputs, user worker switches,
duplicate work, and excess queue work remain rejected. `$help`/`>help` share the
slash help pages, and omitted PC lines retain and serially execute every
valid automatic target from 1L through 6L. An Oracle ingress slot is held
through the complete
render/search/send lifecycle, and integrated automatic followups are not sent
back through the legacy self-result renderer. Text mode requires the Developer
Portal's privileged Message Content Intent.

When Clearra and Sfinder-man intentionally coexist, list those guilds in
`CLEARRA_ORACLE_SFINDER_MAN_GUILD_IDS`. Clearra decides ownership before queue
admission, document decoding, attachment download, search, or rendering:
Sfinder-man owns user text commands and ambient Fumen/CTK3 rendering in those
guilds. Clearra still owns its private `bot-control` and `render-file` recovery
commands, slash commands, context-menu recovery, and its own generated-result
rendering. Delegated work is a neutral terminal outcome rather than a failure.

## Oracle and remote execution settings

The active Oracle settings are:

```text
NODE_ENV=production
CLEARRA_ORACLE_RENDER_ENABLED=1
CLEARRA_ORACLE_TEXT_ENABLED=1
CLEARRA_ORACLE_ALLOW_ALL_TEXT_CHANNELS=1
# Optional comma/space-separated guild IDs where Sfinder-man owns text/render.
CLEARRA_ORACLE_SFINDER_MAN_GUILD_IDS=<guild IDs>
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
CLEARRA_SETUP_PROGRESS_NOTICE_MS=300000
```

The root-owned Oracle settings file contains only OCI Vault Secret identifiers
and non-secret runtime settings. The wrapper fetches the Discord bot token, job
bearer, and private administration keys at `CURRENT`, decodes them only in
memory, exports them to Node, and writes or prints none of their values.
