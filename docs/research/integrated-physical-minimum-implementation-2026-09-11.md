# BuildUp / Inverse Lock-Clear / APDP integrated minimum search

Status: revised goal complete, including multithreading A/B and verified
restoration of the fastest retained GUI result. The original S0-S5 plan remains recorded below with
its actual unfinished work. This local record grants no release or deployment
authority.

Scope revision from the user's latest steering: finish the current extended
parent application and its A/B, then perform only multithreading improvements
and close this goal. Remaining open-catalog, implicit-controller and mode
expansions below are historical pending work outside the revised goal. They
must not be marked implemented as a consequence of this narrower completion.
The final steering further requests restoration of the fastest verified retained
result after the current comparison. Selection therefore examines the entire
retained GUI record, while performance adoption still requires reproducible
comparisons rather than a single minimum observation.

## Boundaries

- CPU only: native C and Rust WASM compact/extended. General PB learning, MaxSAT,
  BDD bundles, and GPU kernels/solvers are excluded.
- PC and mask-based Build may synthesize diagrams. Colored-target and supplied
  Build remain inside their existing identity allow-list. Spin-structure receives
  regression coverage for shared code, not a new physical-search interpretation.
- A slot denotes one common logical diagram. Queues may use different legal
  placement orders, hold histories, clear schedules, and temporal realizations.
- Preserve Same-Tile proof premises, every viable temporal parent, original
  normalized-key ordering, dense IDs, all equal/dominated alternatives, and the
  distinction between cardinality proof and first canonical selection.
- Preserve per-pattern score optima, PC score's entire required universe, score
  profile accuracy, and VisibleSeven's global observation-policy finalizer.
- Unknown, cancellation, memory limits, ungenerated parents and unfinished
  families never establish infeasibility.
- New implicit evidence has its own typed contract. Legacy complete-source
  validators are not weakened. An identity-only complete source dictionary is
  initially retained for old dense IDs; full coverage is eligible for laziness.

## Work and evidence ledger

| Stage | Required work | Status / evidence |
| --- | --- | --- |
| S0 | Freeze source, compare C/Rust temporal-parent rules, fresh private GUI baseline | Source frozen: 904 CPU files at HEAD `81b04356682e9253bd594eefa567e06d868945f1`; baseline manifest below. Audit and measurements in progress. |
| S1 | APDP incremental/duplicate work, inverse partial-row rejection, feasibility/cache ablations, component complement joins | C temporal-parent safety fix and Rust APDP/MITM arms implemented. C packing target, 6 component, 4 APDP and 3 inverse tests passed. Inverse early projection, feasibility Off/RelaxationOnly and optional APDP counters compile in the second WASM. No performance adoption yet. |
| S2 | Same-complete-diagram multi-queue queries on the unchanged complete candidate domain, scoped failure reuse | Experimental intersection foundation passed 3 tests, including all 512 small incidence matrices. Actual compact Oracle selected-queue provider, any-queue identity admission and complete admitted identity dictionary are implemented. Existing P7 singleton/full-row comparison passed across policy combinations; full PatternIds, immutable owners and error usage remain bound. |
| S3 | Open inverse families, shared physical transitions and DAG provenance, partial-order reuse, optional 2/3-piece completion graph | Candidate-scoped feasibility/DAG, projection and physical-transition reuse implemented and tested. Queue/supply failure state remains separate. Bounded compact root-branch streaming and a same-owner full physical/objective integration are implemented. Compact logical-only catalog collection and on-demand complete temporal-parent reconstruction are implemented and tested. Fresh compact P7 native (9) and GUI (21) comparisons are complete with no new default adopted. Extended packed-parent runtime, four existing mode parity fixtures and 48 extended timing samples are complete; no new default was adopted. Truly open logical catalogs, remaining mode integration and optional completion graph remain outside the revised goal. |
| S4 | Exact K-slot synthesis, counterexample refinement, implicit evidence and canonical/alternative provider | Native experimental controller and actual compact Oracle PC-minimum adapter implemented. Three controller tests (all 512 small matrices/all original-ID optima, limits/Unknown) and two actual-adapter tests passed. Browser scheduling and remaining mode adapters are pending. |
| S5 | Mode-wide differential verification, A/B combinations, portable evidence, select validated defaults | Compact P7 parent-policy and I2/F1 interaction comparison completed on the current host; all raw matrices and first canonical results match. Material baseline drift prevents attributing early/late timing changes to the candidate. Other-mode differential and portable finalist validation remain pending. |

## Measurement contract

Use the private benchmark GUI on 127.0.0.1:4195. Primary input: minimum / P7 /
`ctk3_w0kCQBjwwAMPPAD37g`. Existing Qnia 3-5 s is a prior measured reference;
do not remeasure Qnia or use old Clearra timings as current baseline.

Fresh baseline workers: 1, 2, 4, 8, 11, 12. Initial candidate comparisons: 4, 8,
11; finalists cover the entire worker axis. Default batch count 3, alternating
serial comparisons; expand ambiguous comparisons to 6 total. Do not benchmark
concurrently with builds or other solver runs. Record worker count separately
from partition count, artifact/source/request identity, cold/warm lifecycle,
host resources, peak memory, stage times and first canonical paint.

Compare old/off/new policies plus justified interactions: APDP x feasibility,
Same-Tile/APDP x MITM, shared DAG x caches, lazy families x counterexamples,
workers x partition width. OFF keeps complete physical validation.

Performance adoption requires correct output and at least 5% median first-paint
improvement without over 5% median regression in the adopted input class.
Unclear or unsuccessful candidates stay experimental. Correctness fixes are
evaluated separately and become part of the corrected comparison baseline.
The original matrix, known K, and historical canonical answers are post-run
verification references, never hidden inputs to the new engine.

## Verification

Extend existing fixtures rather than introducing a separate release gate:
mixed static/dynamic parents; Same-Tile counterexamples; O/T decomposition
deduplication; row-shift parity; queue/hold/reachability failures; converging
temporal parents; component coupling; original identity and score preservation;
hidden future-piece policy; cancellation/resource/stale evidence. Small exact
instances use exhaustive reference search and native/WASM differential checks.

Local artifacts live in the primary checkout under
`_local/research/integrated-physical-minimum-20260911/`.
`baseline-source.json` binds the before-edit CPU source snapshot; `bench/` owns
the new private server and fresh benchmark records. Raw records are append-only.
WSL compilation is authorized when Windows application control rejects a build.
No production deployment or CI monitoring is part of this experiment.

## Verification completed during implementation

- `clearra-coverage` joint-domain: 3 passed, 0 failed. The exhaustive fixture
  compares every three-row/three-pattern matrix and every required subset.
- `clearra-core-executor`, feature `minimum-physical-ab`: component tests
  6 passed; APDP tests 4 passed. The separate arm-pair filter reran 2 of those
  APDP tests; these are not two additional independent cases.
- C packing target: passed using the authoritative CMake source manifests,
  GCC 13.3, C11 and the existing test driver. CMake itself was unavailable, so
  this was a direct compile/link/run, not a `ctest` result. The exact source
  hashes, flags, target membership and result are in `c-packing-result.json`.
- All native Rust execution used the authorized WSL environment. Windows
  application-control policy was not changed.
- The first score regression invocation omitted the existing `parallel` feature
  and two tests stopped at `WorkerPoolUnavailable`. Repeating the same existing
  filter with `minimum-physical-ab,parallel` passed all three tests; no score
  assertion or production worker policy was weakened.
- The first current-source WASM attempt failed before compilation because
  WSL reconstructed an unencoded environment-check command incorrectly. The
  build tool now encodes that script and releases its staging lease even if
  provenance setup fails. The subsequent build completed; its WASM SHA-256 is
  `41478756f48dbd98249b4b627e476b430c65fcf89838aeca42d2bf605d9e7a4e`.
  This frozen first artifact contains the APDP/MITM arms, not the later inverse,
  feasibility, physical-provider or implicit-controller edits.
- One historical-artifact GUI preflight timed out at 180 seconds. It remains
  UNKNOWN; neither a baseline nor an infeasibility/performance-adoption result.
  The new private GUI preserves progress and lifecycle data on timeouts and
  verifies the policy initialization of every isolated WASM worker.

## First artifact observations (not adoption evidence)

Fresh B/A0/C0 workers 11 produced first canonical paint in 23.396, 23.085 and
23.359 seconds. Those three records precede OS sampling. Later axes use an
explicit 2-second sampler epoch, so final comparisons require matched pairs.
Workers 8 completed in 24.728, 25.303 and 24.849 seconds. Workers 4 completed
two samples (64.303 and 94.811 seconds) and reached the 180-second deadline in
the third. The deadline record is UNKNOWN, not a slow successful result.

The existing N32 combination at workers 11 completed in 44.249, 44.853 and
45.170 seconds. It changes the bounded canonical probe and idle-assistance
policy; selecting this arm does not itself increase the partition count.
Workers 12 require the existing explicit all-CPU-threads option and completed
in 50.758, 48.796 and 48.567 seconds. Worker count, partition count and total
initialized module instances are different measurements.

Workers 2 completed one sample in 40.719 seconds. The next had a 229.601-second
gap in both browser progress and OS sample delivery. Its deadline callback
arrived late; preserve it as UNKNOWN with a host-gap marker and do not attribute
the 291-second callback latency to solver computation. The cause is unconfirmed.
Raw records are append-only; additional samples use a named new timing epoch.

The identity-only preprocessing pass must still prove any-queue buildability
before the existing normalized-key ordering assigns dense IDs. PC minimum and
Build use their reachable coverage union; PC score retains its entire input
universe. A queue unsupported by the current portfolio can be excluded from
PC minimum only after complete failure across the full source. VisibleSeven
requires the existing global policy finalizer; independent queue witnesses
cannot substitute for that finalizer.

## Second artifact and activation evidence

The second private WASM build completed in WSL in 8m04s. Its SHA-256 is
`22d81ee5fd5d1cf109d9f89834747d97594c050ae7a7344e5a23721262e48384`,
with source SHA-256
`6bc864b07022d6c8451542c6f879139ca38e5d6785bac59e536e352ef7b3f5df`.
Private GUI v2 uses the same production result pager, limits progress snapshots
to stage changes and 100 ms cadence, and records every I/F/D policy ACK. This
instrumentation differs from v1, so v1 timings are not the v2 adoption baseline.

Sample 032 is a diagnostic run, excluded from timing-only D0 comparisons. P7
reported 3,436 minimum-domain evaluations and **zero advanced/APDP-eligible
evaluations**, with all APDP work counters zero. The advanced geometry guard
requires target depth at least seven and a non-authoritative resource path;
this P7 fixture places six pieces. The earlier v1 matched time differences
between A0/C0, A1/C1 and A2/C2 therefore do not prove APDP/MITM acceleration.
Retain those raw observations without adopting a policy on that basis.

The auxiliary native algorithm probe also compiled in WSL. Its frozen binary
SHA-256 is `91e0d8b54f359a7a1d75668edc853c06c0fb032d0d00056cea9284b418acb211`.
It compares legacy and lazy physical algorithms using one native binary and
includes source preparation. It is not a browser first-paint endpoint and is
not directly compared with the WASM or Qnia GUI time. Its offline scratch
workspace resolved cached dependency versions independently of the product
lockfile; native arms share those exact dependencies.

## Fresh v2 P7 comparison (workers 11)

Samples 032-062 all preserve K=25, the original-ID first canonical hash
`7314db27276521fe547b76236fd196726c7a40cbc4ef2af2935ea2363df04d8c`,
the lazy-alternative contract, and every module's artifact/policy ACK.
Sample 032 is diagnostic only; 033-062 are 30 timing observations. All 31
have full-periodic OS observations in one sampler epoch. The largest interval
including boundaries is 2,036 ms. Observed per-run minimum free RAM varied
from 4,078,202,880 to 7,327,076,352 bytes; physical CPU frequency/thermal state
was not measured. This is not a claim of identical resource conditions.

| Timing policy | Completed runs | Median first paint (s) | Fastest verified (s) | Range (s) |
| --- | ---: | ---: | ---: | --- |
| Corrected legacy I0/F0 | 9 | 21.150 | **15.185** | 15.185-31.893 |
| Inverse early I2/F0 | 3 | 21.054 | 20.097 | 20.097-21.152 |
| Optional feasibility off I0/F1 | 3 | 20.772 | 20.220 | 20.220-21.607 |
| Relaxation only I0/F2 | 3 | 21.161 | 20.666 | 20.666-21.357 |
| Inverse early + off I2/F1 | 6 | 16.618 | 15.424 | 15.424-20.701 |
| Inverse early + relaxation I2/F2 | 6 | 18.315 | 16.743 | 16.743-20.869 |

These are descriptive TF-screening results, not adoption claims. Both
combinations improve the pooled median, but the fastest verified result in
that screening epoch remains TF without the new physical policies.
Paired I2/F1 changes are -7.01%, -32.05%, +6.34%; paired I2/F2 changes are
-8.23%, -25.15%, +23.17%. The reversal against the last adjacent baseline and
large baseline drift prevent attributing the pooled gain confidently to a
policy. Keep both combinations experimental, and keep the user-requested
fastest verified baseline visible alongside medians. No default was switched.

Exact rows, hashes, pair membership, diagnostics and qualifications are in
`bench/physical-ab-comparison.json`; `compare-physical-ab.mjs` regenerates it
from independently verified raw records. Individual worker counts are not
claimed to emulate a different physical PC, and v1/v2 GUI timings are not pooled.

The first auxiliary native lazy run completed preparation in 92.77 ms, retaining
246 admitted original identities and all 5,040 full PatternIds. It then returned
UNKNOWN/WorkLimit after 103.26 ms total: the initial 100,000,000 conservative
controller work-unit cap was inadequate, rather than a 180-second performance
timeout. It reached 91 joint queries/eight counterexamples without claiming a
minimum. The next frozen harness retains the 180-second deadline and explicit
512 MiB controller bound, increases only the finite controller work cap to
1,000,000,000,000, and reports that cap in every result. Its binary SHA-256 is
`ab6a02edcc4e2d85bd9cd0aaafe07c0390d7f6b0f765407f8f65781ff8e15a70`.

## Native K-slot prototype findings and exact-query reuse

These are single auxiliary native comparisons with a 180-second deadline,
1,000,000,000,000 controller work units, one worker and a 512 MiB controller
bound. UNKNOWN rows report time **until the limit**, not a completed solve.

| Frozen binary cohort | Algorithm | Elapsed (s) | Result |
| --- | --- | ---: | --- |
| `ab6a02ed` | Existing exact minimum | 25.401 | K=25, same ordered canonical keys as GUI |
| `ab6a02ed` | Lazy physical K-slot | 91.829 | UNKNOWN / WorkLimit |
| `ab6a02ed` | Lazy physical + feasibility/projection/transition reuse | 84.617 | UNKNOWN / WorkLimit |
| `8917d33c` | Existing exact minimum | 23.324 | K=25, same ordered canonical keys as GUI |
| `8917d33c` | Lazy physical + exact joint-query memo | 157.802 | UNKNOWN / WorkLimit |
| `8917d33c` | Lazy physical + joint memo + physical reuse | 164.720 | UNKNOWN / WorkLimit |

Within the first cohort, physical reuse reaches the same 1,007,150 assignment
nodes / 22 counterexamples / five negative-K decisions sooner, but does not
reduce the combinatorial search. Every source preparation is about 0.08-0.10 s;
the complete admitted dictionary has 246 identities and 5,040 full PatternIds.
Opening source families alone cannot remove this measured P7 bottleneck.

The additional memo in `implicit_minimum.rs` stores only an exact original-row
range and required-pattern slice under the unchanged source binding. It is
disabled by default, bounded by entries and bytes, admits table-growth peaks
before allocation, and never stores Unknown, errored or invalid observations.
It does not learn a PB inequality, change constraints, consume historical
answers, or issue a legacy v2 coverage proof. Cache capacity 0/1/64 was checked
against all 512 existing small matrices and every original-ID optimum; all
three existing controller tests passed, including incomplete/stale evidence.

The new frozen binary is
`8917d33c83d19c05886639a596f15f4b9091521daec0282015920bacd444becd`.
Its memo experiments both visit 171,562,629 assignment nodes and find 35
counterexamples. The memo hits 170,806,976 times, misses 755,653 times, and
resets 46 times at its 16,384-entry cap. Despite over 99% hit rate, only K<6
has been excluded; no canonical query or minimum result is reached. Increasing
reuse alone does not fix the current queue-to-slot assignment search.

Keep this native K-slot implementation experimental. It is not ready for a
browser scheduler or the remaining mode adapters, and these measurements are
not evidence that the full agreed implementation plan is complete. The next
direction needs to address the combinatorial controller or the existing
canonical search rather than assume more physical caches solve it. SCP remains
on hold unless the user changes that earlier instruction. General PB learning,
MaxSAT, BDD bundles and a full GPU searcher remain excluded.

`native-probe/comparison.json` retains every raw digest, binary cohort, work
cap, source binding, output check, GNU time/RSS result and matching OS sampling
epoch. Both native baselines were independently checked against the GUI's
ordered canonical keys only after execution. The reference is never an input
to the executable. All native measurement processes and their OS samplers are
terminal; the private GUI server has a separate finite lease.

## Existing N32 reference refreshed in the second artifact

The product already selects N32 (761) at requested partition counts of at least
32, retaining TF (185) below that threshold. The experimental setter's default
185 is not the ordinary product policy at 44 partitions. The earlier TF-only
screening therefore cannot select a replacement for the current fastest policy.
No N32 product change was needed; this phase corrects the comparison reference.

Samples 063-077 use the same v2 WASM/GUI, workers 11, requested partitions 44,
and sampler epoch `93876ff8-f383-453c-9130-600cd71e3f23`. All 15 are VERIFIED,
with identical original canonical members, K=25 and policy/artifact ACKs.
All have full-periodic OS coverage. Each three-run batch is serial, with no
compiler or other owned solver running concurrently.

| Policy/block | Runs | Median first paint (s) | Fastest (s) | Range (s) |
| --- | ---: | ---: | ---: | --- |
| Existing N32, initial control | 3 | 11.757 | 11.465 | 11.465-12.195 |
| TF comparison | 3 | 15.100 | 14.505 | 14.505-15.400 |
| N32 + inverse early / feasibility off | 3 | 11.822 | 11.283 | 11.283-11.945 |
| N32 + inverse early / relaxation only | 3 | 17.202 | 17.198 | 17.198-18.294 |
| Existing N32, return control | 3 | 27.344 | 13.050 | 13.050-29.007 |

The fastest verified observation is 11.283 s with I2/F1; its median is 0.55%
slower than the initial N32 control, far from the 5% improvement criterion.
The return-control median drift is about +133%, so pooling it with the initial
control would falsely exaggerate a candidate gain. The cause of that drift is
not established by periodic memory observations. No physical policy is adopted.

For the initial N32 control, first canonical publication occurs at 4.604-4.858 s;
publication-to-paint then takes 6.862-7.337 s. These intervals include query
preparation/scheduling and rendering, not pure DFS time. They continue to locate
the principal measured P7 cost after source generation. The final sampler was
stopped after the last run; raw records remain append-only. The comparison
script now names N32 as the reference for this epoch and retains TF screening
as a separate cohort.

## Bounded two-queue conflict experiment

The next native-only candidate addresses the measured queue-to-slot assignment
explosion. `set_pairwise_conflict_ordering` is disabled by default. It queries
each newly observed queue pair over the complete original-row domain and stores
an edge only for a complete `ProvedEmpty` result. An edge remains necessary in
every narrower canonical row range. A positive pair is not treated as a
positive multi-queue proof: each slot still requires one exact common diagram.
Full PatternIds and every original alternative remain unchanged.

The bounded graph selects a still-unassigned queue with the fewest slots not
already ruled out by known pair conflicts, breaking ties by conflict degree.
Independently verified cliques give a necessary slot-count lower bound. These
are specialized physical two-queue consequences; no general PB solver,
inequality learning, MaxSAT, BDD, GPU or new full coverage-matrix backend is
introduced. The first 256 observed queues are eligible in the native probe;
later queues retain exact joint checking with no assumed pair conflict.

Graph allocation and replacement coexistence are preadmitted. Pair probes,
heuristic scans and clique scans consume the existing finite work ledger.
Unknown, resource failure and cancellation never create a conflict edge.
The three existing controller tests passed with cache capacities 0/1/64 and
graph capacities 0/1/2/3 across all 512 small matrices and all their original-ID
optima, including incomplete/stale observations. No new test or release gate
was added.

The frozen native binary is
`482bd40e319920c4f0319112a58e6cfaaf5fc9dfbc70f5128cb747908050f54a`.
Its initial serial legacy control completed in 25.125 s with K=25.

| Algorithm in binary `482bd40e` | Elapsed (s) | Result |
| --- | ---: | --- |
| Existing exact minimum, initial control | 25.125 | K=25; ordered canonical keys verified |
| Lazy physical + pair conflicts | 86.690 | UNKNOWN / WorkLimit |
| Lazy physical + pair conflicts + joint memo | 180.001 | UNKNOWN / deadline cancellation |
| Lazy physical + pair conflicts + joint memo + physical reuse | 180.002 | UNKNOWN / deadline cancellation |
| Existing exact minimum, return control | 25.720 | K=25; ordered canonical keys verified |

The graph-only run observes 49 counterexamples, 446 pair conflicts and a clique
lower bound of five. It skips 762,305 assignments, but reaches only seven
negative-K decisions. Both memo combinations reach the same 66 counterexamples,
871 pair conflicts, clique bound seven and eight negative-K decisions. They
attempt about 149 million assignments and reach no canonical query. Physical
reuse replaces 889,957 of 1,588,739 fresh physical constructions, yet does not
resolve the dominant assignment enumeration. A large cache hit/pruning count
is not itself an optimization result.

The candidate is rejected for adoption on this input. Keep the native
experiment opt-in and retain existing N32/TF product admission. This does not
prove that every possible integration of open inverse families or physical
explanations is ineffective, and does not complete the pending open-family,
browser-scheduling or other-mode stages. The counterexample provider currently
selects the first uncovered full PatternId; whether a more informative ordering
helps remains an unmeasured hypothesis, not an implemented improvement.

All five records have full-periodic OS coverage in native sampler epoch
`08f1f984-8028-496e-8146-6603076eceef`, with maximum observed gaps no larger than
2,025 ms. The two complete controls agree with the GUI's ordered canonical keys
and differ by about +2.37% in elapsed time. Native process maximum RSS is
15,324-15,444 KiB for controls and 15,404/22,180/22,200 KiB for the three
candidates; this is distinct from the controller's conservative memory bound.
Every raw report digest and binary identity verifies. All native processes and
OS samplers are terminal; the private GUI server's finite lease also ended.
No new subagent, CI run, commit, merge or deployment was created in this follow-up.

## S3 root-family ownership and streaming work

The shared arena-node decoder now drives both ordinary closed enumeration and
an experimental borrowed root cursor in `geometry_open_family.rs`. Completed
root branches are published only after insertion into the parent's union; the
original branch is published, never the accumulated union carry. The compiler
does not mutate its arena while a cursor is active. Direct root completion has
a one-time fallback, while final root completion does not re-emit earlier
branches. Only full leaf candidates leave the owner; raw family IDs do not.

The initial bounded path uses a fresh resource-authoritative compact root with
no tablebase. It retains the complete inverse catalog and all compiler arena
storage. It is not yet lazy generation of inverse temporal parents or complete
S3 integration. The cursor interprets one node per call, precharges conservative
support/hash/cursor work, admits stack-replacement coexistence, and poisons
structurally failed or cancelled owners. A work rejection before mutation may
resume with the same owner and an explicit remaining budget. Open source
completion requires both the compiler and every pending cursor to be exhausted.

Two focused tests passed: closed-source equality/target IDs with early output,
root union non-duplication and a direct Product fallback; and cancellation /
storage-failure poisoning. The prior decoder extraction separately passed one
serial/parallel geometry test, two stack tests and two actual PC-provider tests.
The existing large P7P4 test remained ignored and is not counted as verification.
The existing exact-cap family allocation test also passed after moving both
chunk-directory and node-chunk admission before allocation.

A constructor-frozen deferred-publication setting now supplies the same-path
A/B control. The producer reports bounded work and does not convert a resource
failure into ordinary complete-source evidence. Cursor heap retention is also
included in its enclosing GeometrySearch/session accounting, without counting
it twice inside the compiler's family allocation allowance.

The separate private `WasmCpuSearchSession` constructor runs either publication
setting with the same full physical coverage and existing objective finalizer.
It retains one request owner; it does not inject implicit subset observations
into the legacy complete result. Its finite geometry work cap is distinct from
the K-slot prototype's controller cap. An early candidate exposed an existing
consumer assumption that target groups are available only after enumeration
starts. Target lookup now borrows the complete target groups from the still-open
compiler when this experimental mode is active. Ordinary target availability
is unchanged.

The existing actual-PC provider fixture was extended to compare both full
objective paths against its original identity, proven minimum and reachable
universe result, and to ensure that exhausted work cannot resume as Complete.
Both tests in that fixture passed after fixing target access. Two open-family
tests and the existing exact-cap allocation test also passed. These native
checks establish local behavior, not P7 speed, browser parity or other-mode
completion. Fresh native/4195 measurement remains pending; earlier frozen
binaries preceding the `db1fcacc` cohort do not contain these S3 edits.

## Fresh native open-root comparison

The new frozen binary is
`db1fcacceed0f2345770d3eec57a38b91d152fce2f4831afff6da37704bb9707`.
Its archive binds 2,314 tracked source/manifest and private harness inputs with
source SHA-256 `5442f1b14bbf03826613c82adbadf8af5c5e2e63953dcd24351cab0bc79d97da`.
It was built before the later implicit-provider publication-mode bridge; that
bridge is not covered by the binary's measurements.

The native endpoint includes request compilation, all physical coverage and
the existing exact minimum objective, with one worker and a 180-second deadline.
The two new arms differ only in whole-root versus completed-branch publication.
Both use the same one-node decoder, bounded source policy and one full search
owner. They do not use the experimental K-slot solver. The geometry cap is
1,000,000,000,000,000,000 conservative work units; work values are preserved as
decimal strings in the derived analysis to avoid JavaScript integer rounding.

| Mode | Runs | Median full minimum (s) | Fastest verified (s) | First candidate processed median (ms) |
| --- | ---: | ---: | ---: | ---: |
| Existing ordinary control | 2 | 25.321 | 25.027 | not sampled |
| Bounded whole-root publication | 3 | 25.893 | 25.156 | 81.877 |
| Bounded early-branch publication | 3 | 25.133 | 24.757 | 4.396 |

The order was control / closed / open / open / closed / closed / open / control.
All eight reports verify K=25 and the same ordered original normalized keys as
the post-run GUI reference. Both bounded arms process 2,260 geometry candidates
and retain 246 admitted source identities. First-candidate timing includes that
candidate's physical verification; it is neither raw branch publication nor a
browser paint measurement.

Early publication lowers the full-objective median by 2.93% against its matched
closed-path control, below the agreed 5% adoption threshold. The ordinary
return control changes by +2.35%. The 94.63% first-candidate latency reduction
does not establish comparable whole-search acceleration. Keep streaming as an
experimental option and retain the product's existing N32/TF admission policy.
The complete inverse catalog remains eager, and this does not complete S3/S4/S5.

All eight runs have full-periodic OS coverage in sampler epoch
`f2c64917-df50-41d6-ba8e-d94b84e500c5`; maximum observed gap is 2,028 ms.
Process maximum RSS is 15,160-15,292 KiB for closed publication and
15,420-15,532 KiB for early publication. RSS, the admitted memory bound and
Windows aggregate process WorkingSet remain distinct observations.
Raw digest/canonical verification and this table are reproduced by
`native-probe/analyze.mjs` and `native-probe/compare-open-family.mjs` in the task
artifact directory. All benchmark and sampler processes are terminal. No new
browser benchmark, subagent, CI run, commit, merge or deployment was started.

The subsequent implicit-provider bridge now takes an explicit ordinary /
bounded-deferred / bounded-open source selection and a separate finite geometry
work cap. It charges observed work even when the producer fails, keeps the
complete admitted identity dictionary, and preserves the distinction between
an open branch and a complete source. Ordinary source work is reported as
unmetered (`None`), not zero. The owning actual-PC fixture compares all six
source-publication x physical-reuse combinations and their source bindings.
This bridge has focused native verification but no new P7 timing artifact yet.

## Compact deferred temporal-parent experiment

The next frozen native binary is
`a6a2bf9b318580cc99b28d9df3a1874b958f99f8f56077021332a4129c76620c`.
Its source archive binds 2,316 tracked source/manifest and private harness inputs,
SHA-256 `63787561702b64163d46c9d0f1a4573faa3e5d66a04976b32f20977c82c380f7`.
This cohort is separate from the earlier root-publication measurements.

The private R policy has three values: R0 retains eager parents and the existing
instantiation table; R1 retains eager parents but disables the instantiation
table and temporal-requirement domains; R2 collects only logical piece/cell/
rotation descriptors and reconstructs temporal parents when a row is first
requested. R1 and R2 disable the same precomputed acceleration structures, so
their comparison isolates the parent-storage/generation change more closely.
The production default remains R0.

For a fixed logical tetromino, each rotation determines at most one original
row projection. The decoder checks all four rotations, exact row-gap constraints,
horizontal translation and projected-cell equality. A rejected rotation is
Pending until all rotations are exhausted. Symmetric rotations are retained.
R2 allocates a fixed OnceLock cache for every logical row up front and publishes
the complete parent family at once. It does not treat an unfinished family as
empty. It currently reserves full Realization arrays, not packed-parent arrays;
memory reduction is therefore a hypothesis to measure, not an established fact.

The logical catalog still enumerates all logical rows before ordinary search.
Precharge/count queries use exact row counts without triggering parent creation.
Deferred catalog provenance uses a separate LPARENT1 identity instead of silently
claiming the eager raw-parent digest. Existing tablebase identity checks remain
intact. This change alone does not make the full inverse catalog open or remove
the complete admitted original-identity dictionary.

The existing three inverse-projection tests now compare all nine parent x
projection policies for compact catalogs, every reconstructed raw parent,
original row/start/count ordering, stable per-policy digests, and unchanged
retained-byte admission before/after materialization. All three passed with
`minimum-physical-ab,parallel`. The extended eight-row fixture additionally
checks decoder parity; that compact cohort predates the extended runtime below.
The two owning actual-PC adapter tests also passed; their full-domain fixture
now compares 18 parent x source-publication x physical-reuse combinations with
the same original identities and exact result. Work-limit behavior stays fail
closed. These checks do not establish other-mode or browser completion.

Fresh native R0/R1/R2 measurements are complete. The order was
R0/R1/R2/R2/R1/R0/R1/R2/R0, three runs per arm, one native worker and the same
full physical coverage/exact-minimum objective. All nine digested raw reports
verify K=25, the same ordered original normalized keys and 246 admitted source
identities. Policy activation and all 400 logical-row counts match.

| Parent policy | Median full minimum (s) | Fastest verified (s) | Maximum RSS range (KiB) |
| --- | ---: | ---: | ---: |
| R0 eager tables | 38.703 | 38.164 | 15,388-15,516 |
| R1 eager raw | 39.350 | 38.585 | 15,336-15,464 |
| R2 deferred parents | 38.927 | 38.359 | 15,324-15,608 |

R2 reconstructs 370 of 400 parent families in every run; only 7.5% remain unused.
It is 1.07% faster than R1 by median and 0.58% slower than R0, below the adoption
threshold. First-to-last R0 changes by -12.84%, so these descriptive differences
also contain material host/run-order drift. No new parent policy is adopted.
Sampler epoch `dd1fdf2c-6a29-45a1-97a3-40b5cceedc0d` has full-periodic coverage
for all nine runs and ended normally with 198 samples. Native comparisons are
reproduced by `native-probe/analyze.mjs` and `native-probe/compare-parents.mjs`;
the structured report is `native-probe/inverse-parent-comparison.json`.

The native cohort is separate from the following private GUI/WASM generation.
The GUI uses independent R acknowledgements from every worker and keeps the
original product result pager. No previous GUI generation is replaced.

## Third private WASM and parent-policy GUI comparison

The WSL release build completed in 11m31s. The frozen WASM is
`45f8946bbc3df06d52e7c0203cf4995a073eeabc963f2628e9c75539ae106946`
(20,704,303 bytes), with bindings SHA-256
`31ec33cd573d35f1a15f96d9440a8659afa9be93e62e85d45c0a0706fb9e8a05`.
The build-contract source digest is
`b6897a2e8611d465f0f0ab5310bad5b3dc4ffac465d71deca5d39fc7dfc87a09`
(2,460 inputs). The narrower benchmark snapshot digest is
`b43ffc0d03a0172da7ca83b70f5261681a6d57f8cd841179142895df087b1963`
(2,309 inputs); these two source schemes are intentionally distinguished.
GUI v3 has seven verified assets, aggregate GUI identity
`7caae00eab9d4828ffe616cd3a19bb08cee6935a00c45fae5a92f4bbe950e7a5`.
The normal non-experimental core library also passed `cargo check`. A new
feature-off irrefutable-pattern warning was removed with an equivalent match;
the subsequent default check passed with the two existing experimental-variant
dead-code warnings. No new test group or CI gate was added for that cleanup.

All 21 GUI measurements used the same artifact, GUI, command, N32 policy,
11 requested workers, 44 requested partitions and serial batch count 3. They
ran on an Intel Core 5 210H with 8 cores / 12 logical processors, Windows 11,
16,755,945,472 bytes of OS-visible physical RAM. No agent build or other solver
ran alongside them. Worker restriction is not an emulation of another CPU.

| Chronological batch | Parent/other policy | Samples | Median first paint (s) | Fastest verified (s) |
| --- | --- | --- | ---: | ---: |
| Initial control | R0, I0/F0 | 078-080 | 19.446 | 19.166 |
| Eager raw | R1, I0/F0 | 081-083 | 19.873 | 19.807 |
| Deferred parents | R2, I0/F0 | 084-086 | 14.006 | 13.734 |
| Immediate return control | R0, I0/F0 | 087-089 | 13.863 | 13.321 |
| Existing interaction candidate | R0, I2/F1 | 090-092 | 13.492 | 13.418 |
| Combined candidate | R2, I2/F1 | 093-095 | 13.224 | 12.867 |
| Final return control | R0, I0/F0 | 096-098 | 13.030 | 12.810 |

The initial-to-final control median changes by -33.00%. The immediate return
control reproduces the apparent early-to-late acceleration without R2. Within
I2/F1, adding R2 improves the descriptive median by only 1.99%; the final R0
control is faster still. The fastest verified observation in this new cohort,
12.810 seconds, belongs to the unchanged R0 control. Do not pool the early
19-second and later 13-second controls into a causal gain. Retain R0 and the
existing N32/TF product selection. Broader worker or platform rollout is not
warranted by this candidate screening result.

Every report verifies K=25, all 25 original candidate IDs/normalized keys in the
same first canonical result, 25 rendered fields and the unchanged lazy-alternative
contract. All initialized policy ACKs match the selected arm and artifact.
Every initial 246-row / 5,040-pattern minimum query has the same matrix digest:
`b8ece431d039953ed00a5cac5e73f93969f9fcafd02da3f3177a5e88e77f622f`.
The change in deferred catalog provenance does not change this objective matrix.

Current telemetry also localizes the P7 bottleneck. All 2,260 emitted candidates
are physically verified, the producer is complete, and active physical workers
are zero at an observed 262.615-545.405 ms across these runs. First canonical
query issuance occurs much later (batch medians 5.380-7.796 seconds), followed by
the remainder of canonical selection and paint. These are observed milestones,
not additive per-function CPU timings. They show that deferred parent creation
alone cannot explain a multi-second speedup in this fixture; minimum-cardinality
proof and canonical selection dominate after physical source completion.

OS sampler epoch `8519dbcc-27a7-4eb1-a69a-3c31e9dd18ca` ended normally with
552 samples. All 21 runs have full-periodic coverage, maximum observed gap
2,027 ms, and at least 5.458 GiB observed free physical RAM. CPU runtime clocks
and thermal state were not sampled, so the cause of the baseline drift remains
unconfirmed. Earlier Clearra observations and Qnia's existing measured 3-5 s
reference were not rerun or substituted for the current control.

Raw records are append-only under `bench/results/078-*` through `098-*`.
Reproduction: `bench/analyze-memory.mjs`, `bench/analyze.mjs`, then
`bench/compare-parents-v3.mjs`; the result is
`bench/inverse-parent-v3-comparison.json`. The comparison asserts raw-report
integrity through the owning analyzer, original canonical result, matrix
identity, policy activation, physical completion and continuous OS coverage.
Batch medians are retained separately so control drift remains visible.

All benchmark workers and samplers are finished. The finite local HTTP server
only serves the private GUI; navigation does not start a benchmark. No new
subagent was used in this continuation. The broader mode adapters, truly open
logical catalogs, browser scheduling for the implicit controller and mode-wide
validation remain unfinished; these P7 results do not complete the full plan.

## Extended deferred-parent runtime and mode parity

The R2 arm now also collects logical piece/cell/rotation projections for 7-24
row fixed fields. Its fixed OnceLock cache stores four packed TemporalParents
per family, sharing the logical ExtendedBoard instead of copying it into each
cached parent. Every rotation is checked before the complete family is
published. Static APDP eligibility consumes that complete family; an unexamined
parent can never be silently counted as a proved static or impossible domain.
R0 and R1 use the same existing raw-parent path for extended fields, which do
not have the compact instantiation table.

All original skeleton IDs/start/counts, parent ordering, exact row gaps and
instantiations are preserved. The LPARENT1 provenance remains distinct from
eager storage. Materialization performs no heap allocation: the full fixed
cache capacity is admitted and included in retained bytes before search.
Projection collection and final allocations use checked reservation errors.
The logical row catalog itself still closes before search; this is not a
claim that true open logical-catalog publication is complete.

The three existing inverse fixtures now cover R0/R1/R2 x I0/I1/I2 on both an
8-row split O and a 24-row split I. The former checks all 256 deleted-row
masks; the latter includes the exact 20-row deletion gap, a missing required
row and out-of-height bits. Full parent tuples, original row descriptors,
instantiations, stable digests and retained sizes match. The existing extended
APDP fixture also covers all parents for static O/T across the 64-bit boundary.
These three inverse fixtures, one APDP fixture and four existing resource
fixtures passed. The resource fixtures retain their original limited scope;
they do not establish full nonempty resource-failure coverage.

Four owning integration fixtures were extended without adding a test group:
compact/extended serial versus distributed Build with Include/Omit reporting;
ordinary minimum plus the unreduced score portfolio; native parallel complete
minimum/score source evidence; and VisibleSeven incomplete candidate evidence.
All four passed for R0/R1/R2 with identical original keys, per-pattern coverage
and complete exact scoring graphs where applicable. The VisibleSeven fixture
checks evidence handling, not a new observation-policy optimizer. Remaining
implicit-controller adapters are still unavailable rather than weakened.

Normal feature-off cargo check passed with only the same two experimental
enum-variant dead-code warnings. The extended native source-stage harness was
built separately from the frozen compact minimum and GUI artifacts.
It measures fresh requests at heights 8 and 24 with batch count 3, including
catalog/geometry preparation and complete physical coverage. R0/R2 x I0/I2
comparisons retain full raw source dictionaries and coverage words.
This auxiliary metric excludes the minimum solver and browser paint and
cannot by itself authorize a GUI performance-default change.

The fresh extended cohort is now complete: 16 serial processes, batch count 3,
48 samples, one native worker. Each height uses a forward/reverse factorial
order with the original control repeated last. All 246 original keys and every
bit of the 5,040-pattern coverage agree for each height, and the union is
recomputed. The analyzer reads original JSON u64 tokens without floating-point
rounding. R2 materializes 370 of 400 parents in every sample.

| Height | R0/I0 median ms | R2/I0 median ms | R0/I2 median ms | R2/I2 median ms |
| --- | ---: | ---: | ---: | ---: |
| 8 | 363.316 | 377.950 | 363.206 | 361.940 |
| 24 | 842.342 | 829.090 | 839.210 | 825.923 |

The combined source-stage gain is 0.38% at height 8 and 1.95% at height 24.
Height-24 catalog/geometry preparation falls from 10.963 to 5.060 ms, but
physical coverage still dominates. Initial/final control drift is -3.69% and
+1.64%, respectively. Maximum native RSS ranges from 10,368 to 10,624 KiB.
Retain the existing default; these results do not meet the 5% adoption rule.

Binary: `26537dc1f30c3a64f31a64ab5144fccd198b970f0fd7236c81117dce39fb203f`.
Source: `a1ee0a837cda5372b9753fbef89569b84edcf596b5f7e034b5fe05ff6c216fe6`
(2,318 inputs). The earlier preflight `extended-20260911T133105988811202Z-h8-r0-i0`
is retained as UNKNOWN: its harness supplied unequal PC line/height CLI values.
The corrected auxiliary adapter parses the original valid P7 command, then
uses the existing typed Build fixture path to set the fixed-field height. It
does not weaken the public command validator.

OS epoch `3fbc4347-3262-4f12-856b-e6653d63ccc4` ended normally with 76 samples;
its periodic observations bracket every process. No build or other solver ran
alongside the cohort. Raw process RSS comes from `/usr/bin/time -v`; OS samples
are periodic memory observations, not exact per-sample allocator peaks.
Reproduction: `native-probe/compare-extended-parents.mjs
extended-parent-cohort-20260911T133329Z.json`. The structured artifact is
`native-probe/extended-parent-comparison.json`.

## Final multithreading scope

Two independent scheduling candidates and their combination were compared:
T1 assigns larger estimated residual cubes first, retaining every cube and
assigning its internal partition ID after the permutation; T2 reduces the
requested partition budget from 4 per executor to
`min(4*w, max(32, 2*w))`. The latter preserves the existing N32 policy boundary
and leaves low-worker requests unchanged. T0 is the existing behavior and T3
combines both. The task-order switch is private bit 8192; no default has changed.
Temporary ranking storage is checked against the original memory guard;
cancellation and missing/stale/forged receipts remain non-authoritative.

Fresh private GUI batches used 11 workers with return controls, followed by a
bounded 12-worker T0/T2/T0 check. Original minimum K, original canonical IDs/keys,
lazy alternatives, worker policy acknowledgements, actual partition requests
and periodic OS memory evidence are retained. A lower worker count still does
not emulate another CPU. The revised goal closes after this comparison and
the requested fastest-result restoration.

## Final multithreading measurements and source disposition

WASM v4 SHA-256:
`e42348ec5b2c4c54c856aea2460614251a1112438535fd0edc83236416c6f93d`.
Build-contract source:
`47ceb02624ceb3a555ec5c297cb0753d9b96d717546e43139437b681a1d654fb`.
GUI asset-map identity:
`ec76011b15013f3f5f8bb2e4bd066a5a18cefbc2d1451c5d663f9adbb52721fe`.
These artifacts remain frozen under `wasm/physical-v4` and `bench/gui-built-v4`.
T2 is a private GUI-build topology injection; the product TypeScript topology
source was unchanged. T1 used private bit 8192 and checked temporary ranking
storage. Nine owning partition/protocol tests passed before the GUI comparison,
including exhaustive small matrices, cancellation, memory refusal and forged
or missing receipts.

All measurements used P7, the same CTK3-equivalent board, CPU, batch count 3,
one active sample at a time, and no concurrent build or other owned solver.
The 11-worker order was T0/T1/T2/T3/T2/T0. T1 and T3 were visibly over 35% slower
in their first batches, so their planned reverse repeats were explicitly pruned.
The remaining T2 reverse and return control were completed. The 12-worker order
was T0/T2/T0 with `--use-all-cpu-threads` explicitly enabled for all valid runs.

| Workers | Policy | Requested partitions | Batch medians, chronological (s) | Fastest valid (s) |
| ---: | --- | ---: | --- | ---: |
| 11 | T0 existing | 44 | 16.908, 16.098 | 13.069 |
| 11 | T1 larger estimated cubes first | 44 | 23.946 | 23.918 |
| 11 | T2 fewer partitions | 32 | 16.201, 38.311 | 15.438 |
| 11 | T3 combined | 32 | 23.323 | 22.946 |
| 12 | T0 existing | 48 | 37.333, 28.409 | 18.100 |
| 12 | T2 fewer partitions | 32 | 27.737 | 26.137 |

The 11-worker control median changes by -4.79%; the 12-worker control changes
by -23.90%. T2's 12-worker pooled descriptive gain is 3.35%, and its advantage
over the adjacent return-control median is only 2.37%. Its large 11-worker
reverse-batch regression also fails the adoption condition. Neither standalone
candidate nor their combination establishes a repeatable qualifying gain.
Retain the established partition/dispatch behavior.

Samples 099-116 and 118-126 are all VERIFIED: 27 valid samples out of 28 attempts.
Sample 117 is retained as UNKNOWN because the first 12-worker command omitted
the explicit all-thread option and failed input validation before search. It
contributes no timing. The analyzer permits only that identified setup failure
as an exclusion; other unverified outcomes still fail the comparison.

Every valid sample retains K=25, the same 25 original canonical IDs/keys, 25
painted fields and the lazy-alternative contract. Every base 246-row / 5,040-
pattern matrix retains digest
`b8ece431d039953ed00a5cac5e73f93969f9fcafd02da3f3177a5e88e77f622f`.
All 2,260 physical candidates complete. Worker policy ACKs and actual prepare
partition arguments agree with each arm. Full-periodic OS coverage spans every
valid sample; the maximum observed sampling gap is 2,043 ms and minimum observed
free physical RAM is 4,552,118,272 bytes. Runtime clocks and thermals were not
measured, so baseline movement is not assigned a cause.

The v4 sampler `678a6bf1-0b11-4139-9839-122f664a483e` ended with 1,003 samples.
The owned v4 server and workers were stopped. Reproduction is
`bench/analyze-memory.mjs`, `bench/analyze.mjs`, and
`bench/compare-multithreading-v4.mjs`; the result is
`bench/multithreading-v4-comparison.json`.

Before restoration, the complete owned Rust experiment was archived as
`bench/multithreading-v4-rejected.patch`, SHA-256
`42ef3cfe9f4b21920d6b7d6c0f71149557c6d81b39fc7ae3613966d092b3cf65`.
Current hashes, HEAD and empty staged diffs were checked before restoring
`minimum_hotfix_policy.rs`, `exact_at_most_parallel.rs` and
`exact_at_most_assistance.rs` to their pre-experiment state. No other dirty source
file was restored. `bench/multithreading-v4-source-archive.json` records the
ownership hashes and completed restoration. The restored partition/protocol
suite passed all nine existing tests; the default core library check passed
with the same two pre-existing experimental-variant dead-code warnings.

## Fastest retained result restored and final goal closure

Sorting the entire verified GUI record identifies sample
`069-N32-A0C0I2F1D0-w11-1789117251814.json` at **11.283375 seconds**.
Its raw SHA-256 is
`16d57b98c1071398bba8191868ec81d454d4b31e9fb1857e994b01643582f64c`.
The 12.810070-second result is the later v3 cohort's best control, not the
minimum across all retained cohorts. Restoration follows the user's explicit
minimum-observation preference. In its original v2 batch, I2/F1's median was
0.55% slower than the initial N32 control; this restoration does not establish
general performance superiority or promote a product default.

The private GUI at `http://127.0.0.1:4195/` now redirects to
`/N32-A0C0I2F1D0/`, with 11 requested workers, 44 requested partitions and batch
count 3. It uses the identical frozen `wasm/physical-v2` and `bench/gui-built-v2`
assets from sample 069. WASM SHA-256 is
`22d81ee5fd5d1cf109d9f89834747d97594c050ae7a7344e5a23721262e48384`;
build-contract source SHA-256 is
`6bc864b07022d6c8451542c6f879139ca38e5d6785bac59e536e352ef7b3f5df`;
GUI identity is
`dbc2a132b13dd44c6990a1d66829331f770c0837bf4dd4a444dc01fa35f88683`.
All seven GUI assets, bindings and WASM bytes passed integrity verification.
The restoration server changes only the landing URL from the frozen v2 server;
the measured GUI and engine bytes remain identical.

One final fresh batch, samples 127-129, completed in **15.679255, 15.572630 and
15.617880 seconds** (median 15.617880). All three are independently VERIFIED,
with identical command, engine/GUI identities, original canonical result,
complete physical source, matrix and policy ACKs. The historical 11.283-second
time was not reproduced in this final resource epoch; no such claim is made.
The final OS sampler `b859bc68-ec11-41aa-92de-e718fa2dd609` ended normally with
68 samples and full-periodic coverage of all three runs.

`bench/finalize-restoration.mjs` independently selects the retained minimum,
checks the restored sample identities, source restoration and ended samplers,
and writes `bench/fastest-restoration.json`. This artifact separates historical
selection from fresh verification. The active private server is
`bench/serve-restored-fastest.mjs`, session
`f3f4353d-9ced-4f34-af08-8a2bb906a2eb`, with a finite 90-minute lifetime.
Navigation starts no benchmark. All owned search workers and resource samplers
are finished; only the private HTTP server remains available for the user.

Restoration scope is the exact private GUI/WASM/policy and the three owned
multithreading files. Earlier validated physical experiments remain in the
current dirty worktree with their existing experimental boundaries. The revised
goal is complete. Remaining open-catalog, controller-scheduling and mode-wide
work is outside the user's final scope. No new subagent, CI run, deployment,
merge or background monitoring was started for this final continuation.
