# Cloud CLI parity and minimum follow-up — 2026-09-07

## Scope and release decision

The user explicitly moved the minimum first-canonical runtime target to a P2
v0.8.0 hotfix, while asking that improvement and A/B continue alongside release.
Product correctness remains a release prerequisite. The diagnostic work below
does not replace the single canonical acceptance run, production recovery,
image/source identity checks, or live deployment observation.

Keep the cached-pivot experiment on the separate
`codex/minimum-hotfix-pivot-20260907` worktree/branch. Do not merge it or enable
warm-start merely to publish v0.8.0. The experiment retains exact minimum proof
and numeric-lexicographic canonical selection; later tied sets remain lazy.

## Completed diagnostic evidence

The source-bound Linux diagnostic on commit
`7fcfa462ddfd4c611f72979dab8d0a842cbb7cdd`, Actions run
`34045835905`, compared warm seed off/on in the same binary with four workers.
This was one paired observation, not a repeated benchmark or GUI acceptance.

| Stage, milliseconds | Warm off | Warm on |
| --- | ---: | ---: |
| Minimum cardinality proof | 9,504 | 8,197 |
| First exact canonical set selection | 12,698 | 11,583 |
| Sum of these two stages | 22,202 | 19,780 |

Both completed with cardinality 25 and the same first canonical set. The
observed sum reduction is about 10.9%; it does not establish a 3-second GUI
result. The default policy remains unchanged. Matrix export, row binding and
this A/B job passed; this says nothing about other jobs in the same run.

The same run's Windows regression step later completed with two failed
selections and no compile-blocked selections. One was the one-I BuildCover
fixture expecting one source despite its default horizontal-mirror policy:
the actual 686 WASM returns two sources for edge target `0xf`, one for centered
target `0x78`, and a selected minimum of one in both cases. The fixture now
checks both, without adding the unsupported BuildCover `--no-mirror` option.
The other selected the nonexistent `pc_replay_page_source::tests` module and
correctly failed its nonempty-test guard; its real `memory_tests` module is now
selected. Neither correction is a waiver of product correctness. The unused
Scenario score fixture warning is addressed by a real synthetic-authority
continuation parity case, not a broad warning suppression. Revised native
tests still require execution.

The isolated 4195 browser audit with source-bound `686067a` WASM completed Build
all-solutions for the supplied CTK3/P7/Jstris180 fixture in 0.7 seconds fresh and
0.4 seconds warm, with 246 solutions and 5,040/5,040 successful patterns. Build
minimum for that same input failed after Geometry/verification, before its
minimum wave. The actual WASM reproduction rejected source evidence with
`distributed_build_cover_source_evidence_rejected`. Therefore all-solutions
success must not be reported as evidence that Build minimum also works.

The failure was traced to per-solution probability evidence: compact uniform
weights used multiplication in the producer, while the reducer reconstructed
them by ordered floating-point addition and required exact equality. The
correction must share the evidence accumulation order, not weaken comparison
with an epsilon or bypass the source guard. Native/GUI regression evidence for
that correction must be recorded after it is actually run.

## CLI versus Discord computation contract

The Cloud job service invokes the native Rust CLI. The
`wasm-cpu-runtime,webgpu-search` build feature names do not imply a WASM VM on
this path. Compare the same binary SHA, source/engine identity, actual prepared
arguments and worker policy, rather than comparing local n-1 defaults with
Cloud's eight-worker setting or binaries compiled by different toolchains.

`benchmark-cloud-cli-parity.mjs` runs the actual loopback Job Service, Runner
and Job Executor, and the same executable directly. Three fixtures are fixed:
PC all-solutions, PC minimum and Build all-solutions, all using the left CTK3
field, P7, 4L, hold and Jstris180. Build's target is the 24-hole complement, not
the final filled board. Each fixture gets one warm-up pair and three measured
pairs with alternating AB/BA order, without concurrent jobs. The known minimum
25 is a postcondition only and is never passed as a bound or hint.

Four clocks remain separate:

- Direct CLI process spawn to close.
- CLI process spawn to close through the service.
- Service acceptance to completed result, including serialization/projection.
- Loopback HTTP request to terminal response, including polling delay.

Container startup and capability probes happen outside all samples. These are
not pure solver CPU measurements. In particular the default 250 ms result
polling interval can affect HTTP wall time without slowing CLI computation.
External Oracle/Discord network and rendering time is outside this diagnostic.

Every sample must have exit code zero, no cancellation/truncation, matching
runtime identity, complete solution keys and exact result identity. Minimum
validates all 25 members of the first canonical set before Discord's one-candidate
display projection; it does not force enumeration of later tied sets. Raw
inputs/results, environment values, tokens and executable paths are not logged.

## Pipeline integration and remaining evidence

The non-publishing candidate CLI job builds one optimized Linux binary and
runs same-host parity with at most four available workers. It has no deployment
or credential authority. The production candidate image includes the same
diagnostic module and checks its import closure during image build.

After immutable image and zero-traffic candidate validation and recovery
artifact upload, the deployment workflow invokes an isolated, uniquely named
Cloud Run Job on that exact image with 8 vCPU, 16 GiB and eight workers. No
managed secret is attached to the diagnostic's local service. The wrapper
verifies the Job and execution identity/specification, collects only its exact
diagnostic report, rechecks zero traffic and removes only resources it created
and can still identify. Existing resources must never be adopted or deleted on
an ambiguous create failure.

The report includes individual samples, medians, service/direct process ratio
and wrapper overhead. Result mismatch or lifecycle failure stops promotion;
an observed runtime delta is reported numerically, not silently declared small
or converted into an invented performance threshold. Actual Cloud measurement
and release remain pending until those operations complete. Source/mock tests
alone cannot establish Cloud performance parity.

The real loopback mock audit also found double projection: the Runner narrowed
a minimum portfolio to the canonical candidate, then the Executor rejected that
normal form on its second validation. Strict idempotency, including the same
pattern in other typed products, needs regression coverage; accepting arbitrary
already-projected objects is not a valid fix.

## Evidence references

- [Source-bound diagnostic run](https://github.com/daejunnom/Clearra/actions/runs/34045835905)
- `apps/clearra-discord-bot/scripts/benchmark-cloud-cli-parity.mjs`
- `scripts/release/cloud/benchmark-cli-parity-v080.mjs`
- `docs/research/qnia-cpsat-minimum-cover-comparison-2026-09-06.md`
