# Changelog

## Unreleased

## 0.6.3 - 2026-08-05

- Consolidated Discord ingress behind one Gateway process and delegated heavy
  searches to the bounded Cloud Run job service, removing the extra relay hop.
  Private administration code, credentials, and deployment overlays remain
  excluded from the public source and release artifacts.
- Reworked Discord commands around localized English and Korean help, structured
  per-command Modals, CTK3/Fumen field inputs, permission-gated server and
  channel controls, and compatible ordinary-message commands. Improved command
  registration recovery so stale Discord command caches fail closed and refresh
  without exposing unrelated runtime details to users.
- Improved Discord CTK3 and GIF presentation with GUI-matched piece colors,
  connected garbage rendering, reply-based original-file retrieval, bounded
  attachment lifetimes, and consistent no-solution responses that omit
  unnecessary files.
- Added dedicated structural searches for T-Spins, T-Spin Minis, All-Mini(+),
  and All-Spin(+), together with additional forward-search command surfaces.
  Accelerated their shared traversal and result paths while retaining the
  existing highest-damage, general-spin, PC, build, and setup search semantics
  and exact pruning boundaries.
- Applied complete-row normalization once at search ingress so initially full
  rows are cleared before search and exported CTK3 results reflect the normalized
  field. Image rendering intentionally preserves the supplied rows rather than
  applying gameplay line clears.
- Extended the native CLI with the same bounded multi-worker execution contract
  used by the WASM runtime, including automatic allocation, explicit full-CPU
  use, and a hard cap at the logical processors available to the host.
- Improved the CTK GUI drawer with automatic tetromino color inference,
  discoverable color shortcuts, working `.ctk3` drag-and-drop and keyboard
  controls, up to 100 preview frames in either direction, and stable frame
  navigation that no longer shifts the surrounding page.
- Hardened the public operational boundary with resolved command and timing
  fields plus bounded diagnostic detail, while keeping credentials, private
  management policy, and administrator-only implementation outside public logs
  and release artifacts.

## 0.6.2 - 2026-08-02

- Fixed the Discord/Cloud Run worker-authority boundary so a single automatic
  session delegates the final worker count to the native Clearra hard limit
  while preserving the explicit full-CPU policy. This avoids forcing Node's
  affinity-visible count into a Rust runtime that reports a lower effective
  Linux parallelism limit; explicit and multi-session allocations remain
  bounded numeric requests.

## 0.6.1 - 2026-08-02

- Kept queued interaction deadlines and Discord REST request timeouts alive
  until their pending work settles, including under the Node.js 22 event-loop
  behavior used by the release workflow.

## 0.6.0 - 2026-08-02

- Added `/help` and structured Discord command inputs (`field`, `next`,
  `remaining`, and `scope`). Optional search settings now use one bounded,
  command-specific `key=value` allow-list instead of an unrestricted argv
  string. CTK3 is decoded directly through the npm package and is never
  re-encoded as Fumen; Fumen is decoded independently. Both formats require one
  static page and project every non-empty input color to canonical occupancy
  masks. `/cover` now accepts separate `base` and non-overlapping `target`
  delta fields plus `next`, then compiles to the existing build-probability
  request. The legacy raw CLI colored-Fumen cover boundary remains available.
- Made CTK3 the active Discord solution-document output. Generated tetrominoes
  retain piece colors and inherited initial occupancy is encoded as `G`; the
  Discord result path does not emit Fumen. These boundary changes do not alter
  PC/build engines or pruning. The discarded CTK input piece-identity,
  grey/colored pair, and multi-page cover designs must not be reapplied.
- Replaced the Discord `/clearra` and `/view` surface with individual represented
  Sfinder-compatible slash commands and made the direct signed Cloud Run
  interaction service the only active slash ingress. Oracle, ordinary-message
  commands, and image rendering remain disabled future proxy seams.
- Added bounded per-instance serial Clearra execution with queue-inclusive
  interaction deadlines, source-built Linux CLI packaging, local one-shot
  command registration, and a Tokyo (`asia-northeast1`) Cloud Run deployment
  contract: 0--4 instances, concurrency 1, 8 vCPUs/16 GiB per instance, CPU
  throttling disabled, and startup CPU boost enabled.
- Fixed both service-level and revision-level maximum instances at four so the
  platform's lower revision default cannot silently cap the service at three.
- Added `/health` as the Cloud Run-safe health endpoint while retaining
  `/healthz` only as a local compatibility alias because Cloud Run reserves
  some paths ending in `z`.
- Added a Windows PowerShell 5.1/7-compatible masked Discord command-registration
  wrapper and fail-fast credential diagnostics so unsupported `-MaskInput`
  syntax cannot silently leave `DISCORD_TOKEN` unset.
- Restored the Tauri desktop entry points for all six product tools and routed
  their forms through typed native requests instead of leaving the desktop on a
  solver-only surface.
- Unified worker allocation across native, browser, and bot hosts: automatic
  desktop work reserves one logical processor, explicit full-use and Discord or
  Linux hosts may use all logical processors, and every path is hard-capped at
  the processors visible to that host.
- Hardened browser and desktop worker lifecycles with bounded artifact waits,
  verifier heartbeats and watchdogs, terminal cleanup, and prewarm ownership so
  failed joins or stale workers cannot leave the five product flows loading
  indefinitely.
- Improved Setup execution and presentation with bounded parallel progress,
  lower duplicate-state memory pressure, stable path-detail ownership, and
  retained exact reachability and pruning rules.
- Hardened transient build and artifact-cache boundaries so repeated WASM,
  desktop, and benchmark runs reuse managed locations without accumulating
  stale generated output.
- Corrected Discord result completeness reporting to ignore inactive optional
  post-processing stages and propagated the configured output ceiling through
  the optional remote job seam.

## 0.5.1 - 2026-08-01

- Added native Sfinder-compatible command roles and a compact standalone PC
  solver page while retaining Clearra's exact search, queue grammar, and CTK3
  or Fumen result export.
- Added first-class `.ctk3` file encoding, decoding, downloading, attachment
  handling, and page-wide document paste routing across the npm package,
  browser GUI, and Clearrabot.
- Strengthened exact tiling identity, colored Fumen reconstruction, setup and
  BuildUp dependency handling, and worker execution contracts without enabling
  the optional tablebase or dependency analysis in bot searches.
- Added a slash-command-only Google Cloud Run adapter for Clearrabot with raw
  Discord Ed25519 request verification, immediate deferred responses, intent-0
  Gateway fallback, idempotent HTTP jobs, and a disabled-by-default relay
  boundary for future ordinary-message handling.

## 0.5.0 - 2026-08-01

- Added Clearrabot with native Clearra command execution, bounded per-session
  worker allocation, search timeouts, CTK3/Fumen detection, standalone GIF
  rendering, and direct or attachment-based viewer links.
- Added an explicit tiling-only objective across CLI, browser, desktop, and
  bot surfaces, including incompatible-option validation and a clear warning
  that geometric tilings may not be buildable.
- Added a compact paged tiling-solution store and tiling-only root workers so
  large result sets can be merged, paged, rendered, and exported without
  routing every candidate through BuildUp or retaining expanded wire records.
- Improved distributed WASM execution with bounded segmented results,
  fail-operational worker retries, terminal resource cleanup, and exact
  canonical merging that remains stable across worker completion order.
- Improved large CTK3/Fumen export and document loading, browser viewer query
  handling, lazy solution galleries, and user-facing progress reporting.
- Reused external build caches and overwrite-in-place transient artifact slots
  to prevent repeated builds and runtime comparisons from accumulating large
  local histories.

## 0.4.0 - 2026-07-31

- Added the compact, lossless CTK3 field-document format, CTK workspace,
  Fumen-compatible conversion API, comments, operations, multi-page documents,
  lazy decoding, and asynchronous large-result export.
- Prepared the public `ctk3` package with ESM, CommonJS, and TypeScript entry
  points plus compatibility tests and package documentation.
- Unified product progress and result surfaces, removed developer-only report
  details from the normal GUI, and added incremental solution loading and
  full-result copy controls.
- Improved setup candidate streaming, segmented worker completion, and
  forward-search parallel execution while retaining exact result identities.
- Distinguished user cancellation, forced termination, and runtime failure in
  browser WASM workers, and made every product search and export path release
  owned workers and buffers on terminal exit.

## 0.3.1 - 2026-07-30

- Replaced the setup-wide partial BuildUp graph with bounded candidate
  streaming, shared exact suffix coverage, compact representative solution
  reconstruction, and balanced multi-worker completion to reduce setup-search
  peak memory and long-tail execution without weakening exact coverage.
- Added exact setup residue support for one duplicated piece kind. The duplicate
  is derived as the initial hold while the bag round remains determined by the
  residue supply, and invalid duplicate combinations remain rejected.
- Made tablebase changes during an active browser search apply to the next job,
  preventing worker teardown and stalled searches when the control is toggled.
- Published browser WASM artifacts from a completed staging generation, with
  manifest-last atomic replacement, fresh-response retry, byte-length and
  SHA-256 verification, and build-time integrity checks to prevent truncated
  modules from reaching the runtime.

## 0.3.0 - 2026-07-29

- Added an opt-in compact PC4 tablebase for compatible empty 10x4 searches,
  with bounded artifact size, schema and digest validation, lazy browser
  loading, explicit CLI installation, exact-dead-only pruning, and exact-search
  fallback for every unknown or unsupported state.
- Corrected Setup Finder continuation coverage so each candidate retains its
  exact post-setup hold, queue, bag-boundary provenance, line-clear state, and
  complete PC solution family rather than substituting a fresh supply.
- Shared exact suffix and observation-language results across Setup candidates,
  preserved symmetric and multi-parent solution families, and connected the
  corrected path through single-worker, multi-worker, browser WASM, CLI,
  desktop host, reports, and on-demand PC solution rendering.
- Added an opt-in piece-dependency DAG for PC and Build Probability BuildUp.
  The beta path preserves every canonical multi-parent convergence, fails open
  to the baseline search when unavailable, and leaves kick-sensitive
  dependencies to the exact selected-rule reachability engine.
- Added SRS+ regression coverage for a T lock whose legal kick path depends on
  a previously placed J piece, preventing geometry-only dependency analysis
  from replacing exact kick-table reachability.
- Refined English and Korean product controls, responsive product-mode
  navigation, progress reporting, tablebase readiness, queue-visibility
  wording, and Setup result semantics without exposing implementation details.

## 0.2.7 - 2026-07-27

- Added Fumen as the default solution-copy format across PC, Setup, Build
  Probability, Maximum Damage, and Spin Finder results while retaining CTK as
  a user-selectable compact format and preserving piece colors in Fumen output.
- Removed the GUI's eight-piece pattern length policy from the forward-search
  CLI and shared execution engine, keeping the limit only in the browser GUI
  while fixed queues remain unrestricted.
- Removed the hidden 255-edge piece-language range failure exposed by
  `visible-7` searches, retaining compact indexing for ordinary nodes and using
  an exact sorted-edge fallback only for high-fanout nodes.
- Added regression coverage for long forward-search patterns and high-fanout
  piece languages, including the eight-piece visible-queue product path.

## 0.2.6 - 2026-07-27

- Added an exact `visible-7` future-queue knowledge policy for PC and Setup
  search while retaining the full-future Oracle as the compatibility default.
- Required queues with the same current hold and visible seven-piece prefix to
  choose one shared placement/hold action, with exact branching only when
  hidden pieces become visible.
- Connected queue knowledge through CLI, browser WASM, desktop host, GUI,
  continuation tokens, reports, documentation, and architecture validation.
- Kept hidden suffixes in the complete pattern universe, routed limited-
  observation searches through the required global language finalizer, and
  reported unsupported per-solution objectives without Oracle substitution.

## 0.2.5 - 2026-07-27

- Preserved a single explicit Setup Finder residue as the guaranteed leading
  supply prefix, with regression coverage from browser command parsing through
  the exact pattern transition.
- Canonicalized on-demand PC completion paths by exact placement set and
  deleted-row state, removing duplicate solutions that differed only in legal
  operation order while retaining genuinely distinct clear states.
- Clarified the probability distinction between a guaranteed one-piece residue
  and selecting the same piece from an unordered seven-piece residue.

## 0.2.4 - 2026-07-27

- Restored Queue-Based Setup Finder semantics so observed next-bag pieces are
  available to a setup without becoming mandatory locks or an exact terminal
  inventory.
- Added an independent next-cycle remaining-inventory constraint that can be
  combined with either shape-oracle or queue-based setup search without
  renormalizing the original pattern universe.
- Corrected setup Build, Joint, and Conditional probability aggregation by
  keeping same-board states with different concrete tilings, deleted rows, or
  placement depths independent through exact coverage evaluation.
- Added opaque exact-state setup identities so on-demand PC completion paths
  continue from the same fixed tiling measured by the selected setup card.
- Updated CLI, browser WASM, multi-worker transport, validation, documentation,
  and English/Korean GUI contracts for the corrected setup model.

## 0.2.3 - 2026-07-26

- Corrected Queue-Based Setup Finder semantics so the requested next-cycle
  remaining inventory constrains the exact terminal hold and bag suffix without
  forcing every observed piece into the setup or renormalizing its probability.
- Removed implicit occupied-hold expansion from the product UI while preserving
  explicit initial-hold analysis in the CLI with exact duplicate-piece and bag
  provenance validation.
- Added configurable one-through-ten-piece setup results and on-demand rendering
  of the actual PC completion placements from a selected setup.
- Added the exact Jstris 180 rule profile across Rust, C, WASM, CLI, Tauri, and
  browser controls: standard SRS quarter turns, ordered two-offset half turns
  for I/J/L/S/T/Z, and no O rotation transitions.
- Expanded executable reachability, wire identity, compact C ABI, browser WASM,
  validation, documentation, and English/Korean GUI coverage for the new rule
  and corrected setup contracts.

## 0.2.2 - 2026-07-26

- Added exact queue-based Setup Finder input, observed-piece consumption, and
  bag/hold provenance so visible QB pieces can participate in the setup without
  being silently reordered or left unconsumed.
- Added user-selectable setup length preferences and deterministic priority
  tie-breaks while preserving complete candidate counts, hashes, and exact
  build/joint probability semantics.
- Added on-demand all-solution expansion for a selected setup. The initial
  result remains compact; selecting a setup enumerates every distinct legal
  placement and hold path with exact backward PC liveness.
- Removed duplicate coverage traversal from selected-setup expansion, keeping
  measured runtime and browser memory within the release performance budget.
- Expanded CLI, browser WASM, Tauri host, validation, wire, documentation, and
  English/Korean GUI contracts for the complete Setup Finder feature set.

## 0.2.1 - 2026-07-26

- Corrected Setup Finder residue compilation so unordered remaining pieces,
  explicit initial hold, bag epoch, and cycle boundaries retain their exact
  supply semantics.
- Added automatic cycle derivation, exact hold-slack handling, and the
  seventh-cycle bag-reset policy without treating residue input as a fixed
  queue.
- Added All, Build Probability First, and PC Probability First setup candidate
  views across CLI, browser WASM, desktop host, and the English/Korean GUI,
  while keeping full candidate counts and hashes independent of presentation.
- Preserved complete setup placement replay in result boards and kept serial
  and segmented multi-worker ordering, limits, and wire transport equivalent.
- Expanded exact setup regressions for IOTS residue, initial hold, cycle
  boundaries, candidate ranking, and parallel result merging.

## 0.2.0 - 2026-07-26

- Added the exact 4-line Setup Finder product path, deriving buildable partial
  setups from inverse lock-clear geometry families instead of synthetic shelf
  placements or per-solution continuation searches.
- Added residue and initial-hold condition compilation, exact build/joint/
  conditional pattern coverage, representative setup boards, and complete
  setup reports across CLI, browser WASM, and desktop request surfaces.
- Added segmented multi-worker setup evaluation with canonical result merging,
  cooperative progress and cancellation, bounded worker memory accounting, and
  a serial fallback for small searches.
- Connected the Setup Finder workspace between PC Search and Build Probability
  with English and Korean controls, validation, result rendering, and stable
  query semantics.
- Improved inverse geometry and reachability reuse while preserving exact
  candidate, coverage, hold, kick, and line-clear identities.
- Parallelized browser verifier prewarm, overlapped CPU and GPU preparation,
  reused the warmed worker pool for the first search, and removed a WASM timing
  trap from WebGPU initialization.

## 0.1.5 - 2026-07-25

- Corrected finite fixed and pattern queue handling so an exact terminal held
  piece can be released without inventing an unknown standard-bag draw.
- Kept terminal hold projection separate from standard-bag complement
  projection throughout BuildUp, coverage, replay, spin, and scoring paths.
- Restricted witness-only BuildUp verification to a single concrete pattern,
  preventing a representative witness from hiding solutions in grouped queue
  languages.
- Added exact WASM regression coverage for the finite `[LOJ]!` build
  probability case and preserved complete count and probability reporting.

## 0.1.4 - 2026-07-24

- Extended the selected-spin-profile B2B preservation policy to PC and build
  probability searches, including exact single- and multi-worker WASM paths.
- Added Guideline and Jstris Ultra score tables behind the shared score-profile
  contract and connected score selection through CLI, WASM, and GUI requests.
- Preserved command, queue pattern, initial hold, kick rule, spin profile, and
  score profile identities across browser and Tauri request normalization.
- Fixed initial-hold scenario compilation, exact spin-aware BuildUp filtering,
  and unsupported kick-profile handling without weakening PC existence search.
- Added clearer initial-hold guidance and corrected build-search control
  spacing in the English and Korean GUI.

## 0.1.3 - 2026-07-23

- Added an optional B2B-preserving line-clear policy to forward damage and
  spin searches.
- When enabled, the search accepts only perfect clears, Quads, and spins
  recognized by the selected spin profile while retaining zero-line placements.
- Connected the policy to browser commands, English and Korean GUI controls,
  and exact single- and multi-worker WASM execution.

## 0.1.2 - 2026-07-23

- Preserved the held piece after the visible queue is exhausted so forward
  damage and spin searches enumerate legal hold continuations.
- Separated regular All-Spin and All-Spin+ awards from All-Mini profiles and
  restored regular non-T spin damage without changing SRS+ kick behavior.
- Added exact and minimum cleared-line requirements to Spin Finder, including
  `N+` command syntax, complete continuation search, multi-worker transport,
  and stable numeric selection in the English and Korean GUI.

## 0.1.1 - 2026-07-23

- Added GitHub Pages delivery for the WASM GUI and tag-owned standalone Linux
  CLI, Windows CLI, and Windows SvelteKit/Tauri GUI executables. GitHub's
  release-asset digest replaces duplicate checksum sidecars.
- Routed CLI, desktop, and WASM requests through the typed `AppRequest` /
  `AppResponse` application boundary and canonical `SearchProblem` compiler.
- Connected product search to C Geometry Skeleton Exact Cover, host reduction, and C
  BuildUp BFS with PieceSource and HoldAutomaton supply verification.
- Enforced candidate identity, pattern-specific BuildUp coverage, PatternBitSet
  union probability, proof-authorized pruning, and incomplete resource reports.
- Made native-core unavailability explicit as `E_NATIVE_CORE_UNAVAILABLE`;
  product requests no longer synthesize fixture candidates, traces, or complete
  resource reports.
- Separated search and PostProcess ownership for replay, spin interpretation,
  score matrices, rendering, and backend trust reporting.
- Reduced the desktop product to the Tauri -> `clearra-gui-host` -> `clearra-app`
  route and kept unsupported native GPU capabilities explicit.
- Completed desktop async jobs with ordered event batches, cooperative native
  cancellation, terminal worker cleanup, and consecutive-job reuse.
- Consolidated Cargo tasks onto one external target tree and added a dynamic
  Windows UMCI preflight that blocks unsupported Tauri compilation before a
  generated build-script can trigger application-control denial.
- Routed GPU backend selection through the native C capability query, preserved
  device/kernel fallback reasons, and advanced the C ABI contract to version 16.
- Ranked GPU adapters into explicit Clearra product indices, reported physical
  vendor/device identity, and kept small automatic workloads on the measured
  CPU path until GPU startup and execution are benchmark-qualified.
- Batched compatible WebGPU packing families and added a worker-local exact
  BuildUp reachability cache whose collisions or allocation failures never
  authorize candidate removal.
- Made BuildUp root transitions demand-driven and included all concurrently
  retained BuildUp worker workspaces in product CPU-memory reports.
- Removed transition-era validators, task labels, helper names, and speculative
  stable ABI values from the current product surface.
- Kept decode-only support for version 1 `pc1` and `sc1` continuation tokens;
  current encoders emit version 2 tokens and the decoder remains outside the
  solver hot path.
