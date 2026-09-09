# Release Rust shard contention A/B (2026-09-09)

## Scope and authority

This note records local and non-publishing CI evidence for
`codex/release-rust-shard-contention`. It does not authorize release,
publication, or deployment. The canonical product gate remains
`.github/workflows/release-cli.yml` on exact `main`.

No Rust test is removed by this candidate. The compiled inventory contains
1,575 tests: 1,563 pass and 12 remain explicitly ignored by their source
contracts.

## Completed baseline decomposition

The completed canonical Rust job in run
[`34316349210`](https://github.com/daejunnom/Clearra/actions/runs/34316349210)
at source `97b6654160b35a08f9b7954fb388b3248466c213` took 928.26 seconds.
Its measured critical-path components were:

| Component | Time | Interpretation |
| --- | ---: | --- |
| Native preparation plus Rust/Cargo compile and link | about 709-716 s | 76-77% of the job; the primary bottleneck |
| Rust harness execution | 155.55 s | App, executor, and coverage account for 153.80 s |
| ProductE2E process cases | 25.58 s | The separate CLI build inside ProductE2E took another 155.03 s |
| Render test execution | about 0.06 s | Its separate compile/link took 13.27 s |
| Bootstrap, reporting, retention, and other overhead | about 30.9 s | Includes buffered output and artifact bookkeeping |

The baseline restored a roughly 718 MiB cache but still refreshed the native
link fingerprint and spent 533 seconds in the Rust test compile. The raw target
cache therefore did not establish a useful warm local-crate build for that run.

## Rejected unsafe partition

Run
[`34334424373`](https://github.com/daejunnom/Clearra/actions/runs/34334424373)
used test-name filters and two in-process test threads for the remaining App and
core-executor tests. It failed with shared-resource contention in both
harnesses. A test body does not need to call the guard directly to reach the
process-global lease: normal product calls can acquire it below the test
boundary. The repository must therefore not classify these packages from test
name prefixes alone.

## Isolated-harness candidate

Run
[`34337155110`](https://github.com/daejunnom/Clearra/actions/runs/34337155110)
completed successfully with the following policy:

- Compile all nine library harnesses in one Cargo inventory operation.
- Run the App, core-executor, native FFI, and WebGPU harnesses first. Each
  process uses `--test-threads=1`, while at most two independent harness
  processes run concurrently.
- Run domain, coverage, objectives, scoring, and postprocess afterward, one
  harness at a time with at most two test threads.
- Continue every runnable harness after a failure, then fail once with the
  complete partition failure list.
- Parse each harness result and require its passed-plus-ignored count to equal
  the compiled inventory. Missing executables, duplicate names, omitted
  partitions, or malformed summaries fail closed.

The candidate's measured phases were:

| Phase | Wall evidence |
| --- | ---: |
| Compile | 612.920 s |
| App isolated process | 85.569 s |
| Core-executor isolated process | 89.288 s |
| Native FFI isolated process | 0.236 s |
| WebGPU isolated process | 0.320 s |
| Five parallel-safe harnesses combined | about 21.062 s |

Because the two long isolated processes overlap, the test-execution wall is
about 110.7 seconds rather than the baseline 155.55 seconds, a directional
reduction of about 28.8%. Compile/link remains much larger than test execution.

## Global-resource reduction

Two WebGPU workload-selection tests only evaluate a pure deterministic selector
and never acquire or release shared compute state. Their unnecessary
`score_resource_test_guard` calls are removed. A source-level regression check
keeps those pure tests outside the lease.

The other guarded tests and the four whole-harness isolation decisions remain.
They execute native, GPU, search, application, or FFI product paths that can
reach mutable process-global state. Removing those guards or enabling
in-process parallelism without a new ownership design would repeat the observed
contention failure rather than remove test overhead.

## Native cache reproducibility candidate

The Rust/native link fingerprint contains the exact static-library digest.
MSVC objects and archives were not byte-reproducible by default, so identical C
sources rebuilt at the same path caused an avoidable fingerprint refresh.
`/Brepro` is now applied to both compilation and the static librarian for the C
core and test oracle.

Two complete local clean rebuilds at the same output path produced the same
archive SHA-256:

`e44eee1b05792b297dfecccadcc3bb033469c40ff9084c0cc950ee427ae06c5f7f8b2e5`

Different output paths are not claimed to be interchangeable. GitHub Actions
uses a stable native build path, so the same-path property is the relevant
contract.

## Hosted A/B acceptance criteria

The non-publishing branch
`codex/release-rust-shard-contention-ci-v3` must complete twice:

1. The first run validates the isolated scheduler and seeds an archive produced
   with `/Brepro`.
2. A source-identical follow-up commit restores that run's cache. It must report
   `native-link-cache=reused`, preserve all 1,575 inventory entries, and finish
   with no shared-resource contention.

Compile time and cache size are recorded for both runs. A cache hit alone is not
enough to claim a total improvement: hosted wall time must improve, and exact
test counts and failure semantics must remain unchanged.

## Larger remaining opportunities

The measured job shows that further material improvement belongs to build
architecture, not wider unsafe test threading:

1. Evaluate a content-addressed compiler cache whose key includes exact C
   source/configuration/toolchain/archive identity.
2. Evaluate a non-authoritative, source/run/attempt-bound native CLI producer in
   parallel with RustExactTests so ProductE2E can verify and execute exact bytes
   instead of rebuilding for roughly 155 seconds.
3. Independently A/B CI-only debug-symbol reduction and test-profile
   optimization; do not mix these candidates because compile-time and runtime
   effects need separate attribution.
4. Stream Cargo output while retaining the exact marker buffer to expose build,
   harness, drain, and retention time. This is mainly diagnostic and is not
   expected to remove the primary bottleneck by itself.

Background Cargo builds sharing one target directory, unsealed reuse of a
binary from another source identity, immediate removal of
`--test-threads=1`, and arbitrary cache-size increases remain rejected.
