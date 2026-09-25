# Release build architecture A/B (2026-09-09)

## Scope and authority

This note closes the two non-publishing candidates recorded on
`codex/release-build-architecture-ab`:

1. a content-addressed Rust compiler cache keyed by tracked C source,
   configuration, toolchain, and native archive identity; and
2. one exact source/run/attempt-bound CLI producer that runs in parallel with
   RustExact and supplies verified bytes to ProductE2E.

Neither workflow, artifact, nor result is release authority. The A/B workflow
has no deployment identity, write permission, canonical acceptance artifact,
or publication step. After the measurements below, its automatic branch-push
trigger was removed; another experiment now requires an explicit dispatch.

The earlier raw target-directory cache experiment and baseline decomposition
remain in `release-rust-shard-contention-ab-2026-09-09.md`; they were not rerun.

## Baseline used for comparison

Canonical run
[`34316349210`](https://github.com/daejunnom/Clearra/actions/runs/34316349210)
at source `97b6654160b35a08f9b7954fb388b3248466c213` took 928.26 seconds in
the Rust shard. Its relevant measured components were:

| Component | Baseline |
| --- | ---: |
| Rust test compile | 533.00 s |
| ProductE2E's second CLI build | 155.03 s |
| ProductE2E cases plus two terminal adapters | about 31.64 s |

This is a directional baseline rather than a same-runner statistical claim.
The acceptance decision below therefore relies on both the measured wall time
and whether the candidate actually exercised the intended mechanism.

## Candidate A: content-addressed compiler cache

The experiment created a namespace from the exact tracked native inputs,
configuration, compiler/linker identity, and archive digest, then set
`RUSTC_WRAPPER=sccache`. It did not restore a raw Cargo target directory.

| Phase | Run | RustExact wall | Compile phase | sccache requests / hits / misses |
| --- | --- | ---: | ---: | ---: |
| Cache seed | [`34349610002`](https://github.com/daejunnom/Clearra/actions/runs/34349610002) | 816.352 s | 664.712 s | 0 / 0 / 0 |
| Source-tree-identical warm | [`34351448956`](https://github.com/daejunnom/Clearra/actions/runs/34351448956) | 722.783 s | 567.526 s | 0 / 0 / 0 |

Both runs passed the full 1,575-entry inventory: 1,563 tests passed and the
same 12 source-declared tests remained ignored. However, sccache reported zero
compile requests, zero compilations, and zero cacheable or non-cacheable calls
in both phases. The 97.186-second compile difference is therefore ordinary
hosted-run variation, not a cache hit. A cache namespace and restored backend
are not evidence of compiler reuse when the wrapper intercepted nothing.

Decision: **reject this implementation**. Do not merge it into the canonical
gate or represent the warm run as a speedup. A future attempt must first prove,
with a tiny exact Cargo probe in the same process-launch path, that the wrapper
receives requests; only then may it repeat the full inventory. The native input
identity remains a useful design boundary, but it has no measured performance
authority by itself.

## Candidate B: exact CLI producer and verified consumer

The producer in run
[`34349056513`](https://github.com/daejunnom/Clearra/actions/runs/34349056513)
built the ProductE2E CLI in parallel with the compiler-cache job. It sealed the
binary, the exact native identity, source SHA, producer run/attempt, build
recipe, size, and SHA-256 in a non-authoritative artifact.

| Phase | Wall |
| --- | ---: |
| Producer build plus receipt sealing | 169.904 s |
| Recorded-artifact ProductE2E plus both terminal adapters | 31.397 s |

The initial consumer had a harness-only CTK3 workspace-name typo. The producer
itself succeeded, so the later source-bound consumer did not rebuild it. Run
[`34351448956`](https://github.com/daejunnom/Clearra/actions/runs/34351448956)
checked out the exact producer source, downloaded only producer run
`34349056513` attempt `1`, verified the receipt and binary bytes, and passed
ProductE2E plus both Discord/UI terminal adapters. The 31.397-second execution
is consistent with the baseline's approximately 31.64-second product-test
work, while the 155.03-second second build is absent.

The producer is slower than the old 155.03-second in-shard build by itself,
but it finishes far earlier than RustExact. The value is topology: its build is
off the Rust shard's serial tail. If integrated, the Rust job must consume the
already completed artifact and remain the sole ProductE2E evidence owner.

Decision: **accept the architecture as a future integration candidate**, not
as release authority and not as an unconditional main-branch merge. Integration
must preserve these conditions:

- producer and consumer use the same exact source SHA and workflow run/attempt;
- the receipt binds package, binary, feature set, profile, target triple,
  native identity, file size, and SHA-256;
- missing, extra, foreign, or modified files fail closed without a local
  fallback rebuild;
- the producer emits no pass marker, shard report, or canonical acceptance;
- ProductE2E remains the only process-evidence owner; and
- producer completion is a prerequisite for consumption, while producer work
  remains parallel to RustExact.

## Closed outcome

The content-addressed cache implementation is rejected because it did not
intercept a single compilation. The exact CLI producer/consumer is the only
candidate that removed measured serial work while preserving functional
evidence: it eliminates the roughly 155-second duplicate CLI build from the
Rust shard's tail at the cost of a parallel producer job. No A/B result in this
branch changes product behavior or production authority.
