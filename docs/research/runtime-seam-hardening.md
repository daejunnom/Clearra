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
