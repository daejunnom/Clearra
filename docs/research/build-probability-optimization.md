# Build Probability Optimization Record

This record prevents repeating rejected experiments and keeps exactness evidence beside performance results. Raw reports live outside the repository under `%LOCALAPPDATA%/Clearra/reports/build-probability-optimization`.

## Benchmark Contract

- Large case: target `0x000318e3fdffffff`, `P7[^T]4`, SRS+, mirror included, 11 requested workers.
- Small cases: PCO and Tsar Cannon product commands from `scripts/benchmark/run-build-probability-optimization.mjs`.
- Every candidate change is measured twice per case.
- A result is accepted only when count, normalized solution hash, probability, and completeness remain exact.
- Large-search time has priority. A severe small-search regression requires an adaptive or separate path.

Exact reference identities:

| Case | Solutions | Normalized hash |
|---|---:|---|
| PCO | 63 | `cts1:8415f86603b3be9d` |
| Tsar Cannon | 42 | `cts1:4996a1501bbb8212` |
| `P7[^T]4` | 3,018 | `cts1:4a1f5df1599fc97a` |

The large case covers all 1,814,400 patterns with probability 1 and no truncation.

## Decisions

| Change | Small result | Large time, two runs | Decision |
|---|---|---:|---|
| Current-pass lazy construction | Exact | Pre-factorization run remained resource-bound | Keep |
| Factorized exact queue-pattern universe | Exact | 73.119s, 79.676s | Keep; removed 1,814,400-sequence replication |
| Compact finalizer and early session release | Exact | 75.320s, 79.360s | Rejected; slower with negligible retained-byte reduction |
| Same-instance immutable supply catalog sharing across mirror passes | Exact | 67.622s, 64.688s | Keep |
| Omit unused pattern-weight strings from worker partials | Exact | 33.990s, 34.215s | Keep; final coordinator still materializes the required weights once |
| Increase distributed candidate batches from 16 to 256 | Exact | 31.768s, 32.289s | Keep; bounded worker backpressure limits in-flight storage |
| Start verifier initialization without first yielding to the host | Exact | 31.861s, 32.103s | Superseded; worker initialization messages were not actually dispatched before synchronous geometry |
| Dispatch verifier initialization, then overlap it with geometry | Exact | 31.273s, 31.208s | Keep; no retained-memory increase |
| Reuse sequence scratch and fixed seven-piece atom storage | Exact | 25.452s, 26.149s | Keep; removes millions of transient heap allocations per worker |
| Typed final coverage-weight reduction; materialize postprocess strings only for spin | Exact | 21.534s, 21.014s | Keep; removes 1,814,400 weight strings and parse work from buildability finalization |
| Reuse sequence scratch while constructing packing multiset families | Exact; PCO 0.124–0.140s, Tsar 0.131–0.148s | 20.298s, 19.868s | Keep; removes one allocation per concrete pattern per verifier |
| Dispatch batches as soon as the first verifier is ready | Exact | 19.694s, 20.249s | Rejected; indistinguishable from the 19.868–20.298s baseline and adds pool state complexity |
| Replace per-batch verifier timeout timers with one client watchdog scan | Exact | 9 workers: 32.137s, 32.724s; 11 workers: 30.638s, 30.568s | Keep; removes hot-path timer churn without changing search, merge, or pruning |

## Accepted Artifact

- WASM SHA-256: `bc1d0d89477f53bf6f377464347e9eb1d19d0b1506f57229efd14d81139f78a5`
- Source snapshot SHA-256: `42f7b7d888a76c2a4c7567d105ce3544e24fdff4e262c3b386b08353ceb9ce6b`
- Large engine-reported peak: 348,445,702 bytes.
- Browser process memory sampling is diagnostic only; sparse samples are not used as an acceptance oracle.

## 2026-08-02 Runtime-Seam Remeasurement

This measurement is a new same-generation comparison, not an absolute
comparison with the historical 19.868--20.298 second row above. The runtime,
worker lifecycle, and WASM source snapshots differ from that historical
artifact, so only the two rows below are ranked against each other.

- Source snapshot SHA-256:
  `1c6f8e5ef94cbe331fd0053b1416d09db39832ceec71d61ba3b40041bf57bd84`
- WASM SHA-256:
  `399e9408973135a0d385aaf88ab952075e8d13742621d5bae3dcca3563f31a33`

| Rank | Policy | Run 1 | Run 2 | Mean | Engine peak |
|---:|---|---:|---:|---:|---:|
| 1 | 11 total workers (`L-1`, now the default on this host) | 30,637.730 ms | 30,568.405 ms | 30,603.067 ms | 348,445,702 B |
| 2 | 9 total workers (historical constrained policy) | 32,136.685 ms | 32,724.280 ms | 32,430.482 ms | 348,445,702 B |

Both policies preserve the 3,018-solution identity, full 1,814,400-pattern
coverage, probability completeness, and no-truncation result. Nine workers are
5.97% slower but cap total WASM worker instances 18.18% below eleven. Browser
process memory samples were unavailable; no RSS reduction is inferred from the
identical engine peak. This comparison is retained to avoid repeating the
experiment; nine is no longer a foreground execution cap. Automatic execution
now uses `L-1`, while explicit full-CPU mode uses `L` and cannot exceed it.

The benchmark runner and this orchestration script now fail immediately unless
`--browser-root` contains a built `index.html`. Passing
`apps/clearra-web/static` previously served `/` as 404 and produced no progress
or result until the 15-minute case timeout; the correct product harness root is
`scripts/benchmark/wasm-product-browser/dist` after its Vite build.

## Remaining Experiments

Run these independently and retain only measured wins or justified adaptive paths:

1. True cross-worker catalog sharing requires a shared-linear-memory WASM build; a `SharedArrayBuffer` that is copied into separate WASM memories does not satisfy the memory contract.
2. Mirror work is already naturally overlapped when the producer advances to pass 2 while pass 1 verifier batches drain. Do not add a duplicate speculative pass.
3. Backlog-driven worker activation needs a medium-size benchmark where the serial path is bypassed but all workers are not immediately useful.

Do not retry the rejected compact-finalizer representation without a materially different ownership design.
Do not retry ready-worker dispatch without command-specific evidence of materially skewed verifier initialization.

## Validation

- `cargo fmt --all -- --check` passes.
- `cargo check -p clearra-core-executor -p clearra-wasm` passes.
- `cargo test -p clearra-coverage -p clearra-supply -p clearra-core-executor --lib --no-run` compiles the native test binaries.
- Browser-product WASM runs preserve all three reference counts and normalized hashes.
- A build-probability spin smoke run completes with exact coverage, confirming that spin requests still materialize postprocess weights.
- Windows Application Control blocks execution of newly generated native test executables with OS error 4551. The binaries were not bypassed, re-signed, or executed through an alternate native surface.
