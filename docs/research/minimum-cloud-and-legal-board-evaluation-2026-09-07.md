# Minimum / Cloud CLI / legal-board evaluation checkpoint

## Authority and stop boundary

The user requested these evaluations after the temporary CI completed, and asked
that newly launched CI/Cloud measurements not be polled in the same turn. Results
are to be reviewed on the next ordinary user turn, not a steering message. Public
deployment, recovery, Pages, production traffic, Discord registration, and v0.8.0
publication remain paused. Minimum runtime is still a P2 hotfix, not a new release
blocker. Correctness failures are not waived.

## Completed input evidence

[Candidate 34047830012](https://github.com/daejunnom/Clearra/actions/runs/34047830012)
at `6a076a1d478ff9f5e9acf5e80d5dbed5cbc1aba5` completed with Rust/WASM, UI and
minimum diagnostics passed; CLI parity failed before computation. Its real import
graph reached the CTK3 decoder through job telemetry -> slash command catalogue ->
input decoder. Field capacity constants now live in a dependency-free leaf and
are re-exported for existing input callers. A transitive graph regression and an
early real import in the Linux job prevent repetition. No parser limits changed.

Single A/B pairs from that same Linux runner (4 workers, Jstris180, immutable
matrix, freshly created proof and canonical selector for each arm):

| Experiment | Baseline proof / canonical / total ms | Candidate proof / canonical / total ms |
| --- | ---: | ---: |
| Residual warm seed | 10,097 / 13,136 / 23,234 | 8,512 / 12,034 / 20,546 |
| Cached pivot exhaustion | 9,866 / 13,178 / 23,044 | 9,478 / 12,397 / 21,875 |

Each proved minimum 25 and the identical first canonical member list, with original
coverage checked. Rounded stage times need not add to rounded total times. These
are about 11.6% and 5.1% one-pair observations, not repeated speedup guarantees,
GUI timings, or evidence for 3 seconds. The hotfix remains isolated from main and
its optional algorithms remain default-off pending wider evaluation.

The Jstris coverage SHA is
`63de33e1d86077c179a38f6311df893ba9abcc13c9b18fa433384f5961eeee91`:
246 candidate fields, 5,040 queues, 79 u64 words. It is not the older SRS+ matrix.
This diagnostic fixture pin is unrelated to the updateable HF dataset policy.

## Exact first canonical and lazy later ties

The completed Rust/App regression
`canonical_ready_reads_do_not_enumerate_hidden_ties_and_explicit_pages_preserve_all_ties`
uses the common canonical constructor. Reading the completed first page does not
advance a hidden enumerator; known count stays 1 and total count stays unknown.
Each explicit next request advances the continuation, preserving all eight ties in
the small exhaustive fixture. The first immutable response remains honest after
later enumeration completes. Exact minimum proof, first-canonical proof, member
paging, and later-tie enumeration are separate concepts.

CLI/GUI must not replace an unknown total by 1, equate a known first set with
complete tie enumeration, or prefetch every tie before displaying the first set.
Copy means all members of the currently selected set, not just its 100 rendered
members. Discord retains its existing smallest canonical candidate projection;
it must not receive a widened alternative list. The real Linux parity harness
validates all 25 raw first-set members before testing that Discord projection.
These regression passes do not prove that the older WASM still on 4194 contains
the latest source, nor do they constitute a new browser timing.

## CP-SAT ideas and Qnia comparison

Qnia main was rechecked and still resolves to
`03b637730c5b541f4f2934be613498fbe65327fd`. Its OR-Tools 9.15 adapter proves exact
cardinality, but default Fast secondary optimizes a human-quality objective with
bounded refinement; it is not Clearra's exact first canonical objective. Some
architecture prose still describes HiGHS Auto; the actual adapter and the dedicated
OR-Tools integration document take precedence for the current backend.

Clearra's cached pivot exhaustion experiment applies an independently implemented
integer dual necessary condition: for uncovered weight N, maximum eligible row
load D, and k remaining choices, a selected pivot row r must satisfy
`load(r) >= N - (k-1)D`. If every legal covering row for one required pivot violates
that strict bound, k is impossible. Equality is retained; invalid shape, stale
coverage or overflow skips the optional prune. The cached proposal is recertified
against current eligible rows. Tests include restored siblings, exact tiny covers,
all canonical ties, cancellation, and unchanged root certificate exports.
This borrows the dual/reduced-cost pruning idea, not Google's CP-SAT implementation
or its complete SAT/LP/cut/learning machinery. Warm-start changes proposal search,
not the proof authority. Neither technique imports an answer or assumes 25.

`scripts/tools/benchmark-qnia-cpsat.mjs` now compares Qnia's public CP-SAT API on the
same Clearra Jstris matrix and normalized candidate order. It verifies raw matrix,
candidate and queue hashes, transposes coverage, records preparation and kernel
times, and performs three fresh solver calls with upstream defaults. Every
OPTIMAL/objective/bound result is checked against the original, unreduced matrix.
It reports cardinality proof time only and explicitly sets first-canonical proof
to false. Do not compare that time to Clearra proof-plus-canonical as though they
were the same output contract. The upstream source/WASM is separately checked out
in temporary CI storage, never linked or copied into the product. No product
dependency or license list changes are made for this reference-only diagnostic.

## Cloud execution equivalence

`Warm Cloud CLI Diagnostic` is a new manual-only workflow on an exact current-main
SHA, using the existing protected Cloud environment. It builds one source-bound
immutable image and creates one fresh, UID-checked Cloud Run Job: 8 vCPU, 16 GiB,
one task, no retries, no injected managed secrets. It never changes a service or
traffic and never stages Oracle. It cleans up only the Job and execution it owns;
uncertain ownership is reported instead of adopting/deleting another resource.

Inside that image, direct CLI and the actual Job Service/Runner/Executor use the
same executable hash, argv, source identity and 8-worker policy. One warm pair is
excluded and three measured pairs alternate execution order. Report four separate
times: direct CLI process, service CLI process, service job, and loopback HTTP wall
time. Cloud startup/capability probes are outside these measurements. CLI process
time still includes spawn/serialization, so this is not a pure solver timer.

The independent mode records `measurement_binding=isolated-image` and
`production_service_verified=false`. It can assess the Cloud environment and
Discord compute wrapper, but cannot prove that an already deployed service has
correct CPU throttling, concurrency or network latency. The existing zero-traffic
candidate mode retains all pre/post service readback checks for that later gate.
Node mocks or a GitHub-hosted Linux comparison are not live Cloud evidence.

Newly dispatched diagnostics are not polled during this turn. No measured Cloud
equivalence or new Qnia A/B result is claimed before the next requested review.

## wirelyre legal-boards: applicability, not a product import

The source is `wirelyre/tetra-tools/legal-boards`, not a standalone legal-board
repository. Reference revision: `2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c`.
The repository declares GPL-3.0-or-later. No source, generated list, Rust crate,
serialization or binary data is copied, translated or vendored into Clearra.
Qnia's licensing/provenance statement does not license wirelyre's original code
or establish permission for Clearra to redistribute its derived assets.

The useful abstract idea is a finite graph of partial fields that can reach a
complete field: generate transitions, retain states backward-reachable from the
goal, and use an index as a negative feasibility filter. It ignores concrete queue
choice and cannot itself enumerate every replay or solve minimum set cover.

| Clearra application | Required soundness boundary | Current decision |
| --- | --- | --- |
| PC Geometry/BuildUp negative filter | Index must be complete for a superset of the exact rule, height, representation, initial-field and line-clear semantics | Evaluate a separately generated Clearra index; no pruning activation yet |
| Arbitrary user-supplied garbage field | Empty-origin forward reachability is not guaranteed; a missing state can still be solvable from the supplied field | Never reject solely by absence from wirelyre's empty-origin index |
| SRS-X/SRS+ and changing kick tables | Union of older Jstris/TETR.IO movement is not automatically a superset of current Clearra rules | Require explicit rule-bound generation/proof, otherwise bypass |
| Setup and target-only Build geometry | Target-relative geometry, line-removal histories and placement identity differ from an unlabelled board | Reuse a scoped feasibility adapter only, not solution authority |
| Minimum proof / first canonical selection | These operate on already verified coverage rows, not unvisited boards | No direct acceleration; focus on the set-cover algorithm |

An independently authored future index should expose only `impossible` or
`unknown`, with explicit domain/version binding. Incomplete/unavailable data,
unproven rule inclusion, memory exhaustion, cache miss outside a complete domain,
or ambiguity must return unknown and use the normal ILC/verifier path. Positive
membership is not a solution, queue-coverage, or reachability proof. Test all
solutions/canonical IDs with index on/off on line-clear, arbitrary initial field,
mirrors, repeated pieces and each kick profile before activation. Existing
component-area necessary checks should be reused instead of duplicated.

For this benchmark the source stage is tens of milliseconds, while minimum proof
and canonical selection take seconds. Even eliminating that source stage cannot
explain the minimum bottleneck. Therefore this idea is not substituted for the
requested CP-SAT/minimum work or presented as a 3-second fix.

## Primary references

- [Qnia OR-Tools integration and limits](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/ORTOOLS_INTEGRATION_AND_LICENSE.md)
- [Qnia CP-SAT public adapter](https://github.com/Qnia28/sfinder_wasm/blob/03b637730c5b541f4f2934be613498fbe65327fd/src/ortools-min-cover.mjs)
- [Google CP-SAT architecture](https://github.com/google/or-tools/blob/stable/ortools/sat/README.md)
- [wirelyre license](https://github.com/wirelyre/tetra-tools/blob/2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c/LICENSE)
- [wirelyre graph reference, not imported](https://github.com/wirelyre/tetra-tools/blob/2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c/legal-boards/src/boardgraph.rs)
