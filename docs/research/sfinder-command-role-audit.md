# Sfinder Command Role Audit

The Tauri scope, non-PC follow-up fixes, rejected visible-seven summary
experiment, and final validation evidence are recorded in
[`desktop-runtime-follow-up.md`](desktop-runtime-follow-up.md). Do not re-apply
that rejected setup experiment without new allocation evidence.

This audit uses knewjade/solution-finder 1.43 commit
`0e7c935a5399159a3d9c42fb8721e3c6842ae17d` and cringemoment/sfinder-man commit
`438187b6a0ce4bf543ffc9faae507fdc11970e13`. Command names are not treated as
interchangeable algorithms. Each command retains only the evidence required by
its output contract.

solution-finder is MIT licensed; a substantial port must retain its copyright,
permission notice, and dependency attribution. The audited sfinder-man commit
has no root license or copying notice and contains an unversioned `sfinder.jar`.
No sfinder-man source, data, or binary is copied into Clearra. Only observed
command contracts are independently represented by the typed Clearra engines.

## Command Roles

| Sfinder command | Solver work and retained evidence | Clearra mapping |
|---|---|---|
| `percent` | Tests each queue family for existence, builds success/failure coverage, and reports probability plus failed queues. It does not need a normalized set of every tiling. | `percent` uses an exact `PatternBitSet` union with `SearchOutputPolicy::CoverageSummary`. It omits solution identities, hashes, candidate digests, and traces. |
| `path` | Enumerates perfect-pack candidates, validates build orders, groups legal piece sequences, and attaches coverage to each surviving solution. | Direct `clearra path` remains the historical alias of `pc-replay`. Sfinder semantics are isolated at `clearra sfinder path` and compile into the complete PC result surface. |
| sfinder-man `cat-finder` | Expands one exact queue into its unique hold reorderings, asks solution-finder `path` for an exact perfect clear at the selected height, and ranks the paths with Jstris scoring plus the supplied initial state. | Clearra exposes this contract only as `score-finder` (including Discord `/score-finder`); the former raw name is retired. Clearra compiles it to fixed-queue `pc --objective all --score --score-profile jstris-ultra`, not forward `damage`. Nonzero initial combo and B2B end bonus fail closed until the typed scoring contract can represent them. |
| `cover` | Consumes supplied operation or fumen solutions and evaluates which queues can build them. It does not run perfect-pack search. | Direct `clearra cover` remains the historical alias of typed `build-coverage`. Active Discord `/cover` accepts a colorless base plus a target delta and compiles to build probability. The raw CLI `clearra sfinder cover <solution-fumen> ...` form remains a separate legacy exact-solution boundary. |
| `setup` | Searches placements satisfying required-area and margin constraints. It is not a PC-family setup policy finder. | Direct `clearra setup` remains the historical alias of the PC-family `setup-finder`. Sfinder's colored required-area contract is isolated at `clearra sfinder setup` and maps to the build-probability surface. |
| `spin` | Uses a specialized unordered T-spin structure search and SRS reachability. | This is a `spin-structure` family contract, not an ordered forward-spin alias. The compatibility namespace fails closed until the structural search/cover projection is implemented; Clearra's native forward spin finder remains a separate command. |
| `ren` | Runs forward longest-combo search, with separate hold and no-hold searchers. | Clearra forward-search infrastructure is the applicable boundary; no PC packing evidence is generated. |
| `util` | Transforms fields, operations, and fumens. | Clearra codec, CTK, and Fumen tools remain outside the search core. |

## Adopted Optimization

The `percent` output contract is existential coverage. Clearra therefore keeps:

- exact PatternBitSet coverage words;
- weighted probability and completeness;
- exact materialized, covered, and failed pattern counts;
- an explicit materialized-universe scope and completeness flag for failures;
- a bounded list of failed queue examples;
- resource and backend diagnostics.

It does not keep:

- normalized solution identities or their set hash;
- per-solution probability rows;
- representative replay state;
- candidate-set digests;
- minimum-cover inputs.

Hashing remains available to PC and `pc-replay` correctness surfaces. The output
policy cannot prune a candidate: it changes retained evidence only, so
buildability and coverage semantics stay identical.

## Fixed-Queue BuildUp Specialization

Sfinder's fixed-order BuildUp does not test every operation against every queue
position. It groups operations by piece and recursively visits only the current
or held piece. Clearra adopts that local idea without replacing inverse
lock-clear geometry search:

- inverse geometry and realization-feasibility rejection remain unchanged;
- an exact one-pattern fixed source enters witness verification from the first
  candidate rather than waiting for global coverage to become complete;
- current, occupied-hold swap, and empty-hold draw transitions select a
  precomputed per-piece operation mask;
- kick reachability, line clears, deleted-row provenance, and projection
  confirmation continue through the existing exact transition function;
- equal current and held pieces share one semantic existence branch;
- a held piece is not released after the finite source is exhausted;
- score, minimum-cover, execution-constraint, and observed-queue requests keep
  the complete BuildOrder language path.

The optimization changes candidate verification work, not the retained
solution identity. A forced full-coverage run remains an exact dual-run oracle.

## Compatibility Boundary

`clearra sfinder` is a limited Sfinder-man-style dialect, not complete
solution-finder 1.43 CLI compatibility. It accepts search spellings whose result
contracts have a Clearra-native representation. The boundary normalizes legacy
queue syntax and compiles a typed request; it never starts Java or another solver.

- PC search: `path`, `chance`, `minimals`, `score`, `score-minimals`, `saves`,
  `best-save`, and `score-finder`;
- two-field buildability coverage on Discord, and legacy supplied-solution
  coverage on the raw CLI: `cover`;
- colored target/build analysis: `setup`, `congruent`, `congruent-cover`,
  `setup-cover`, `cover-percent`, and `special-cover`;
- structural search: native Clearra `spin-structure`; Sfinder `spin` and
  sfinder-man `spincover` remain explicitly unavailable until the distinct
  structural cover result contract is implemented;
- forward search: native Clearra `spin-finder`/Discord forward-spin and damage
  surfaces only;
- PC-family setup ranking: `pc-setup`, `best-setup`, and `dpc-finder`.

The legacy raw CLI colored-Fumen allow-list uses exact initial occupancy plus
one aggregate occupancy mask per piece kind. This preserves repeated equal
pieces: two exact placement decompositions are equivalent only when their
colored board identity is equal. Hashes are lookup aids and never authorize
acceptance. Discord does not construct this identity from CTK3 or Fumen input;
its active `/cover` path uses colorless base and target-delta occupancy instead.

The remaining differences are product-contract differences, not spelling bugs:

- solution-finder defaults to SRS and softdrop behavior, while the Sfinder-man
  `chance`/common `cover` wrappers use Jstris 180 and other wrappers do not use
  one global kick rule; Clearra now defaults every omitted compatibility rule
  to SRS+, while Jstris 180 remains an explicit built-in selection;
- solution-finder `cover` consumes one or more supplied operation/Fumen pages
  directly and supports B2B, spin, Tetris, line, and softdrop modes; active
  Discord `/cover` maps one base/target-delta pair to build probability, while
  the retained raw CLI mapping accepts one exact colored solution and constrains
  a PC calculation;
- solution-finder `setup` has required, margin, free, and forbidden regions that
  are not equivalent to Clearra's PC-family setup finder or a colored exact-fill
  build-probability target;
- solution-finder `spin` is an unordered SRS T-spin structure search. It is
  not lowered to Clearra's ordered, queue-consuming forward-search contract;
- the official solution-finder percent fixture reports 4,374 / 5,040 under its
  SRS contract; the measured Jstris-180 dialect reports 4,408 / 5,040. This is a
  deliberate rule-contract difference and must not be advertised as parity.

## Worker Routing

Every represented Sfinder search command uses the worker path of its typed
Clearra target instead of maintaining a second pool:

- `path`, `percent`, `chance`, `minimals`, score variants, saves, and
  `score-finder` use the PC/scenario distributed geometry and
  verification coordinator;
- Discord `/cover` and target setup/cover variants use distributed build
  probability; legacy raw CLI colored-Fumen cover retains the PC/scenario path;
- native ordered spin and damage variants use the forward-search coordinator;
  Sfinder structural spin variants do not enter that coordinator;
- PC setup variants use the setup coordinator.

The compatibility boundary accepts `--workers N` and its `--cpu-threads N`
alias as a fixed request. `--auto-workers N` is an adaptive ceiling: it retains
each target engine's small-work serial gate. `--use-all-cpu-threads` is required
when either selection consumes the normally reserved logical processor. Fixed
and adaptive requests are mutually exclusive.

The boundary only transports resource policy. Exact canonical reduction,
candidate identities, and coverage unions remain owned by the target engine, so
worker completion order cannot change the result set.

## Rejected Direct Ports

- Sfinder strip/profile packing is not substituted for Clearra's inverse
  lock-clear geometry family and proof-carrying pruning pipeline.
- Sfinder's DFS, special I-piece paths, and SRS-only spin assumptions are not
  copied into the WASM CPU or WebGPU product paths.
- Sfinder `setup` and Clearra `setup-finder` have different product meanings.
- Clearra `pc-replay` is not behaviorally expanded into Sfinder `path` without a
  separate product contract and benchmark.

## 2026-08-02 Discord and Translator Audit

The historical Discord boundary registered one slash command for each represented
Sfinder contract. The current boundary does not register the native command
families, a `/clearra`
catch-all, or `/view`.
Tablebase, dependency-DAG, output format, native file/output paths, custom WGSL,
and custom kick JSON remain host-owned or blocked. Commands continue to use an
argv array and `shell:false`; sfinder-man's shell command, upload/list/delete,
process-list, and message-content-intent model is not imported.

The active slash path is Discord directly to one Cloud Run interaction service
and then directly back to the Discord interaction webhook. With no
`CLEARRA_JOB_URL`, the service executes the source-built Clearra CLI in the same
container. Oracle Gateway and the versioned remote job service are not hops in
this path. CTK3/Fumen compute decoding is active at the interaction boundary;
the image renderer remains dormant, with no advertised command or active image
ingress.

The compatibility translator now fails closed instead of silently changing these
inputs:

- extra positional values, including an additional cover Fumen;
- setup-family options that the one-inventory native contract cannot preserve;
- every Sfinder `spin`/`spincover` variant, because an unordered structural
  inventory and structural cover projection cannot be represented by the
  ordered forward-spin request;
- unsupported Sfinder commands such as `ren`, `util`, `parity`, render tools, and
  `special-minimals`.

Target mappings explicitly disable horizontal mirror. Clearra's native
build-probability default includes the mirror, whereas the audited wrapper's
single supplied target does not. Discord CTK3 and Fumen colors are occupancy
only. Repeated equal-piece color identity remains solely in the retained raw CLI
colored-Fumen cover boundary.

The WASM probe was also updated for ABI status `4` (`Progress`). Both serial probe
paths now continue until status `1`, `2`, or `3`; stopping at status `4` previously
produced an empty final event for cooperative postprocessing jobs.

### Accepted direct Cloud Run boundary

The deployment decision is recorded here so the earlier Oracle/job-service
shape is not re-applied to the slash path without new evidence:

- Cloud Run receives and verifies Discord HTTP interactions directly. It returns
  the deferred ACK within Discord's three-second window and uses the interaction
  token's 15-minute lifetime only for the later edit. The configured hard maximum
  is 14 minutes; the initial default is four minutes including queue time.
- Execution is serial per instance, not globally serial. Request concurrency 1
  and one active Clearra search apply independently to each instance; Cloud Run
  may scale from zero to four instances, so four searches can run concurrently.
  Each instance has its own bounded pending queue, 8 configured vCPUs, 16 GiB,
  and one native runtime-selected full-capacity search. Instance CPU stays
  allocated after the HTTP ACK through `--no-cpu-throttling`, and startup CPU
  boost remains enabled.
- Deployment fixes both the service maximum and active-revision maximum at four.
  Setting only service `--max=4` left Cloud Run's revision default at three, so
  `--max-instances=4` is also required for the approved capacity.
- Cloud Run's per-instance CPU maximum is 8 vCPUs. The rejected alternative of
  one 16-vCPU instance is not a deployable configuration; horizontal scale to
  at most four 8-vCPU instances owns that capacity instead.
- The selected deployment region is Tokyo, `asia-northeast1`. The interaction
  image builds the current checkout with Rust 1.96 and the Linux release features;
  it does not download the v0.5.1 CLI.
- Cloud Build ignore rules remain root-anchored. A rejected global
  `**/target`/`**/coverage` form removed tracked `clearra-spin/src/target` and
  build-coverage source modules from the upload and failed the Linux build; it
  must not be reintroduced as a source-size optimization.
- The Rust image stage must copy `assets/skins/default` and the embedded PC4
  tablebase path used by `clearra-cli`. Omitting either is a build-input defect,
  not a reason to remove the renderer or tablebase contract from the CLI.
- Command registration is a trusted local one-shot operation. The deployed
  interaction service keeps registration off and does not receive the Discord
  bot token.
- Ordinary text commands remain disabled. A later experiment may deliver them
  through Oracle Gateway or a distinct Cloud Run ingress. Small image rendering
  may later use a length-bounded Oracle request; large rendering may move to a
  separate Cloud Run boundary after load evidence. Neither deferred feature may
  silently expand the current slash ingress.
- `Dockerfile.job-service` and `clearra.job.v1` remain only as an explicit remote
  compatibility seam. Their retained v0.5.1 image is not the current slash
  computation image.

This boundary changes ingress, lifecycle, and resource ownership only. It does
not alter candidate generation, result identity, pruning, or any previously
rejected optimization below.

### Structured slash syntax and CTK3/Fumen occupancy normalization

The 2026-08-02 follow-up replaced the single unrestricted `arguments` field on
search commands with command-specific structured inputs. `field`, `next`,
`remaining`, and `scope` are separate Discord options; `/cover` instead exposes
`base`, `target`, and `next`. Commands with optional legacy settings have one
bounded `options` string, parsed only as a space-separated `key=value`
allow-list:

- PC-family: `clear=1..6` (or `lines`) and `hold=use|avoid`;
- `/score-finder`: `lines=1..6` plus `initial_b2b=true|false`; the raw
  sfinder-man positional form also accepts combo and B2B end-bonus positions,
  but both must remain zero until the typed scoring contract represents them;
- `/cover`: `hold=use|avoid` only; its height is derived from the two fields;
- historical Clearra `/spin-cover` and `/spin` forward-search aliases:
  `type=TSS|TSD|TST|TSPIN|T-SPIN|ANY`; these names never establish Sfinder
  structural-spin parity and are replaced by the typed forward-spin options in
  the grouped v0.8 surface;
- all other represented commands: no bundled settings.

Worker selection, output format, paths, custom code/profiles, primary field and
queue replacement, unknown keys, and duplicate keys remain unavailable. This
parser is intentionally not a second arbitrary CLI ingress. `/help` reads the
same catalog metadata and handles `arguments:<command>` locally without
starting a search or consuming a compute slot.

The earlier CTK3 gap was at the Discord-to-translator seam: the viewer decoded
CTK3, while compute passed it unchanged to a Fumen-only Rust decoder. Compute
board options now accept one raw CTK3/Fumen document or a URL containing one
document value. The Node boundary decodes CTK3 directly with the npm `ctk3`
package and never converts it to Fumen; Fumen is decoded independently. Both
formats require one static 10-column page, and every non-empty `G/I/O/T/S/Z/J/L`
or Fumen cell becomes the same occupancy bit. Ordinary fields use a canonical
16-digit Board64 projection.

Discord `/cover` receives `base`, `target`, and `next`. `base` is the existing
occupancy and `target` is a non-overlapping delta containing only cells to add,
not the final union. Each field is independently CTK3 or Fumen and is projected
to a canonical 60-digit, 24-row mask. The translator validates the target's
tetromino area, overlap, completed base rows, and derived height before compiling
the request to the existing build-probability surface. It does not install a
colored solution identity or enter the legacy PC cover path.

CTK3 metadata is inspected before a page payload is materialized. Multi-page
documents, operation-bearing pages, non-empty garbage rows, unsupported widths,
and cells outside the command's Board64 or 24-row range are rejected at the
interaction boundary. This prevents compact bundles representing hundreds of
thousands of pages from creating eager decode work or indefinite loading.

Discord solution documents use CTK3 exclusively and do not emit Fumen. Generated
tetromino cells preserve their piece colors, while occupancy inherited from the
input board is encoded as `G`. This is an output representation rule, not an
authorization to recover piece identities from input colors.

This seam was complete for that revision. Do not reapply the discarded CTK input
piece-color identity, grey/colored pair, or multi-page `/cover` design, and do
not reimplement the projection in the Rust search engines. The PC/build engines,
candidate generation, and pruning code remain unchanged. `.ctk3` attachment
ingestion and image rendering remain dormant future interfaces; active board
options accept text or a URL carrying the document value.

The following deployment evidence predates the later occupancy-only input and
two-field `/cover` revision; it validates the recorded topology, not the current
slash syntax. Tokyo Cloud Build `7d56b65c-f65f-4666-9e40-6d7f0aaa546c`
validated that source-built image end to end, including the CLI rules and
two-line PC smoke checks plus the Node workspace build. The pushed immutable image is
`clearra-interaction@sha256:c4534d55a841bae073bb7cc9007e5e8af289d86943ad9eb890028cfdffc0993f`.

The first live Cloud Run probe also confirmed that `/healthz` is intercepted by
the platform's reserved-path rule for some URLs ending in `z`. The active
external probe is `/health`; `/healthz` remains only a local compatibility alias.

Tokyo Cloud Build `bdba77c3-35ca-46b4-9b71-3f6ac156c802` then rebuilt that
health-route correction as immutable image
`clearra-interaction@sha256:5089d2ba1fda3565f3517679e79e68ae3d66fa17c2a8d5158a11bc3e9a892b8a`.
Production revision `clearra-interaction-00003-kls` serves 100% of traffic at
`https://clearra-interaction-50060711800.asia-northeast1.run.app`. Its observed
configuration has no minimum-scale annotation, fixes both service and revision
maximums at four, and assigns each instance concurrency 1, 8 vCPUs, and 16 GiB.
The live `/health` probe returned 200 and an unsigned `/interactions` request was
rejected with 401. Startup logs reported Node's eight affinity-visible logical
processors and the then-materialized eight-worker request. A later real command
showed that this was not proof of native capacity: Rust rejected that request
against its effective hard limit of six. Do not restore the eight-worker
materialization or repeat the earlier service-only maximum deployment.

The Discord Developer Portal initially rejected the saved application metadata
as `tags: 0`. The first tag was `#falling-block-puzzle`, which is 21 characters
including the manually supplied hash while Discord accepts at most 20 characters
per application tag. The saved value is `falling-block-puzzle`; do not restore
the leading `#`. The application name, description, and direct Cloud Run
interaction endpoint were saved successfully with that corrected tag. Discord
stores the application name and bot-user name separately; both are now saved as
`ClearraBot`. The existing public-app setting, disabled privileged Gateway
intents, installation scopes, and bot token were not changed.

The first local command-registration attempt failed before any Discord request
because this Windows host uses Windows PowerShell 5.1, whose `Read-Host` has no
`-MaskInput` parameter. The literal `-MaskInput Discord bot token:` prompt was
therefore not evidence that `DISCORD_TOKEN` had been set correctly. The accepted
Windows path is the `register:commands:windows` workspace script: it uses the
5.1-compatible `-AsSecureString`, converts the token only in process memory for
the child Node process, and removes the temporary environment value afterward.
Do not retry the unsupported manual `-MaskInput` command on this host.

### Native worker authority correction

Production revision `clearra-interaction-00005-lpg` remained configured for
8 vCPUs, 16 GiB, concurrency 1, minimum 0, and maximum 4. Node 22's
`os.availableParallelism()` reported eight, while the source-built Rust CLI's
`std::thread::available_parallelism()` reported a hard limit of six and rejected
`--auto-workers 8` before search execution. Node wraps libuv, whose Linux query
uses the calling thread's affinity mask; Rust also treats container affinity and
cgroup capacity as part of its effective estimate. The observed values alone do
not distinguish a steady cgroup quota from a thread-affinity undercount, so an
eight-worker override would violate the product hard-lock contract.

The accepted fix applies only at the host/runtime boundary. A single automatic
session omits the numeric `--auto-workers` argument and passes
`--use-all-cpu-threads`, allowing the native Auto policy to choose its own hard
limit (six in the failing revision). Reserve-core mode omits the full-use flag
and therefore selects the native limit minus one. Explicit numeric settings and
automatic allocations divided across multiple concurrent sessions retain their
bounded numeric requests. PC/build candidate generation, result identity, and
pruning are unchanged. The current production concurrency remains one; raising
the per-instance search concurrency above one requires a separate native
capacity probe or shared worker budget so a Node-based partition cannot
oversubscribe a lower Rust effective limit.

References: [Rust `available_parallelism`](https://doc.rust-lang.org/std/thread/fn.available_parallelism.html),
[Node `os.availableParallelism`](https://nodejs.org/api/os.html#osavailableparallelism),
and [libuv `uv_available_parallelism`](https://docs.libuv.org/en/v1.x/misc.html#c.uv_available_parallelism).

### Evaluated modal field fallback (not enabled)

A missing board can be collected with a Discord Modal, including `/cover`'s
separate `base` and `target` boards, but the current command registration and
HTTP ingress deliberately do not enable it yet. Board options are registered as
required, the adapter always returns deferred callback type 5, and ingress only
accepts application-command interactions. A Modal must instead be the initial
type 9 response before any defer; its submission arrives as a new type 5
interaction and can then enter the existing deferred search path.

The safe Cloud Run design is stateless. The modal must contain every value needed
to reconstruct the command (board fields plus any already supplied next/options)
rather than storing a nonce in an instance-local Map, because scale-to-zero,
four instances, and revision replacement can route the submission elsewhere.
`/cover` fits within the five-component modal limit with `base`, `target`,
`next`, and `options`. Modal text inputs are limited to 4,000 characters while
the current slash board boundary accepts 6,000, so the future modal path must
cap inline values at 4,000 and direct larger CTK3/Fumen values to the existing
payload-URL path. The eventual implementation also requires optional board
registration, an initial-response preflight router, modal-submit validation,
and new command registration; none of those changes are part of this worker
correction.

References: [Discord interaction responses](https://docs.discord.com/developers/interactions/receiving-and-responding),
[Discord component reference](https://docs.discord.com/developers/components/reference),
and [Discord application commands](https://docs.discord.com/developers/interactions/application-commands).

### Rejected coverage-summary reroute

Do not retry the following routing change without a new representative corpus or
a different coverage data structure. Mapping `sfinder chance/percent` directly to
`failed-queue --failed-count 0` looked attractive because it retains less output
evidence. On the official percent field with the measured Jstris-180 rule, however,
the candidate route took 42.5893 ms versus 37.7679 ms for retained
`pc --objective unique` (+12.8%), used 6,639,029 versus 6,337,325 peak engine
bytes (+4.8%), and grew WASM memory from 7,995,392 to 9,043,968 bytes (+13.1%).
Both covered the same 4,408 / 5,040 patterns. The existing PC route wins on both
time and memory and remains the compatibility mapping.

### Isolated v0.5.1 versus current two-run benchmark

Baseline is tag/commit `v0.5.1` / `9a06e2b99843e87705641d7be7efd2adf57edf39`.
The current artifact includes the post-v0.5.1 worktree plus this audit. The two
release WASM builds used separate Cargo target directories; a shared target had
incorrectly reused a cross-worktree artifact and is not valid benchmark evidence.
SHA-256 and size were:

- v0.5.1: `b0da8d3a04821574c24b0d000024a2a3fead0f508e5e643715c0f6b3343e07c3`,
  3,739,883 bytes;
- current: `faab0d3d8777f87d57a52697beab948ae655f9f4fed6aad97e2fc33e618e2730`,
  3,742,521 bytes.

Each entry is a fresh Node/WASM process, profiling disabled, work budget 8,192,
one worker, and two timed search runs. Load time is excluded. Rows are sorted by
the current mean time.

| Rank | Product family and fixed input | v0.5.1 runs | Current runs | Current mean change | WASM memory | Exact identity |
|---:|---|---:|---:|---:|---:|---|
| 1 | PC/compat percent; official percent field, `P7`, Jstris 180 | 39.47, 48.81 ms | 38.53, 42.18 ms | 40.35 ms, 8.6% faster | 7.625 MiB both | 34 solutions, `cts1:724953d4a7774d5b`, 4,408/5,040 |
| 2 | Build probability; P7 All-Mini+ B2B target, mirror included | 78.87, 92.20 ms | 95.23, 89.33 ms | 92.28 ms, 7.9% slower | 8 MiB both | 4 solutions, `cts1:b7b1b3e3722e8c4f`, 4,704/5,040 |
| 3 | Setup finder; IOTS, longer/all, at most two locks | 35,214.47, 35,501.73 ms | 35,595.78, 35,765.53 ms | 35,680.65 ms, 0.9% slower | 330 MiB both | 1,701 setups, `css1:894530174826d925`, 610,196 families |

The PC improvement is visible in both current runs. Build ranges overlap and the
setup delta is below 1%, so neither is a justification for micro-optimization.
All result identities, counts, completeness flags, and memory sizes match across
versions. Setup's 330 MiB family graph is the clear memory priority, but this
audit did not find a safe representation change; trading correctness for a lower
graph is not permitted. That earlier v0.5.1 table did not rank external
solution-finder or sfinder-man artifacts. The later fixed-queue benchmark below
uses the user-supplied local solution-finder artifact as a read-only performance
reference without copying it into Clearra.

### Fixed-queue score-finder correction and reference benchmark (2026-08-03)

Sfinder-man's `cat-finder` is not an unrestricted forward-damage search. Its
audited positional contract is `cat-finder <fumen> <queue> [clear=4]
[initial_b2b=false] [initial_combo=0] [b2b_end_bonus=0]`. It expands the exact
queue into unique hold reorderings, asks solution-finder `path` for exact perfect
clears with hold already represented by those queues, and applies its Jstris
scoring stage with the requested initial state.

Clearra retains the audited behavior under `score-finder` only; the former raw
compatibility spelling is rejected. The translator validates a one-through-six-row exact
PC target, derives the required piece count from target occupancy, preserves the
fixed queue, hold behavior, rule, and initial B2B state, and lowers to the
existing PC enumeration and Jstris-Ultra scoring pipeline. It selects the CPU
backend for this single fixed-queue workload so WebGPU initialization cannot
dominate it. It no longer lowers to generic `damage`; public `/damage` remains a
separate forward-search command. Initial combo and B2B end bonus currently accept
only zero and otherwise fail closed. This correction reuses the existing typed
PC/scoring engines: it does not port Sfinder DFS, change inverse lock-clear
generation, or add pruning.

The reference input was `v115@9gB8HeB8HeC8EeH8AeC8JeAgH SIJSTLZO 5 true`,
with omitted combo and end bonus both zero. The eight-piece queue produced 112
unique hold reorderings. After a separate warmup, the user-supplied
solution-finder 1.43 JAR took 1,653.353 ms and 1,247.152 ms wall time (mean
1,450.253 ms). Both runs found 14 paths, covered 88/112 reordered queues, and
produced canonicalized mapping SHA-256
`3932AF3D5523921AEF831B4F510C30622A28E8247FA38BA8D2A600BA3CB060EE`.
Raw CSV order differed, so only the canonicalized mapping is identity evidence.
These timings include JVM startup and path CSV/log output but exclude the
Python/Discord wrapper and Node scoring wrapper; they are therefore a strict
path-stage reference, not an end-to-end sfinder-man score time. The launcher hid
the spawned JVM child from the sampler, so no valid Java peak-memory value is
reported.

After the same separate-warmup protocol, the rebuilt Clearra release used its
normal adaptive ceiling of 11 workers and reported
`parallel-immutable-family-queue` in every measured run. Rows are sorted by mean
wall time; SRS is the reference-parity rule, while SRS+ is the omitted-rule
Clearra product default and has a distinct reachable solution set.

| Rank | Engine and rule | Two measured runs | Mean | Engine peak CPU memory | Exact result |
|---:|---|---:|---:|---:|---|
| 1 | Clearra, SRS | 82.856, 73.401 ms | 78.129 ms (18.56x faster than the path-stage reference) | 85.781, 86.215 MiB | 14 solutions, `cts1:d46c226074e08437`, best score 5,250 |
| 2 | Clearra, product-default SRS+ | 130.446, 109.720 ms | 120.083 ms (12.08x faster than the path-stage reference) | 86.749, 88.861 MiB | 15 solutions, `cts1:f38f04765e781dea`, best score 5,250 |
| 3 | solution-finder 1.43 path stage, SRS | 1,653.353, 1,247.152 ms | 1,450.253 ms | unavailable | 14 paths, 88/112 reordered queues, canonical mapping SHA-256 above |

Every Clearra run reported complete counting and scoring, no truncation, the
same per-rule solution hash, and the same best score. The SRS count also matches
the 14-path reference. The 14-versus-15 difference between SRS and SRS+ is a
rotation-rule difference and must not be presented as cross-rule identity.

A core heuristic that automatically forced one worker for this single-fixed-
queue score state was evaluated and rejected. With SRS, two runs took 90.359 and
94.292 ms, retained the same 14 solutions and score, and peaked at 9.173 MiB.
With SRS+, two runs took 128.640 and 115.535 ms, retained the same 15 solutions
and score, and peaked at 9.267 MiB. The memory reduction did not justify the SRS
time regression: automatic worker serialization was removed, and normal bounded
worker-ready dispatch remains authoritative. Do not reapply that heuristic
without new representative evidence. This rejection is separate from the
accepted serial sequencing of multiple Discord automatic-height PC requests.

Before this semantic correction, the old Clearra compatibility route expanded
the request into generic forward damage and did not finish within 166 seconds;
its working set reached 5.76 GiB before the diagnostic was stopped. It is not a
valid result-equivalent benchmark and is recorded only to prevent that incorrect
mapping from being restored.

## WASM Evidence

The product browser build after the fixed-queue change has SHA-256
`36edde9f0a91e2f1fddc2e9ef97f35c3516b5a0040a47aaa3593703b2ba9fe3e`.

The fixed queue `IOTSZLJIOT` on empty 4L retained 54 solutions and
`cts1:190dabaeafc3ba19`. Two optimized runs took 554.79-581.30 ms, compared
with the 596.55-613.36 ms pre-change range. BuildOrder/coverage states fell
from 26,958 to 601, exact reachability states fell from 324,496 to 29,321, and
peak CPU memory fell from 7,635,476 to 7,026,300 bytes. Enabling per-solution
probabilities forced the complete generic path and produced the same count and
hash, while visiting 1,346,717 BuildOrder nodes and 11,784,598 reachability
states.

The small fixed queue `IIOOO` on empty 2L retained 4 solutions and
`cts1:8dc81db9bcd4bab9`; two runs took 74.94-86.12 ms. Empty-hold and occupied-
hold fixtures also matched the forced complete path exactly.

Two-run unchanged-path checks:

| Case | Time | Solutions | Normalized set hash | Coverage |
|---|---:|---:|---|---:|
| PCO | 106.99-122.79 ms | 63 | `cts1:8415f86603b3be9d` | 717 / 840 |
| Tsar Cannon | 113.27-133.51 ms | 42 | `cts1:4996a1501bbb8212` | 4,976 / 5,040 |

The percent continuation fixture materialized all 322,560 patterns, covered
53,640, counted 268,920 failures, and emitted only five requested failed queue
strings. Two runs took 237.11-252.31 ms with 7,185,375 bytes peak CPU memory.
Its result reported `solution_set_materialized=false`,
`solution_count_calculated=false`, and `sample_trace_available=false`.
On the final artifact, two confirmation runs took 276.67-277.42 ms and kept
the same counts and memory peak. A separately capped 5,040-pattern run reported
`failed_pattern_count_complete=false`, so a materialization limit cannot make a
partial failed count appear globally complete.
