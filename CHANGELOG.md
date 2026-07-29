# Changelog

## Unreleased

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
