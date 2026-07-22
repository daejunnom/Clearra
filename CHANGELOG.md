# Changelog

## Unreleased

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
