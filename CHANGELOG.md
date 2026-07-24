# Changelog

## Unreleased

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
