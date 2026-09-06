# Minimum / Cloud CLI / legal-board evaluation checkpoint

## Authority and stop boundary

The latest request permits an additional Pages deployment only after all relevant
P0/P1 issues are cleared, without bypassing canonical acceptance or rollback
capture. New diagnostic CI is submitted without waiting for completion; review
its results on the next ordinary user turn. Minimum runtime remains a P2 hotfix.
External source comparisons are now strictly local-only: no upstream checkout,
comparison test, solver asset or benchmark result is included in CI/deployment.
The legal-board experiment uses a separate local branch and is not a product import.

## Latest completed CI and actual GUI evaluation

[Candidate 34051492524](https://github.com/daejunnom/Clearra/actions/runs/34051492524)
at `2d43a7cce2557ec376d0759746e3c3edb4de8a18` finished with Rust/WASM, UI and
minimum diagnostics passed; CLI parity failed during computation, not import.
The former diagnostic emitted only `cli_not_successful` and discarded prior
fixtures and the failing route. That result cannot establish Cloud or CLI parity.
The harness now writes a failed, non-authoritative JSON artifact with bounded,
allow-listed fixture/route/exit/resource metadata and completed fixture evidence,
and validates an arm before spending time running the next arm. Raw child text,
arguments and environment are never copied. Successful compilation can preserve
the explicitly unqualified CLI artifact even if a later diagnostic fails.

`candidate-rust-wasm` took 44m54s: 3m22s toolchain setup, 24m16s native
regressions and 16m32s independent WASM compilation. Native and WASM are now
sibling jobs after source binding. Every native selection remains; the native
leaf no longer installs npm dependencies or wasm-bindgen. The full gate still
runs once when explicitly selected, and skips all focused leaves. This removes
the serial dependency, not a claim of already measured new-run speedup.

Downloaded artifact archive SHA-256 values were checked, then the five WASM files
were verified against the exact 2d43a7c source worktree and original runtime IDs:
WASM `d7d5a71a45c7f34aab313898fb32f8f228849afc40367a5277ce9fbcbf1f39fa`.
Browser-control checks ran on isolated port 4196 (4194 unchanged), Jstris 180,
left 16-cell field / P7 / 11 compute workers:

| Actual GUI request | Result | Displayed elapsed |
| --- | --- | ---: |
| PC all solutions | 246 fields, 100% coverage, 100 initially rendered | 0.7s |
| Minimum solutions | exact first 25-member set; explicit next opens set 2 | 27.9s |
| Pattern complete replay | 246 representatives, 3,993,088 paths; loaded replay image, 10 frames at 500ms | 9.6s |
| Build all solutions, target complement | 246 fields, 5,040/5,040 patterns | 0.7s cold / 0.4s warm |

The warm Build run also passed with Korean UI and updated Geometry nodes.
Finished workers read 0/11. These bounded browser checks do not clear the unseen
native CLI failure or constitute exhaustive P0/P1 coverage. No new canonical
acceptance or current rollback capture exists for this source, so Pages is not
published from this unqualified artifact. The 3-second minimum target is not met.

The new local-only CP-SAT adapter run (Node 24.16.0, upstream 03b6377 unchanged)
on the same downloaded 2d43a7c Jstris matrix took 5.290, 5.539 and 7.174 seconds
for exact cardinality, including solver-worker load and cleanup. The adapter's
`source_commit=e352492...` identifies the local invoking checkout, not a rebuild
of that downloaded matrix. Matrix provenance is 2d43a7c / the hash below.
All three proved K=25 and covered the original matrix, but selected-key hashes
differed; none claimed Clearra's exact first canonical ordering.

Latest native same-binary ABBA medians (two samples per arm; exact K=25 and
identical first canonical members in every arm):

| Experiment | Workers | Baseline proof / canonical / total ms | Candidate proof / canonical / total ms |
| --- | ---: | ---: | ---: |
| Residual warm seed | 4 | 10,768 / 14,305 / 25,075 | 9,221 / 13,044 / 22,267 |
| Cached pivot exhaustion | 4 | 10,747 / 14,330 / 25,077 | 10,259 / 13,487 / 23,747 |
| Residual warm seed | 2 | 11,134 / 13,463 / 24,598 | 9,057 / 11,626 / 20,684 |

Both techniques stay diagnostic/default-off. The next isolated candidate adds a
combined ABBA arm to test interaction, without changing the production default.

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

`_local/research/benchmark-qnia-cpsat.mjs` compares Qnia's public CP-SAT API on the
same Clearra Jstris matrix and normalized candidate order. It verifies raw matrix,
candidate and queue hashes, transposes coverage, records preparation and kernel
times, and performs three fresh solver calls with upstream defaults. Every
OPTIMAL/objective/bound result is checked against the original, unreduced matrix.
It reports cardinality proof time only and explicitly sets first-canonical proof
to false. Do not compare that time to Clearra proof-plus-canonical as though they
were the same output contract. The adapter and its tests have been moved out of
tracked release source; execution rejects CI environments. Candidate CI no longer
checks out Qnia or invokes/uploads external comparisons. Existing historical CI
results below are evidence from before this policy change, not a continuing path.
No product dependency or license list changes are made for this local diagnostic.

Source comparison explains the timing gap in two separate ways. Qnia's exact
cardinality kernel removes redundant constraints and dominated candidates; the
current Jstris matrix becomes 1,389 constraints / 158 candidates / 15,128 entries.
It solves Boolean coverage constraints and minimizes their sum with CP-SAT's
`max_lp` profile. Clearra instead uses a portable exact cover branch search with
integer-certified dual bounds, memoization and resumable partition receipts.
After proving K, Clearra additionally uses exact AtMost self-reduction on original
canonical candidate IDs; eliminated/dominated alternatives must remain available
for canonical selection and lazy ties. Each negative prefix can require another
exact proof. Copying Qnia's primary reduction into that second stage would lose
valid earlier IDs/ties. Qnia's default Fast secondary is a different human-quality
objective, not this complete lexicographic proof. The historical 3.662-5.543s
full-feature local measurements therefore do not measure the same contract.

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

[Cloud diagnostic 34051405943](https://github.com/daejunnom/Clearra/actions/runs/34051405943)
failed at OIDC exchange before image build or Job creation. Live provider readback
confirmed its condition permits only `discord-deploy.yml@refs/heads/main`, while
the diagnostic uses `cloud-cli-diagnostic.yml`. A narrowly scoped allow-list
addition preserving immutable repository/owner IDs and main was requested from
the user; no IAM changes or production service updates were made automatically.

Live role readback also confirmed `run.jobs.delete` exists but
`run.executions.delete` does not. The diagnostic cleanup formerly requested the
latter and would fail after a successful run. It now validates the exclusive
parent UID, execution count/UID and parent pointer, then deletes the owned Job
using existing authority. Google documents that Job deletion terminates its
running executions. The receipt honestly records `owned-parent-deleted`, not a
separately observed execution deletion. Identity drift still blocks deletion;
cleanup failure still fails the whole diagnostic. No new Cloud timing is claimed.

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

The local `codex/legal-board-kick-index-20260907` experiment implements only the
independent graph/index boundary under `_local/legal-board/`, not a replacement
movement engine. Its generation key binds ordered kicks (including implicit
origin/O policy), engine, spawn/lock/clear rules, dimensions and initial domain.
Tests cover a kick-dependent transition change requiring regeneration, real
SRS-X fixture key invalidation, unclosed domains, incomplete generation, memory
budgets, cancellation and arbitrary garbage. Reverse reachability agreed with
independent forward reachability for all 512 directed graphs on three states
(1,536 state queries). Five tests passed in Node. This is boundary/prototype
evidence, not generation or proof of a complete 4L Tetris index for every kick
profile. Full generation remains deferred until its cost/domain is established.

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
- [Google Cloud Job deletion and execution termination](https://cloud.google.com/run/docs/managing/jobs#deleting)
