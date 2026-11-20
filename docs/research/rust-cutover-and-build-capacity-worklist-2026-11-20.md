# Rust cutover, duplicate ownership, and build capacity worklist

Baseline: integration branch at `4a11de7c` plus the explicitly preserved
working changes. This worklist classifies remaining work; it is not an
acceptance receipt or a replacement for the full relation roadmap.

## Rust transition inventory

| Plan | Current effective path | Remaining work and dependency |
| --- | --- | --- |
| R01, R07 | C ABI 23 remains public; no tracked external consumer is confirmed. C packing/build and independent oracle are distinct from data-only FFI imports. | Establish live link/symbol and ownership contracts. Keep the public ABI and independent oracle until scoped evidence allows retirement. |
| R02, H01 | Published CLI/Discord, Web and packaged Desktop select Rust CPU explicitly. Library/default and explicit C comparison routes differ. | Use one shipped App context constructor; retain explicit C reference and separate generic defaults until feature-by-feature parity. |
| R03 | Typed Build Probability/Cover V2 and legacy Build Cover are different contracts. Legacy C is the general solver; Rust legacy has a narrow proved-empty adapter. | Verify complete original ordinals on actual C/Rust Core requests, then connect complete positive Legacy Observed data at the App authority seam. Do not treat prefix qualification or finite fixtures as Observed parity. |
| R04, R06 | Extended 7–24L Rust exists; ordinary native multiworker advance can block for the whole solve. The 7L exact-admission seam is private. Compact canonical failed-queue has a scoped direct Rust implementation for a single CPU/Auto worker and complete Oracle coverage. | Run exact Core/App qualification for compact before treating it as verified. Cooperative/distributed compact and multiworker support remain open. Finish sliced parity before expanding 7L or activating products; keep variable depth, unsupported rules and request budgets explicit. |
| R05, H03 | Setup/score and typed retained Sc3 already have Rust paths; Sr3 one-worker and Sr2 input restart differ. Compact PC2 worker proof is private with no public issuance. | Compare each continuation contract, preserve checkpoint vs restart and cancellation, exercise real host apply/merge and independent physical witness before publishing PC2. |
| R08–R10 | Packaged CLI already links Rust; release ProductE2E and `clearra-pc-artifact` really execute C. | Qualify family-specific defaults, then split explicit C oracle inputs from Rust release evidence and change the dependent artifact identities rather than just flipping a Cargo flag. |
| R11–R12 | C sources, public ABI and qualification C oracle remain. | Retire only actual consumers proven unused or migrated after V03/H05/performance per scope, preserving the independent oracle, required data types and historical receipts. |
| H02, H04, H05 | CLI/Discord wrappers, Chromium and Desktop bridge have scoped coverage, not full deployed readback or a packaged Windows startup run. | Exact non-publishing x64 package/Job/WebView2 and live Discord/Pages readbacks. No verified pre-main package producer exists; build once and bind run/attempt/job/digests. |

## Duplicate ownership work that can proceed without a backend cutover

1. Web/Desktop workspace dispatch: share the fixed tool registry and
   component dispatch; keep worker factory, navigation, visibility and
   instrumentation host-owned.
2. Workspace timers and export/copy: share elapsed/cancellation plumbing
   only, not solver policy, Clipboard activation, memory caps, or formats.
3. Discord plain-value cloning/freezing and field normalization: share
   structure; keep each result schema and attack redaction scope distinct.
4. Rust terminal projection: consolidate ownership release and common
   public field projection; keep exact allocator limits, redacted audit,
   ordered duplicate fields and fail-closed result semantics.
5. Build ownership: one storage-policy authority for both shells; do not
   carry forward two policy implementations merely to keep launcher names.

## Build storage replacement

MMMM and RoveZero share managed defaults of **8 GiB per slot, 12 GiB for
the catalog, four completed plus one active, five-second monitoring**;
MMMM checks both byte caps while the child runs. Accelerate and Rove4k
provide ownership/path principles but no equivalent enforced byte cap.
Adopt the MMMM contract instead of Clearra's count-only newest-five and
per-digest directory rules. Limits measure logical file bytes and are
checked at admission, during managed execution, and before completion;
sampling is not a filesystem quota and cannot prevent every transient
overshoot. Fail closed on incomplete inventory. Only inactive,
ownership-proven regenerable outputs may be retired. Exported release
artifacts, reports, credentials, foreign/active leases and historical
WSL storage are not cleanup targets.

Windows Node/PowerShell launchers, CI, Cargo defaults and WSL/WASM must
ultimately reach the same capacity owner. The existing custody of
processes, WSL isolation, credentials and exact artifact acceptance is
not part of the storage rule being replaced. Do not provision or delete
a distro as part of the port. Keep bridge/reader lifetime and independent
ABI oracles intact while removing duplication.

The branch's managed Node and PowerShell launchers now share admission
and completion checks and the global catalog lock; Node samples during
execution. On Windows or under an existing supervisor, do not claim
descendant shutdown if it could not be observed. PowerShell direct calls
still lack a live monitor; bare Cargo, the guest's separate WASM target,
and GUI experiment direct-owner calls must be brought into scope. A full
catalog with readers of unknown state blocks a fresh slot instead of
discarding a completed output.

Implementation order: disjoint GUI/Discord/projection owners and Rust
family fixtures may proceed now; App route integration and Rust release
defaults depend on typed producer seams and parity; WSL/Cargo live
storage changes depend on a single admitted slot and cleanup authority.

## Multiworker and progress qualification

The compact native Rust queue dynamically hands out **whole, sealed
Geometry branches**; a worker verifies candidates as it enumerates them.
The last successful claim may mark the queue exhausted, but exhaustion
is not cancellation and does not end in-flight branches. An exhausted
probe may skip the mutex because this queue has no producer after seal.
Do not carry that assumption over to a queue that accepts donated work.
An initially small branch can become the entire tail; slicing or donating
part of an already-running branch needs its own exact ordinal coverage,
reservation, cancellation and late-work proof. The current change does not
add a fixed small-input cap or split active branches.

For Native C PC All/Unique, the Rust adapter first materializes a C
buildable Geometry DAG, then statically distributes graph tasks. The
independent BuildUp pass subsequently claims chunks of retained
candidates. C raw product Tiling instead streams serially to preserve
global sink caps; the older sharded runner is a test reference.
Raw Tiling does not measure combined Geometry and BuildUp. These are useful
correctness references, but the task unit, phase ordering, and retained
candidate volume differ. The positive 2L `IIOOO`, no-hold CountAll fixture
compares normalized solution keys and records per-stage evidence at
requested widths one and two. A missing active BuildUp worker count
cannot qualify a C/Rust parallel speed comparison. For timing, compare
old/new Rust on one runner with the same options, worker width, fixture and result
digest; separate C-vs-Rust architecture comparison from Rust-vs-Rust
scheduler cost. Keep peak and per-stage work separate from wall time.
A lock-attempt regression is not a wall-time or memory result.
For the remaining expanding-branch tail, start with a bounded suffix
donation at an enum cursor boundary, only in response to actual idle
demand. Preserve the original branch plus a checked subrange/lineage;
the compact consumer's filtered rank is not a raw Geometry ordinal.
Prove gap-free final merge in original order and reserve the extra
cursor before transfer. If a large tail is a single in-flight BuildUp
verification rather than Geometry enumeration, suffix donation cannot
split it; measure the two cases separately.

Progress reports observed stage work, not a percentage of unknown total
search time. Geometry and BuildUp may both advance; a later event from
one does not complete the other. Sending the last root to a Web verifier
does not finish the verifier's in-flight Geometry. Native polling can
miss intermediate events; report the counters actually observed rather
than infer missing ones. Dependency checks can use their own progress
when they have an independent owner and a real count; do not serialize
the solver to make the progress bar easier to render.

New Rust code follows the common [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html):
named states, explicit ownership, module-scoped imports, and no recovery
from a poisoned branch lock. Chromium-specific threading rules do not
apply to this Cargo repository.

## Whole-C port and non-regression evidence

The migration denominator is the linked C source manifest (119 translation
units, 25 public headers). Treat the six additional C test-oracle units,
native memory/callback ABI, and explicit C benchmark arms as reference
contracts. An equivalent production path does not require a second unsafe
allocator or a spelling-identical Rust copy of the C API. A feature flag,
FFI declaration, or switched default alone does not prove equivalence.

| C execution responsibility | Rust authority and outstanding evidence |
| --- | --- |
| Board/field/piece/rule/supply | Core domain, geometry, piece registry, ordered rules and supply automata. Test custom packing shapes, terminal replay, row-remapping and exact clear-state semantics separately from standard PC. |
| Candidate/Geometry/ILC/APDP | Compact and extended catalog/geometry and typed projection. Reuse only equivalent exact predicates; a necessary domain superset is not a C exact-realization answer. |
| BuildUp and reachability | Reuse Rust BuildOrderGraph, physical/kick witnesses, supply filtering and cost annotations. Keep geometry-only and finalized witness capabilities distinct. |
| Coverage/reducers/scoring | Compare candidate assignments and exact original pattern ordinals—not only the number of visible queues or displayed percentage. Preserve exact result scope across full-height and continued searches. |
| Resource/parallel control | Distinguish requested, admitted and useful worker participation; keep reservation, termination and incomplete evidence typed. Do not insert timing probes in default worker hot loops. |
| Product and tooling callers | Native CLI/GUI/Discord, release verification and offline artifact tooling need executable Rust routes. Keep real C as a named differential-only backend. Do not substitute a Rust-vs-Rust comparison after changing defaults. |

ProductE2E's C-specific percent and GPU proofs remain native reference
evidence. The Rust product process suite is separate; it must qualify on
the exact selected CLI before it can replace a product-process acceptance
input. Do not reclassify a native-C receipt by renaming its subject.

For C-vs-Rust runtime comparison, build one release example containing both
implementations once. Run A=C, B=Rust, B=Rust, A=C as separate supervised
processes on the same runner, with the same compiled request, worker budget
and disabled optional assets. Verify complete normalized identities and
coverage before interpreting time or peak-memory deltas. Keep engine time
separate from owner-supervised wall time. Report censored, mismatched,
resource-limited and serial-fallback arms explicitly. A runner with fewer
than 11 logical processors does not qualify the 11-worker case. Hosted
qualification does not replace the Windows proof or release a product.

## 2026-11-20 deployed path and local ABBA readback

The [Pages deployment identity](https://daejunnom.github.io/Clearra/clearra-build-identity.json)
reported v0.8.4 with both source and engine bound to `fb795810`. The live
coordinator/verifier JavaScript and WASM manifest matched their listed
SHA-256 values, and the manifest agreed with the source and engine. This
is asset readback, not a timed browser session.

- v0.7.4 ordinary Web PC used a WASM Geometry/candidate producer and up
  to N−1 verifier realms. Its Tiling roots were a separate route.
- v0.8.4 eligible compact CPU CountUnique distributes natural multiset
  roots to separate WASM verifier realms. Resource/observation guards
  can instead select streaming Geometry plus parallel BuildUp, or serial.
  Empty board with empty Hold is normally the Opening preset.
- The integration candidate retains the ordinary Web split while
  distinguishing root issuance from completion and releasing transferred
  batch memory sooner. Native Rust uses a shared in-process queue and
  verifies candidates as their Geometry branches are enumerated; this is
  not the browser protocol.

The `core-c` tree at `fb795810` and the measured `ac9d94ab` is identical
(`9c01f6a0ad6ea1264c1283630183ea694802cffd`). At the native bridge,
worker-report and completion handling changed, not this fixture's
packing algorithm. Older C rules did change: `62320310` corrected the
SRS+ I half-turn table and switched to passing the verified Rust rule
table into C; `f504c8f0` again omitted occupancy-equivalent O half-turns.
The tracked 456,923 baseline was Rust/WASM, not a historical C oracle
under the same request and rules.

The C packing and movement source paths are unchanged from v0.7.4.
Historical C BuildUp also gained finite-source terminal hold release
and generalized the operation-count bound; neither switch changes a
10-lock standard-bag query by itself. The changed C bridge lowers the
same exact 10×4 target occupancy and required cells for this fixture.
Both current CPU implementations get their SRS+ kick offsets from the
canonical Rust table; reachability and first-success implementations
remain independent.

Local WSL provided 8 logical CPUs. All twenty subprocess samples ran in
serial C/Rust/Rust/C order at source `ac9d94ab`, with release example
SHA-256 `632c662f06d7b99bb67347c4fbf11fe772c60117d638c84203e3a707c0178c56`.
The input used the historical native 4L Scenario request for the two
standard-bag fixtures, not the GUI Opening request. Compare canonical
counts and ordered keys for CountUnique; optional coverage is not part
of that comparison. The fixed queue also checks complete coverage.

| Fixture | Workers | C elapsed (mean) | Rust elapsed (mean) | Result |
| --- | ---: | ---: | ---: | --- |
| Fixed 2L, CountAll | 1 | 20.71 ms | 15.22 ms | 4 keys and fixed-queue coverage match |
| Existing field P7, CountUnique | 1 | 180.76 ms | 35.34 ms | 246 keys match |
| Existing field P7, CountUnique | 2 | 130.78 ms | 30.73 ms | 246 keys match |
| Existing field P7, CountUnique | 8 | 97.91 ms | 45.36 ms | 246 keys match |
| Empty 4L P7P4, CountUnique | 8 | 1,258.15 / 1,252.04 s | 99.20 / 102.11 s | Raw timings only: distinct solution families |

Both large C samples returned 456,842 with `cts1:d4b34c7820453afc`;
both Rust samples returned 456,923 with `cts1:98ebe8726537b29f`. The C
receipt reports 1 Geometry DAG worker, 8 candidate-processing workers,
and 1 final BuildUp worker. Its raw Tiling stream is a different
serial-only path. The numbers do not prove that one set is a subset of
the other. The discrepancy is already present in the native retained
packing sink after buildability filtering. Next, diff the full
canonical key sequences in both directions, replay an actual differing
identity, and check raw C DAG membership, realization pruning and
predecessors, BuildUp, and sink dedup in order. Change rules and worker
width as separate variables.

Rust's P7 peak solver RSS at 1/2/8 workers was 11.63/17.38/49.75 MiB;
the 65,536-slot private reachability table accounts for a structural
5.25 MiB per initialized worker. On the large case, raw solver RSS peaks
were about 0.86 GiB (C) and 4.47 GiB (Rust). They are not equal-work
memory qualification, and the smaller host supervisor's sampled peak
must not be substituted for solver RSS high-water. No release or
full C-facing ABI qualification follows from this comparison.

The local canonical-key writer and streaming comparator were exercised
against the existing-field P7 input at `8684bce0`. All 246 sorted
keys matched, with zero exclusive members on either arm, and independent
length-prefixed hashes matched. Restricted single-key probes are a
separate diagnostic: passing a row sink or an explicitly supplied Rust
verification universe does not prove full C Geometry DAG membership.

The same full P7P4 capture resolved the set relationship: 456,842
shared keys, zero C-exclusive, and 81 Rust-exclusive. Every exclusive
key contains the same vertical I spanning original column 9; C also
retains 76,845 other keys with that placement. Rightmost I occupancy
alone is not a rejection condition. Because that I owns column 9 of
all four original rows, no row can clear before it locks; the entire
column is free for vertical hard drop until then. For four diverse
exclusive keys, the exact C catalog contained all ten rows and both
the production row sink and an owned C BuildUp admitted the key when
supplied directly. Rust independently returned the sampled keys and
complete lock/hold/clear witnesses in a restricted verifier. This
rules out per-key C kick/BuildUp rejection for those four and leaves
the entire-family C Geometry generation/task path to diagnose.

The original C graph-only run (`90021f1f`, unchanged C engine) built
243,639 nodes in 306.60 s. Full one-task and 32-task raw traversals both
emitted 29,823,777 paths, with a shared C-accepted right-wall-I control
present and all four sampled exclusive keys absent in both. Thus this
sampled deficit predates splitting, transport, BuildUp and reduction;
it is not evidence of a dropped parallel output batch.

One witness has an inverse-clear T spanning original rows 0, 2 and 3.
On the exact 8-cell remainder its forced three-cell elbow is contained
in an admitted C row, but no second static partial spans all four
cells in the original frame. The C APDP analysis previously used that
as a negative, despite the existing catalog realization after clearing
row 1. The fail-open change keeps the complete feasible SameTile
domain whenever an admitted row has no such static pair. It preserves
kick data and other pruning. This is a soundness correction to the C
reference; full repaired-family agreement and timings are separate
observations and must come from the corrected exact-source run.

The bounded C regression is red at `fa43471e` (only the actual
inverse-clear candidate membership assertion failed) and green at
`f6d3a800` (`packing_tests` passed). The corrected 8-worker P7P4
ABBA was started at `f6d3a800` with binary SHA-256
`0e5fa71ba0f77c19e200201936df81da8205b9acdb9e42e92989d783aa389d63`.
This locally supervised run remains distinct from the old timing
receipts. Require its terminal summary, four clean samples, and the
same complete count, set hash, and ordered key digest on every arm.

The corrected C's first full run still returned 456,842 and
`cts1:d4b34c7820453afc`: the bounded APDP defect was fixed but does not
explain the full 81-key deficit. Two clean Rust samples completed at
100.543 s and 99.835 s with the expected full hash. Because the C arm
was already semantically ineligible and the product cutover no longer
uses it, the redundant second long C sample was stopped in its exact
owned unit. This is not a completed equal-work ABBA. Preserve the two
Rust receipts and their byte-pinned solver for the separate Rust
revision comparison.

The candidate's Rust `clearra-wasm-abi` completed an offline
`wasm32-unknown-unknown` compile check at `375ffdf5`. The native
witness, C-only correction and notes do not change that Rust target's
production code. No current browser execution or latency is inferred
from compilation.

Check the build recipe as well as the binary name when interpreting
future results. The Windows release CLI uses `wasm-cpu-runtime`, as do
the Rust product tests above. At the measured reference, the canonical
ProductE2E producer built `native-c-core,webgpu-search` without
`wasm-cpu-runtime`.
That resolves to the C reference context and seals an archive-bound
receipt; it is not shipping-Rust CLI process evidence. The current
percent golden also asserts C-only progress and coverage markers. An
eventual exact-release-binary E2E migration must update these to typed
evidence while preserving exact source, run, attempt and binary binding.
Keep the separate C differential tests in RustExact.

## Product cutover and first Rust-only reuse candidates

The active CLI line no longer selects C from a Cargo feature
combination. Desktop's host also no longer exposes a C-link feature.
The C archive, explicit Core/App comparison entrypoints, RustExact and
the C sanitizer lane are still available on their named routes.
The release RustProduct shard now requires the Windows release build's
actual executable and verifies a separate receipt binding its bytes
to the version, source SHA, run and attempt. There is no second C
producer or rebuild fallback if the shipping input is absent.

The distinct Rust optimization commits defer per-worker reachability
evidence allocation until concrete keys need it and reuse an existing
decision node when a language union preserves its exact children.
The cache width and replacement semantics are unchanged. The new
Rust/revision ABBA mode checks both managed binaries, each embedded
source SHA, actual worker participation, complete counts and ordered
keys before comparing time or kernel RSS. The first tests of the
runner and the release/process boundaries are local; they do not
qualify peak-memory or end-to-end product improvement. Distinguish
the old C baseline from this independent Rust-only measurement.

### Published Web reference and native cutover verification

The public Pages WASM manifest identified source `fb795810535e6b85dcb201c0f5c5ea8d6a3fad8d`
on the 2026-11-21 readback. This is a source and artifact identity,
not a browser performance measurement. The published Web worker
already called the Rust distributed WASM ABI; its worker coordinator
is not the C Geometry producer used for the historical native
comparison. The Rust cutover changes native CLI/Discord, Desktop's
host feature boundary, and which executable satisfies RustProduct.
It does not replace C with Rust inside the already-Rust public Web
worker.

The first actual C-free Linux CLI run at `95bcb66f` found both stale
projection assertions and a real compatibility failure. The old
explicit-count `sc2` checkpoint carried a two-row visible board but
only one row of packing work; Rust's ordinary frame then rejected the
area as non-tetromino-aligned. Reuse the legacy exact-frame proof for
that checkpoint version without shrinking the displayed field or
dropping higher occupied cells. Keep the old token itself in executable
coverage; do not take a fresh-format substitution as compatibility
evidence. No post-fix acceptance is implied by the original run.

For the Rust-only comparison, a third reuse change moves the existing
fixed-format Board64 canonical string into the result rather than
allocating and copying it again. The estimate of about 100.2 MiB
avoided at 456,923 ten-placement keys is aggregate copy traffic, not
expected peak resident-memory savings. Comparing exact
`f6d3a800` with `1f95ec89` covers the three reuse changes together;
use receipt-level evidence to report execution time, peak memory and
ordered output identity. It does not attribute an end-to-end delta
to any one change.

The serial native-host comparison completed four clean slots for
each fixture. All slots matched the actual worker width, complete
count and ordered canonical key digest; direct receipt checks also
found the same coverage digest in each fixture's four slots.

| Fixture | Workers | Mean solver time A → B | Max sampled solver time A → B | Peak solver RSS A → B |
| --- | ---: | ---: | ---: | ---: |
| Existing 4L field, P7 | 2 | 18.156 → 16.375 ms | 18.430 → 18.583 ms | 17.62 → 9.75 MiB |
| Existing 4L field, P7 | 8 | 18.428 → 11.859 ms | 19.662 → 12.015 ms | 48.38 → 17.88 MiB |
| Empty SRS+ 4L, P7P4 | 8 | 103.262 → 99.251 s | 106.021 → 99.499 s | 4,477.79 → 4,631.34 MiB |

A was `f6d3a800cbdc615da75b488f07a63da25ae9ab85` and B was
`1f95ec89281a3c61a64ad3f65823addb8b0215f9`. Both pinned examples
executed the same Rust CPU route on one host; the example's retained
C comparison linkage was not a product dependency or measured solver.
The ordered digests for the 246-key and 456,923-key fixtures were
`b54515c71970a04fd3a159dcb1d5af4e2f21a069ba629405a5fe432fd641e118`
and `8fda9566962e9235a4fa2a1497db2b7b88e94ec375e1bd9077382aee631afa6f`.
The full receipts are local generated evidence under
`_local/artifacts/benchmarks/native-rust-family/rust-reuse-abba-1f95ec89/`.

Neither the 2-worker maximum nor the full-input memory peak supports
a blanket non-regression claim. The latter rose by 3.43% even as the
mean time fell by 3.88%. A saturated lazy reachability cache adds only
the 6 KiB payload directory per initialized worker to its direct
retention; that accounting neither explains the RSS delta nor rules
out allocator or worker-lifetime effects. Continue by isolating the
lazy cache, unchanged-node reuse and canonical string move with
adjacent pinned revisions. Keep independent memory containment and
work equality for each ablation. None of these measurements is
published Pages, Apple/Windows hardware, or release acceptance evidence.
