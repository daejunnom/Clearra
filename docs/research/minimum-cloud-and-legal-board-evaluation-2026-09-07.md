# Minimum / Cloud CLI / legal-board evaluation checkpoint

## Authority and stop boundary

The latest request permits an additional Pages deployment only after all relevant
P0/P1 issues are cleared, without bypassing canonical acceptance or rollback
capture. New diagnostic CI is submitted without waiting for completion; review
its results on the next ordinary user turn. Minimum runtime remains a P2 hotfix.
External source comparisons are now strictly local-only: no upstream checkout,
comparison test, solver asset or raw benchmark artifact is included in CI/deployment.
The legal-board experiment uses a separate local branch and is not a product import.

The latest steering explicitly approves adding only the diagnostic workflow to
the existing GCP provider allow-list. Newly submitted CI/Cloud results are still
left for the next ordinary user input, not inspected during steering. Experiment
servers are fixed to 4195; existing listeners remain a separate manual cleanup.

## Latest completed CI and actual GUI evaluation

### September 7 follow-up: deploy preparation and live Cloud recovery

[Candidate 34057674746](https://github.com/daejunnom/Clearra/actions/runs/34057674746)
at `09572e6a2eec6150c2cf11cbcd1438e16176022b` passed all six focused leaves,
including the previously failing real native CLI/Discord comparison. The unlimited
Build admission correction below is therefore confirmed by a trusted executable.
No repeat investigation of that resolved early unsupported failure is required.

[Canonical 34057759636](https://github.com/daejunnom/Clearra/actions/runs/34057759636)
at `40315a3bef7a02e0d20b579b731f1837a11aa330` failed before product acceptance:
two release evidence test fixtures omitted the now-required WASM receipt digest,
causing nine assertions to fail. The fixtures now include the digest; validators
are not weakened. All 459 independent Node release regressions passed locally.
Those two test files are also selected before native compilation in candidate CLI
feedback, so this inexpensive fixture regression is caught earlier.

[Capture 34057705261](https://github.com/daejunnom/Clearra/actions/runs/34057705261)
was submitted prematurely and correctly refused missing canonical authority.
The required order is **successful exact-new-main canonical acceptance, then
capture of the active 9177273 site under that main, then Pages publication**.
Capture must not race acceptance. No new Pages publication is claimed by these
failed preparation runs, and no rollback/canonical guard is relaxed.

[Cloud 34057649033](https://github.com/daejunnom/Clearra/actions/runs/34057649033)
passed identity exchange and the immutable image build, then failed before the
Job API: gcloud ArgList rejects the repeated standalone `8` in `--cpus,8,--workers,8`.
Joined `--cpus=8,--workers=8` arguments fix the parser, without IAM or quota changes.
Independent diagnostics now offer closed 4/8-worker profiles (default 4); both
routes bind the same CPU/worker resources. The production-candidate mode keeps its
8-worker configuration check and cannot claim production parity using four.

A locally initiated recovery reused that exact 40315a3 immutable image, rather
than building it again: image digest
`abf3feddd846f26d423a9ecdfa7d0eaaebf4849a8aa78d0f4d61ea83cb6b209f`,
4 vCPU / 8 GiB / four workers. Execution
`clearra-parity-40315a3-34057649033-8q9b8` completed successfully; its owned parent
Job was deleted after UID/count/parent checks. Structured evidence was recovered
after discovering that `gcloud run jobs logs read --format=json` prints blank
human-readable payload lines instead of JSON envelopes. The wrapper now uses
bounded `gcloud logging read` with one closed project/region/Job selector; exact
execution/schema validation remains mandatory. All 32 Cloud lifecycle/command
tests passed after this correction. No raw logs or credentials are archived.

Live Cloud medians, one warm pair excluded and three measured pairs, startup and
capability discovery excluded (process timing still includes spawn/serialization):

| Fixture | Direct CLI ms | Discord service CLI ms | Service job ms | Loopback HTTP ms |
| --- | ---: | ---: | ---: | ---: |
| PC all, 246 fields / 5,040 queues | 34.649 | 34.357 | 35 | 41.357 |
| First canonical minimum, 25 members | 96,162.461 | 96,252.389 | 96,253 | 96,256.212 |
| Build all, 246 fields / 5,040 queues | 134.126 | 140.227 | 141 | 145.333 |

Result identities and all first-set members matched. Process deltas are -0.292ms,
+89.928ms (+0.094%) and +6.102ms (+4.55%); the actual service scheduling wrapper
adds below 1ms at the median for each fixture. This bounded same-image sample
supports small route overhead under the user's relaxed worker-count criterion.
It is not a production service/traffic configuration audit, a formal statistical
equivalence proof, or a successful rerun of the failed GitHub workflow. Absolute
Cloud minimum time remains a P2 algorithm/input-policy issue. Original failed
lifecycle evidence is preserved; the separate local recovery receipt is
`_local/reports/cloud-log-recovery-40315a3.json` and does not relabel that failure.

Latest same-binary ABBA medians from candidate 34057674746:

| Experiment | Workers | Baseline proof / canonical / total ms | Candidate proof / canonical / total ms |
| --- | ---: | ---: | ---: |
| Residual warm seed | 4 | 10,695 / 14,103 / 24,798 | 9,129 / 12,903 / 22,033 |
| Cached pivot exhaustion | 4 | 10,606 / 14,142 / 24,749 | 10,166 / 13,285 / 23,452 |
| Residual warm seed | 2 | 10,962 / 13,351 / 24,314 | 8,922 / 11,594 / 20,517 |
| Combined | 4 | 10,598 / 14,118 / 24,717 | 9,148 / 12,864 / 22,013 |

Every arm proved K=25 and the identical first canonical set. Repeated warm-start
benefit is approximately 11.2% at four workers and 15.6% at two in this run, not a
general speed guarantee. Warm seed is now promoted to the shared product default
for CLI/GUI/Discord, subject to the next exact-source canonical run. Only the dual
proposal is reused; all eligible capacities and checked-u128 prune certificates
are recomputed for the current residual problem. Root exports remain unchanged;
bad seed state falls back to uniform initialization without rejecting a solution.
Diagnostic builds retain explicit on/off A/B. Cached pivot exhaustion remains
isolated/default-off: the combined measurements do not show additive benefit.
First canonical ordering and explicit lazy subsequent ties are not weakened.

The new local-only Qnia comparison uses unchanged upstream 03b6377, the same
Jstris matrix, fresh workers, and two samples per backend/input. Module load is
excluded. HiGHS API time includes LP construction; CP-SAT API time includes model
encoding, while its reported solver wall time excludes that work. HiGHS uses its
WASM default and CP-SAT two workers, so these are not equal-CPU speed ratios.

| Input / exact K | HiGHS API ms | HiGHS rounded-cuts API ms | CP-SAT API ms | CP-SAT solver ms |
| --- | --- | --- | --- | --- |
| Full left P7 / 25 | 16,359 / 16,376 | 14,916 / 15,011 | 4,374 / 4,351 | 4,233 / 4,212 |
| First I / 11 | 600 / 593 | 460 / 455 | 284 / 306 | 157 / 176 |

Continuous root LP bounds were 20.932385 for full P7 and 9.5 for first I, weaker
than the integer optima; a fast LP bound alone does not prove K. CP-SAT's full-P7
pure calculation now reproduces roughly 4.2s and confirms that Clearra's primary
proof, not only canonical refinement, needs further work. Rounded cuts helped
this fixture, but are not imported or treated as an exact integer certificate.
HiGHS reports Optimal with zero relative MIP gap; its floating objective is checked
against the validated integer cardinality with 1e-8 tolerance, not silently
rounded into a Clearra proof. All chosen rows cover the original matrix.
Initial local harness failures (missing Node JSPI flag and an over-strict raw
floating equality check) were corrected and only failed samples were retried;
the report does not claim uninterrupted ABBA. No expected answer was a solver hint.
No Qnia/HiGHS/OR-Tools code or asset enters tracked product/CI/deployment source.
Report: `_local/reports/qnia-highs-cpsat-20260907-complete.json`.

### Earlier completed evidence (retained, not a new verification)

[Candidate 34055343299](https://github.com/daejunnom/Clearra/actions/runs/34055343299)
at `4f6715b0fe5523ea2518e7b2fcde62e4a4876772` completed: source, native Rust,
WASM, UI compilation and minimum diagnostics passed; CLI parity failed on the
first direct Build-all warm-up (4.743ms, exit 3 / unsupported). It did not hit a
deadline, output limit, spawn error or process signal. The new bounded diagnostic
artifact preserved both the failing route and the two already completed fixtures;
its archive SHA-256 was verified before inspection. Raw child text, arguments and
environment remain excluded. The CLI error-code parser now also recognizes the
actual `error E_CODE message` prefix, while recording only the allow-listed code.

The native-only durable Build coordinator treated default `max_candidates=0`
(unlimited) as unavailable task authority; its system admission probe also rejected
zero. Browser WASM does not compile this native route. The correction preserves
zero as unlimited, grows receipt storage with issued tasks using fallible amortized
allocation, retains explicit nonzero bounds and ordinal-overflow checks, and avoids
inventing a cap or silently reducing workers. Existing native durable tests now
use the real unlimited default instead of masking it with 1,024 candidates; those
tests and system-provider tests are explicitly selected in the next candidate CI.
This source diagnosis fits the captured early unsupported exit; confirmation of
the corrected executable still requires that new trusted native run.

Completed same-host measured-pair medians (one warm pair excluded, three pairs):

| Fixture | Direct CLI ms | Service CLI ms | Service job ms | Loopback HTTP ms |
| --- | ---: | ---: | ---: | ---: |
| PC all, 246 fields / 5,040 queues | 35.309 | 35.631 | 36 | 41.609 |
| PC first canonical minimum, all 25 members | 46,289.657 | 46,289.661 | 46,290 | 46,293.085 |

Both completed fixtures matched raw output identity. This is GitHub Linux
same-host evidence, not Cloud Run equivalence. The product CLI minimum timing is
also not the focused proof/canonical diagnostic timing below; that additional
gap remains a P2 input/execution-policy comparison axis, not a solved performance
claim. Failed Build prevents claiming overall native parity.

In the previous run `candidate-rust-wasm` took 44m54s: 3m22s setup, 24m16s native
regressions and 16m32s independent WASM compilation. Native and WASM are now
sibling jobs after source binding. Every native selection remains; the native
leaf no longer installs npm dependencies or wasm-bindgen. The full gate still
runs once when explicitly selected, and skips all focused leaves. The latest
independent WASM job produced its verified artifact in approximately 19m45s
(15m13s Rust compilation), without waiting for native regression completion.
This measures earlier WASM feedback, not a hardware-controlled compiler speedup.

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
Finished workers read 0/11. These bounded browser checks do not constitute
exhaustive P0/P1 coverage, but the now-classified native CLI failure is not a
Pages blocker. No outstanding observed Pages P0/P1 is established by that failure;
minimum runtime remains the explicitly accepted P2 hotfix. Publication still needs
one exact-main canonical acceptance and its accepted Pages artifact plus a durable
active-site rollback capture. An unqualified candidate artifact must not be
restamped as accepted. Start that qualification/capture path independently of
Cloud diagnostics; do not publish an unfinished build or claim a deployment from
successful diagnostic compilation alone. The 3-second minimum target is not met.
Port 4196 was stopped after this completed audit, before the new 4195-only policy.
Do not recreate 4196 or silently replace the user's 4194 WASM/session.

Fresh public readback on September 7 identifies the active Pages source as
`91772735c3f7ec7d89ecd3e82aa4af4014995bf6`, version 0.8.0, accepted run
`33582717675` attempt 1. GitHub deployment `6214318848` is successful and points
to Pages run `33583845820`; the public WASM manifest agrees with the source.
Capture this active canonical site using ordinary `capture`, not the historical
v0.7.4 bootstrap path. The rollback bracket must belong to the newly committed
main SHA. This readback is not a new deployment.

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
| Residual warm seed | 4 | 10,676 / 14,093 / 24,770 | 9,152 / 12,908 / 22,061 |
| Cached pivot exhaustion | 4 | 10,622 / 14,084 / 24,706 | 10,166 / 13,297 / 23,463 |
| Residual warm seed | 2 | 11,050 / 13,430 / 24,480 | 8,955 / 11,631 / 20,587 |
| Combined ideas | 4 | 10,628 / 14,091 / 24,719 | 9,146 / 12,881 / 22,028 |

At that checkpoint both techniques were diagnostic/default-off (warm seed is
promoted in the follow-up above). The combined arm is approximately
10.9% faster in this bounded run and essentially matches warm seed alone within
the observed variation. This does not establish an additive gain, a general speed
ratio or the 3-second target. Exact K and first-canonical identity remain unchanged.

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

### Pure cardinality timing and input sensitivity (latest steering)

The public [Qnia Minimals GUI](https://qniapc.vercel.app/sfinder/minimals) was
actually exercised with `v115@9gD8FeD8FeD8FeD8PeAgH`, `*!`, 4L, hold enabled,
Primary Auto and Quality Fast. Two completed runs displayed 4,265ms and 4,185ms,
5,040 queues, 25 minima, `ortools` and `fast-2x2`. This reproduces the user's
3–5 second range.
That is the site's reported feature timer, not an independently observed native
solver-only timer, and the deployed site's exact artifact revision is not bound
to the local reference. Do not replace this GUI observation by a slower Node
measurement or assert that loading/canonical differences fully explain the gap.

The new ignored `_local/research/qnia-pure-proof.mjs` separately reads the public
`CpSolver.wallTime` (native response `wall_time`). It excludes module/WASM load,
worker creation, model encoding and cleanup, but includes native presolve/search.
It uses unchanged upstream 03b6377 and its two-worker `max_lp` defaults, validates
OPTIMAL/objective/bound and checks every selected row against the original matrix.
Each sample uses a fresh worker. Known 25 is a postcondition, never a solver hint.
Report: `_local/reports/qnia-pure-proof-20260907.json` (local-only, not a CI asset).

| Input (all Jstris 180) | Original rows / queues | Kernel constraints / candidates / entries | Exact K | Native solver milliseconds, three samples |
| --- | ---: | ---: | ---: | --- |
| Left 16-cell field, full P7 | 246 / 5,040 | 1,389 / 158 / 15,128 | 25 | 6,075 / 8,017 / 9,853 |
| Right mirror, full P7 | 246 / 5,040 | 1,385 / 158 / 15,104 | 25 | 9,399 / 8,114 / 4,916 |
| Left, first I + remaining six permutations | 246 / 720 | 356 / 89 / 3,294 | 11 | 229 / 218 / 243 |
| Left, first S + remaining six permutations | 246 / 720 | 241 / 80 / 2,108 | 10 | 176 / 171 / 238 |

The full left input is the exact downloaded Clearra Jstris matrix. The mirror is
independently enumerated using Qnia's public feature API; the two prefixes are
explicit restrictions of the original queue universe, not row-order variants
mislabelled as different physical fields. No cross-rule coverage parity is
claimed from the mirror alone. The two P7 models already differ in constraint
count, so physical mirroring must not be assumed to preserve the Jstris problem.

Input size alone does not predict exact set-cover cost: the same 246 raw rows
produce very different kernels and proof branches (full left 2,571–3,072;
first-I 351; first-S 186–235). Even equal branch counts had different elapsed
times across repeats, so Node/browser runtime, scheduling and machine load remain
uncontrolled factors, not an established source-level cause. These observations
do not constitute matched-browser A/B or a general speed ratio. In particular,
Clearra's 9–11s native proof stage is itself a target, separate from its additional
12–14s first-canonical stage; canonical semantics cannot explain the proof gap.

Keep input sensitivity as an explicit hotfix acceptance axis: physical field
shape/mirror, queue restriction, solution density, kernel size/forced rows,
workers (2/4/all), proof branches, native proof, first-canonical and lazy-next
time must be reported separately. Use repeated paired comparisons on identical
raw matrices and hardware; do not pool easy prefixes with the hard P7 fixture.
Only exact-cardinality-safe dominance belongs in the proof kernel. Original IDs
and equal/dominated alternatives remain available for canonical and lazy ties.

## Cloud execution equivalence

`Warm Cloud CLI Diagnostic` is a new manual-only workflow on an exact current-main
SHA, using the existing protected Cloud environment. It builds one source-bound
immutable image and creates one fresh, UID-checked Cloud Run Job: default 4 vCPU,
8 GiB, or explicit 8 vCPU / 16 GiB,
one task, no retries, no injected managed secrets. It never changes a service or
traffic and never stages Oracle. It cleans up only the Job and execution it owns;
uncertain ownership is reported instead of adopting/deleting another resource.

Inside that image, direct CLI and the actual Job Service/Runner/Executor use the
same executable hash, argv, source identity and selected 4/8-worker policy. One warm pair is
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
failed at OIDC exchange before image build or Job creation. Its provider allowed
only `discord-deploy.yml@refs/heads/main`. After explicit user approval, the live
`clearra-main` provider condition was updated once to additionally allow exactly
`cloud-cli-diagnostic.yml@refs/heads/main`. The initial asynchronous readback had
not converged; a subsequent read confirmed the exact approved condition, ACTIVE
state, unchanged issuer and attribute mapping. Repository ID 1309293231, owner ID
271715321, repository name and main ref remain pinned. No IAM roles, principal
subjects, secret access, production service or traffic were changed.

Source audit found a second pre-compute failure: the old workflow tried to build
using the protected deployer, which has no Cloud Build submit authority. The
workflow now separates a branch-main builder job from the environment-protected
compute job. It reuses the existing builder and deployer accounts without extra
roles, the approved regional source bucket and build execution identity, builds
one image and passes its verified immutable digest into the dependent compute
job. Exact source binding still occurs before Cloud access. These source/mock
checks are not a successful live Cloud measurement; read the newly submitted
Cloud run only after the next ordinary user input.

Live role readback also confirmed `run.jobs.delete` exists but
`run.executions.delete` does not. The diagnostic cleanup formerly requested the
latter and would fail after a successful run. It now validates the exclusive
parent UID, execution count/UID and parent pointer, then deletes the owned Job
using existing authority. Google documents that Job deletion terminates its
running executions. The receipt honestly records `owned-parent-deleted`, not a
separately observed execution deletion. Identity drift still blocks deletion;
cleanup failure still fails the whole diagnostic. No new Cloud timing is claimed.

## Finite experiment server lifecycle

`scripts/tools/run-gui-experiment.mjs` is the only new one-off GUI launcher.
It binds 127.0.0.1:4195 with strict port handling and refuses an existing listener
without adopting/stopping it. Its hidden, IPC-connected child uses local-audit
mode (no HMR), a default 30-minute lease (explicit 1–120 minutes), and no restart.
Parent exit/disconnect, explicit stop or lease expiration closes only its owned
server; an independent child lease prevents parent failure leaving it permanent.
No HTTP-idle timer is used because browser-local WASM can run without requests.
This is not a product search cap, and it does not modify the 4194/8790 watchdog.
Eight lifecycle tests cover occupied ports, strict binding/races, parent exit,
lease/force cleanup, hidden spawning and failure reporting. A live refusal check
preserved existing 4195 PID 25676 and 4194 PID 5952; no replacement was started.

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
- [HiGHS primary repository](https://github.com/ERGO-Code/HiGHS)
- [Google Cloud structured log read](https://docs.cloud.google.com/sdk/gcloud/reference/logging/read)
- [wirelyre license](https://github.com/wirelyre/tetra-tools/blob/2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c/LICENSE)
- [wirelyre graph reference, not imported](https://github.com/wirelyre/tetra-tools/blob/2342953cb424cfd5ca94fa8eefdbe5434bd5ff1c/legal-boards/src/boardgraph.rs)
- [Google Cloud Job deletion and execution termination](https://cloud.google.com/run/docs/managing/jobs#deleting)
