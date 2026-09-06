# Qnia minimum-cover stage comparison — 2026-09-06

Status: diagnostic measurements only. This does not establish Clearra release readiness or satisfy its 20-second GUI requirement. The Clearra solver and product dependencies were not replaced with Qnia/HiGHS code.

## Reproduction boundary

The comparison checkout is `_local/qnia-sfinder-reference`, upstream commit `9f2000252a99f6e8b25a0ffbf461d894a78c3766` (release 2.7). Its Git worktree remained clean. Only its declared `tetris-fumen@1.1.3` dependency was installed locally, with lifecycle scripts disabled and lockfile changes disabled. The prepared upstream PC/HiGHS WASM and legal-board pack were used without a Rust rebuild.

The comparison-only `_local/qnia-stage-benchmark.mjs` imports upstream modules. Exact, single-occurrence loader markers insert timing observations in memory; they do not modify the checkout. Every run records the revision, asset/source hashes, original candidate order, concrete queue order, geometry masks, coverage words, backend result, and selected keys. Output files are create-new-only.

An important input correction was discovered before executing the solver: the [upstream hard-minimals benchmark](https://github.com/Qnia28/sfinder_wasm/blob/9f2000252a99f6e8b25a0ffbf461d894a78c3766/scripts/benchmark-hard-minimals.mjs) Fumen is board `0xf03c0f03c0`, the horizontal mirror of the requested CTK3 board `0x3c0f03c0f`. The harness generates the latter explicitly and validates its decoded mask. The measurements below are for the requested left-hand field, not silently for the mirrored benchmark.

Input: four lines, board `0x3c0f03c0f`, `*!` (all 5,040 seven-bag permutations), hold enabled, no save restriction. The [public wrapper](https://github.com/Qnia28/sfinder_wasm/blob/9f2000252a99f6e8b25a0ffbf461d894a78c3766/src/minimals-wrapper.mjs) defaults to `UseHiGHS="auto"` and `exactHumanQuality="Fast"`. A second fresh process explicitly selects HiGHS while retaining Fast.

## Observed stages

Node v24.16.0, direct upstream WASM, one fresh process per condition. No concurrent Clearra build or GUI benchmark was running. One sample per condition is not a statistical speedup claim.

| Stage | Public auto / Fast | Forced HiGHS / Fast |
| --- | ---: | ---: |
| PC WASM/legal-pack setup, excluded from feature total | 29.6 ms | 27.4 ms |
| Full solution enumeration | 96.8 ms | 91.2 ms |
| Coverage and quality aggregation | 45.1 ms | 41.8 ms |
| Numeric matrix construction | 20.6 ms | 18.5 ms |
| Primary kernelization | 27.0 ms | 24.3 ms |
| Minimum-cardinality stage, including lazy HiGHS setup | 30,005.0 ms | 30,574.4 ms |
| HiGHS solve call alone, nested in preceding row | 29,979.1 ms | 30,551.7 ms |
| Secondary quality plus dispatch | 713.8 ms | 776.6 ms |
| Other input/finish work | 49.4 ms | 47.5 ms |
| Feature total | **30,957.7 ms** | **31,574.4 ms** |

Both runs returned 246 distinct solution candidates, all 5,040 covered patterns, minimum cardinality 25, HiGHS status `Optimal`, and the same exported matrix SHA256 `afffd1e5d0f187d96a6f0923de2cc3ab597ba4596feea485cfc5e3ca3a797c3e`. The reduced primary kernel had 1,389 constraints, 158 candidates, 15,128 incidences, and zero forced candidates. Auto selected HiGHS itself.

Artifacts (ignored local diagnostics):

- `_local/qnia-benchmark-results/1788689283191-auto-afffd1e5d0f1.json`
- `_local/qnia-benchmark-results/1788689333843-highs-afffd1e5d0f1.json`

## Semantics that remain different

Qnia's PC route reaches `reachable_placements(... Physics::Jstris)` in its [movement implementation](https://github.com/Qnia28/sfinder_wasm/blob/9f2000252a99f6e8b25a0ffbf461d894a78c3766/rust/pc-core/src/movement.rs). Clearra's current source fixture defaults to SRS+. The candidate counts agree, but coverage equality does not: after reconstructing the canonical candidate keys and matching concrete queue columns, 496 of 1,239,840 coverage edges differ (approximately0.0400%), across four candidate rows. This is a measured semantic difference, not proof that kicks alone caused it. Solver-only before/after measurements therefore use the actual Clearra wire matrix unchanged, not Qnia's matrix as a substitute.

The [HiGHS adapter](https://github.com/Qnia28/sfinder_wasm/blob/9f2000252a99f6e8b25a0ffbf461d894a78c3766/src/highs-cardinality.mjs) uses a binary cover model and requires an optimal result with zero relative MIP gap. It does not receive a known cardinality of 25 from this harness.

Both observed secondary results were `fast-2x2` with `humanQualityExact=false`, consistent with the [adaptive Fast contract](https://github.com/Qnia28/sfinder_wasm/blob/9f2000252a99f6e8b25a0ffbf461d894a78c3766/src/min-cover-adaptive.mjs). They preserve exact minimum cardinality, but do not prove the secondary human-quality ordering. Clearra instead requires its exact first canonical portfolio. Neither the quality criterion nor a Fast completion is a substitute for that contract.

These Node timings also exclude browser worker initialization, transport, coordinator scheduling, rendering, and GUI elapsed-time conventions. They identify Qnia's dominant primary stage on this machine; they neither validate nor invalidate the user's earlier approximately 19-second observation under unspecified settings.

## Published Clearra WASM: isolated stage run

The unchanged published `512225a45cd548eeb7988ad5ab6d945dc1f4dfdcdaacc657b9a601d02d1cf365`
artifact was driven by `_local/clearra-minimum-stage-ab.mjs` in Node. This is a
diagnostic host: 11 compute workers plus a control-only manager, no IndexedDB
journal and no idle assistance. It is not the GUI scheduler or native CLI.
Worker/module initialization is outside the source time; producer, verification,
partial results and their merge are inside. Candidate batches128 and caller
budget8192 differ from the adaptive browser host. No known minimum or witness is
injected. The returned typed product was successful with exact25 and the original
first canonical set.

| Non-overlapping stage | Observed time |
| --- | ---: |
| Module and worker boot, excluded from total | 404.8 ms |
| Source preparation | 11.9 ms |
| Source Geometry, verification and raw result merge | 128.9 ms |
| Minimum proof including preparation | 47,536.0 ms |
| First canonical selection and product seal | 32,357.7 ms |
| Feature total, including small dispatch gaps | **80,042.6 ms** |

Within minimum proof, the first published query appears after302.3ms; the two
external waves take34,116.3ms (AtMost25) and12,595.8ms (AtMost24). Their summed
worker call times are321,540.2ms and124,285.2ms, respectively, overlapping across
workers. These sums are CPU work proxies and are not additive wall time.

The first two canonical waves take27,246.6ms and4,315.1ms. Their matrices have
245/239 rows and5,041 constraints, including the selector constraint. This run
has26 canonical waves in total, of which24 follow the first two. Those remaining
waves take less than0.7s combined.
The separate proof/canonical matrix domains in the actual wire identify the
stage transition; cardinality values alone do not identify a stage. An early
canonical query may be a negative selector proof, not necessarily a positive
forced-row search.

Artifact: `_local/clearra-stage-ab-1788689522366.json`. The first original query
packet is saved under `_local/exact-cover-fixtures/wasm-512225a45cd5-*.bin` with its
wire SHA checked. Candidate/queue comparison against Qnia was subsequently
performed as described below; full incidence equality does not hold.

The user's revised boundary accepts approximately0.4s for ordinary Build/PC
search and prioritizes minimum-cover optimization; the previous0.1s Build target
is no longer a release gate. The0.129s diagnostic source time must not be
presented as the GUI's full solution-display time or subtracted directly from
the GUI0.4s to claim a measured rendering cost.

## Actual-matrix measurement scope

The published512 query packet has been decoded with its wire SHA, target width,
246 rows and5,040 columns checked. Qnia piece masks were converted into Clearra
canonical keys, sorted in Clearra order and checked against the actual product's
candidate-map SHA; all25 returned canonical member IDs/keys were independently
cross-checked. P7 columns were reordered by concrete queue strings using the
source-defined `IOTSZJL` permutation order. The resulting comparison finds
242 identical coverage rows and four different rows (142 different64-bit words,
496 different incidence bits). It does not equate the two physics contracts.

The diagnostic export
`_local/exact-cover-fixtures/ad8af9326bd6a1eaa3c25747f33d6b9c1ed825601c1431c22b5aec636e7563b9.json`
contains the actual published Clearra coverage matrix. The known answer25 and
returned witness are not supplied to the standalone solver. This permits a
same-input scorer A/B without rebuilding the large App exporter first.

The new ignored Clearra exporter `ctk3_export_exact_cover_diagnostic_matrix` has not yet been compiled or executed. It preserves source row order, normalized candidate mapping, 64-bit coverage words, typed per-piece masks, concrete queue order, and separately validated matrix/candidate/queue hashes. No minimum or witness is an input.

A comparison must first validate hashes and normalize row identities and queue columns. Raw source order, product canonical key order, and Qnia key order must remain separately identifiable: minimum cardinality is permutation-invariant, but branch order and first-canonical-selection time are not.

The proposed standalone `_local` coverage-only native probe uses the existing Clearra coverage library and shared native scheduler helper. Its dependency graph excludes App/WASM and reuses the canonical target directory. It is diagnostic work, not evidence that the CLI product uses that native scheduler. Solver-only results and GUI end-to-end results must be reported separately.

## Coverage-only native scorer A/B

The standalone diagnostic executable was compiled once in release mode, SHA256
`29314002732dae617bca36167ce86a3e10f5e3a660b5822df186f6b74f20cbe1`.
Both runs use the actual Clearra `ad8af932…` matrix, normalized lexical candidate
order,11 compute workers, four partitions per worker and no idle assistance.
The old/new repair scorer is selected only in the comparison build; no
known cardinality or witness is provided. The two runs were CPU-isolated and
sequential. One run per condition is not a statistical performance claim.

| Native diagnostic stage | Old per-bit repair scoring | Word-mask repair scoring |
| --- | ---: | ---: |
| Exact minimum cardinality | 7.523s | 8.072s |
| First canonical selection | 10.553s | 10.908s |
| Solver total | **18.076s** | **18.981s** |

Both results prove25 and return the same first canonical25 keys/indices. The
first three hard waves have matching proposal/prune counters. The word-mask
change has passed three focused equivalence/guard tests, but this A/B does not
demonstrate an overall speed improvement. Later positive-receipt races yield
24/25 canonical waves and must not be described as fully identical scheduling.

This native diagnostic does not establish the20s GUI requirement. In particular,
its global warm phase finds an AtMost25 witness before external dispatch whereas
the published512 Node/WASM run externalizes that query. Host warm/admission/slice
contracts must be checked before attributing the80.0-to18.1s difference solely to
native versus WASM arithmetic. A subsequent read-only audit finds a concrete
cross-width distinction: randomized candidate selection converts its u64 PRNG
value to `usize` before taking modulo. On WASM32 this drops high bits that the
native64 run keeps. Identical seeds and cardinality inputs therefore need not
follow identical heuristic choices. This is not yet a demonstrated sole cause
of the timing gap; see `minimum-cover-cross-width-rng-audit-2026-09-06.md` for
the bounded follow-up. There has been no new WASM publication for this candidate.

Artifacts:

- `_local/exact-cover-fixtures/1788691299759-native-repair-0.json`
- `_local/exact-cover-fixtures/1788691329672-native-repair-1.json`

## Qnia primary on the actual Clearra matrix

The comparison-only `_local/qnia-common-matrix-benchmark.mjs` imports the upstream
numeric/kernelization/HiGHS adapter and uses the actual Clearra matrix unchanged.
It does not run Qnia PC enumeration or secondary quality/canonical selection.

| Stage | Observed time |
| --- | ---: |
| Representation conversion | 48.4ms |
| Numeric matrix construction | 11.8ms |
| Kernelization | 202.0ms |
| Exact primary including lazy HiGHS setup | 34.304s |

The backend reports Optimal25. Its reduced kernel has1,456 constraints,
158 candidates,16,078 incidences and zero forced candidates; these differ from
the kernel on Qnia's own coverage matrix. No secondary-selection claim follows
from this run. Although the incidence matrix and lexical input order now agree,
the Clearra standalone run is native Rust and this reference solver is WASM
HiGHS, so the runtime boundary remains different.

Artifact: `_local/qnia-benchmark-results/1788691469472-common-matrix-primary.json`.

## Controlled cross-width RNG correction

The candidate-selection cast identified above was corrected at all eight
cooperative/blocking heuristic sites: the remainder is now taken in `u64`
before its bounded result is converted to `usize`. This preserves the old
native64 sequence and removes the former wasm32 high-bit truncation. A
diagnostic-only mode recreates the old32-bit choice without changing the host
runtime. The PRNG transition, number of draws, budgets, score/tie order and
positive-witness replay rules are unchanged.

The two cross-width/positive-authority tests, three word-scorer equivalence/guard
tests, and three cooperative/blocking witness tests passed (8/8). An independent
read-only review found no changed random-call order or missing candidate guard.
These checks are functional evidence, not GUI performance acceptance.

The release diagnostic was rebuilt once, executable SHA256
`70d1c284acee06f77fd6c5d1ba8cd917d375ad43e024c6e5eaad763e561fff3c`.
Two sequential CPU-isolated runs use the same actual `ad8af932…` matrix,
Clearra lexical candidate order,11 compute workers, factor4 and assist0.
**Word-mask scoring is enabled in both arms**; only the random-choice width
changes. Neither the known minimum nor a successful witness is provided.

| Same-binary native stage | Old wasm32 choice emulation | Fixed-width u64 choice |
| --- | ---: | ---: |
| Exact minimum cardinality | 20.234s | 6.775s |
| First canonical selection | 28.584s | 9.995s |
| Solver total | **48.818s** | **16.771s** |
| External proof waves | 2 | 1 |
| External canonical waves | 26 | 25 |

Both runs prove25 and return the identical full canonical25-key sequence.
The observed pair is 2.91 times faster (65.65% lower elapsed time), but it is
one sample per condition, not a statistical benchmark or a GUI timing.

The former wasm32 mode reproduces the important warm-path discrepancy:
AtMost26 succeeds after71 reported work units, but AtMost25 exhausts1,000 units
and requires an external34-task positive wave (12.848s in this native run).
The fixed-width mode instead uses352 work units for AtMost26 and **11 units for
AtMost25**, eliminating that external positive wave. Thus this same-runtime,
same-matrix experiment isolates the choice-width effect on the work performed;
the previous native/WASM timing gap cannot be explained solely by execution
speed. It does not prove that every remaining native/WASM difference is gone.

### Remaining measured hot work

Existing diagnostic counters were exposed through the preparation-to-shard
boundary, without changing proposal mathematics or admission policy. The host
retains the last live sample before terminal cursor ownership is released;
consequently terminal-advance work can be absent. These are **summed worker wall
time samples**, not additive end-to-end elapsed time. Softmax and gradient
columns are components of the inclusive Mirror-Prox iteration column.

| Fixed-width hard wave | Sampled MP iterations | Softmax component | Gradient component | Exact recertification |
| --- | ---: | ---: | ---: | ---: |
| AtMost24 cardinality proof | 55.261 worker-s | 26.111 worker-s | 23.618 worker-s | 1.787 worker-s |
| First external canonical selector | 50.224 worker-s | 24.016 worker-s | 21.018 worker-s | 1.634 worker-s |
| Second external canonical selector | 21.307 worker-s | 10.461 worker-s | 8.640 worker-s | 0.686 worker-s |

Softmax plus gradient evaluation dominates these samples. The data does not
support treating checked-u128 certificate arithmetic as the main bottleneck.
No new softmax/gradient optimization was added to this candidate; a verified
WASM build and actual GUI measurement come first.

The existing same-matrix Qnia HiGHS primary result (34.304s, Optimal25) remains
a WASM-reference versus native-Rust comparison. It must not be used to claim
that Clearra's GUI is faster than Qnia, nor may Qnia's Fast secondary contract
replace Clearra's exact canonical-selection requirement.

Artifacts:

- `_local/exact-cover-fixtures/1788692823761-native-repair-1-rng32.json`
- `_local/exact-cover-fixtures/1788692914403-native-repair-1-rng64.json`

At this checkpoint no new WASM/GUI timing exists for the width correction.
The native16.771s result therefore does **not** satisfy the GUI20-second gate.
