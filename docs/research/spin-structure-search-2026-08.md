# Independent Spin-Structure Search Record

Date: 2026-08-04

This record defines the contract, correctness gates, and measurement method for
the independent spin-structure search. It is not a replacement for Clearra's
existing forward maximum-damage or `spin-finder` search. The new implementation
belongs under `crates/clearra-spin-structure-search/`; application surfaces may
route to that crate without changing the existing forward engines.

## Preserved Forward-Search Baseline

The existing maximum-damage and forward spin-search algorithms are deliberately
out of scope. These SHA-256 values capture their worktree baseline before the
independent crate is connected:

| File | SHA-256 |
| --- | --- |
| `crates/clearra-forward-search/src/search.rs` | `484F0551B70934BDF7B44C3E2279422B76A85910873C8B0E0ABB45D74A1F818C` |
| `crates/clearra-forward-search/src/parallel.rs` | `B1D44AF42160B4C642AD7E577183017A72749F5EBA8C8C092B258643E8CBDD65` |
| `crates/clearra-forward-search/src/t_spin_acceleration.rs` | `A52BA7054D102AF490B3D6DD412B4FEF0757DA696E6640B757CECEA24EB257EE` |

A change to one of these files is not part of this feature. If an experiment
changes a baseline file without an independently required fix, revert it and
verify the file hash before continuing. Shared rule, kick, board, and spin
classification APIs may be called through explicit public contracts; their use
does not authorize changing the preserved search behavior.

## Search Contracts

The structure search consumes an unordered piece inventory. Repeated letters
are multiplicities, not a fixed queue, and there is no hold transition. A result
means that some exact legal build order from that inventory constructs the
reported structure and that the terminal lock satisfies the selected spin and
line requirement. This contract is intentionally different from forward search,
which retains queue order, hold state, and path identity.

The minimal T-structure engine has its own route. Its default minimality is
subset-minimal, not globally minimum piece count: removing any non-target
placement from a returned structure must make that smaller structure fail exact
buildability or the requested terminal spin/line contract. Valid structures at
different placed-piece depths therefore coexist. The search must exhaust the
configured inventory domain across every worker; the first worker result or the
first accepting depth is not a completeness or minimality proof.

The All-family routes are also independent dispatch targets, even when they
share immutable operation catalogs, exact build validation, or worker
infrastructure:

| Route | T-piece classification | Non-T classification |
| --- | --- | --- |
| T structures | three-corner T results retain their exact Regular/Mini classification; the Plus profile also admits the exact immobile fallback as Mini | rejected |
| All-Mini | three-corner T results retain their exact Regular/Mini classification | exact last-rotation and immobile results are Mini |
| All-Mini+ | three-corner T results retain their exact Regular/Mini classification; the exact immobile fallback is Mini | exact last-rotation and immobile results are Mini |
| All-Spin | three-corner T results retain their exact Regular/Mini classification | exact last-rotation and immobile results are Regular |
| All-Spin+ | three-corner T results retain their exact Regular/Mini classification; the exact immobile fallback is Mini | exact last-rotation and immobile results are Regular |

Regular and Mini are separate result partitions and separate identity fields.
They must have separate counts and digests, and materialization must not merge
them merely because they produce the same final board. Product output and help
must describe the behavior directly by profile and class; implementation-source
labels are not part of the command vocabulary.

## Structural Pipeline

The T-only route adapts the upstream structural pipeline to Rust while retaining
Clearra's board, line-clear, kick, reachability, spin-profile, and canonical
identity contracts. Its stages are:

1. build an immutable operation catalog with exact used-row and required-deleted-row keys;
2. fill the requested logical row interval;
3. add exact supporting operations for otherwise airborne placements;
4. enumerate the target corner walls;
5. add a bounded roof only until the target rotation is exactly reachable;
6. validate at least one complete legal build order and the terminal scoring edge;
7. remove only exactly redundant supersets and materialize canonical Regular and Mini partitions.

The four All-family routes reuse safe catalog, row-key, exact-build, and
canonical-set machinery. T-specific supply and cavity assumptions do not apply
to their non-T targets. Each target piece and rotation is validated under the
selected profile after exact placement and line clearing.

## Exact Pruning Boundary

Every candidate deletion needs an exhaustive or local geometric proof. A hash
may choose a bucket, but full state equality must confirm deduplication. State
identity includes at least the board, remaining inventory, logical/deleted-row
mapping, target operation and retained rotation evidence, line requirement,
spin profile, kick profile, and every option that changes reachability.

The T-only route may use these exact rejections:

- no T remains for the terminal target;
- the terminal operation has no retained rotation evidence;
- a non-Plus T target does not have three blocked corners;
- a Plus T target has neither three blocked corners nor exact immobility;
- exact placement and line clearing do not satisfy the requested line condition;
- a placement collides, leaves the configured field bounds, or violates an exact required-row key;
- a full-key state or canonical solution is exactly equal to one already retained;
- a candidate is a strict redundant operation superset only after exact
  buildability and terminal validation proves that the smaller canonical set
  still satisfies the complete requested contract.

The All-family routes may use the same bounds, row-key, full-identity,
buildability, and scoring proofs. They must not use absence of a T, a T-shaped
cavity, or a three-corner requirement to reject a non-T target. Non-T acceptance
requires exact last-rotation evidence and exact immobility. T acceptance remains
profile-specific as listed above.

The following shortcuts are forbidden candidate-deletion reasons:

- approximate immobility or a single failed movement probe;
- treating cells above the configured active height as occupied blockers;
- discarding rotation evidence before terminal classification;
- occupancy-monotonicity assumptions across line clears;
- one failed representative build order;
- stopping after the first accepting depth or when any one worker succeeds;
- hash-only equality, Bloom-filter membership, timeouts, memory caps, or any heuristic shape score.

Resource exhaustion produces an incomplete result or an explicit error; it does
not produce a complete empty result. A slow exhaustive oracle must compare
canonical solution sets on small fixtures. Required regression families include
three-corner Regular and Mini fallback cases, every non-T piece, the active-height
roof boundary, fifth-kick rotation evidence, exact line requirements, line-clear
y movement, and one-worker/multiworker equality.

## Fixed Stage and Layer Timing

The benchmark surface is fixed in
`crates/clearra-app/examples/spin_structure_native_benchmark.rs`. The timer
surrounds one typed application request; query construction, digesting,
determinism checks, and printing are outside that interval. The search report
records three coarse work intervals—fill, structural expansion, and
finalization—and one expansion interval per placement depth. A depth clock is
sampled once at its boundary rather than once per candidate.

Substage behavior is recorded with integer work counters rather than hot-loop
clocks: fill checks, support/roof candidates, corner/blocker candidates, entry
states, strict verification checks, exact state deduplications, and exact
outcome deduplications. Each depth separately records input states, piece
choices, reachable locks, generated states, exact duplicates, terminal
candidates, and accepted Regular/Mini counts. This keeps the measurement
surface useful without turning profiling into a placement-loop bottleneck.

`fill_ns`, `expansion_ns`, `finalization_ns`, and `layer_ns` are sums of target
partition elapsed work. They are not process CPU counters and may exceed wall
time under parallel execution. Catalog compilation, dispatch/channel work,
deterministic merge, and application response construction remain visible only
in request wall time. Layer time is nested inside expansion time and is never
added to the reported measured-work total a second time.

Native workers send an explicit ready event before their first task. Every
later task is assigned to the worker whose preceding task just completed, so a
faster worker immediately receives the next target partition. Results are
stored by task identifier and merged in deterministic target order. Parallel
parity comparisons exclude only `workers_used` and elapsed timings; outcomes,
Regular/Mini partitioning, minimum depth, completion state, stage counters, and
layer counters must match the one-worker run exactly.

The run protocol is unchanged:

- searches at or below one second run five times;
- searches above one second run twice;
- the suite contains at least one search above one second;
- peak RSS is measured externally (for example, `/usr/bin/time -f %M`) rather than by polling the search loop;
- speed is the primary acceptance metric;
- a result up to 10% slower may be retained only when peak memory is at most half and semantic identity is unchanged;
- a slower change without a compensating accepted improvement is reverted immediately, then every touched baseline hash is verified before the next experiment.

Previously rejected forward-search experiments are not silently repeated here.
In particular, the 64-entry worker-local reachability cache regressed its
fixture, and raising the layered batch cap from 32 to 128 increased peak RSS
without a useful speed gain. A structurally different experiment needs its own
hypothesis, isolated measurement, and rollback hash.

## Exact Compatibility Reference

The following is a compatibility reference only, not a product acceptance count
and not a comparison with the 12,242-path forward-search fixture. It applies
only to this complete profile:

- source field `0x280f8ffff8f` (the benchmark Fumen field);
- unordered inventory `IOTSZ`;
- at least one cleared line;
- fill interval `[0, 5)`, catalog/search height and margin 7;
- stock SRS, no 180-degree rotation;
- strict build filtering and the roof stage enabled.

The audited reference run returned 340 structures: 214 Mini and 126 Regular.
By placed-piece count the distribution was 2, 117, 109, and 112 structures at
depths 2, 3, 4, and 5. By cleared-line count it was 326 one-line and 14 two-line
structures, with 23 distinct terminal T placements. These numbers may be used
only after every listed semantic option matches. Other profiles, kick tables,
line predicates, inventory semantics, roof policies, or identity projections
must not be expected to return 340.

### Exact result-set comparison

The Rust product route returns 360 structures for that profile: 132 Regular and
228 Mini. Comparing canonical logical operations, terminal operation, terminal
cleared-row signature, and class gives:

- all 340 reference structures are present;
- no reference structure is missing and no shared structure changes class;
- the Rust route has 20 additional exactly validated structures (6 Regular and
  14 Mini);
- two additions use three placements and eighteen use five; all clear one line.

Every additional result passes exact non-target build order, grounding, terminal
shape, last-rotation entry, scoring classification, and one-operation-removal
necessity. The difference comes from row-dependency expressiveness: the
reference fill recursion advances from its current row upward and therefore
cannot represent a dependency that first deletes an upper row and then
completes a lower terminal row. The Rust logical-row model preserves that
dependency explicitly. Pruning these 20 results merely to reproduce the
reference count would therefore be incorrect and is rejected.

## Provenance

The structural stages were audited against
[`knewjade/solution-finder`](https://github.com/knewjade/solution-finder) version
1.43 at commit
[`e8b291b47702cd08daf982bd52ef946902354848`](https://github.com/knewjade/solution-finder/commit/e8b291b47702cd08daf982bd52ef946902354848).
The adapted implementation must retain the upstream MIT copyright and permission
notice; see [`THIRD_PARTY_NOTICES.md`](../../THIRD_PARTY_NOTICES.md). This
provenance is a legal and reproducibility record, not a user-facing engine name.

## Implementation and Benchmark Status

The independent crate, typed application route, CLI/web-command boundary, and
Discord `/spin-structure` surface are implemented. Existing `damage` and
`spin-finder` dispatch remain separate. The crate release suite contains 41
tests, including target-first versus exhaustive exact-oracle comparisons for all
six modes, terminal cleared-row identity regressions, cache-key separation, and
one-worker/task-merge parity. The exhaustive oracle is compiled only for tests;
it is not retained in the product path.

Discord slash input, Modal input, and the `$`/`>` text shorthand all lower to
the same typed `spin-structure` request. CTK3 output keeps the Regular and Mini
partitions parallel to the solution keys and writes the exact class on each
result page. The dedicated CLI help describes all six profiles without changing
the existing forward-search help or dispatch.

The desktop workspace is deliberately not exposed in this change. Its current
job-result envelope serializes core and forward-search reports but not structural
outcomes; adding only a route would therefore report success while discarding
the Regular/Mini placements. A future desktop surface needs a dedicated bounded
preview contract and workspace instead of extending the ordered-queue
`ForwardSearchWorkspace`. Browser exposure additionally needs target-task
cooperative advancement so progress and cancellation do not depend on forcibly
terminating the Web Worker.

The fixed reference fixture was measured on 2026-08-04 under WSL with eight
visible logical processors. Every search at or below one second used five
measured requests. Values below are typed-application wall times in
milliseconds; no warm-up request was removed.

| Mode | Results (Regular/Mini) | Runs | Min | Median | Mean | Max | FNV-1a-64 semantic digest |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| T-Spins | 360 (132/228) | 5 | 101.626 | 115.907 | 125.571 | 150.141 | `5ebdbf8be8a3647a` |
| T-Spins+ | 535 (132/403) | 5 | 305.290 | 323.331 | 372.768 | 588.735 | `99da46ff6974a1bf` |
| All-Mini | 1,437 (132/1,305) | 5 | 498.899 | 530.564 | 532.893 | 561.383 | `bc881d57feefb0a6` |
| All-Mini+ | 1,612 (132/1,480) | 5 | 578.996 | 616.079 | 628.297 | 670.069 | `c8cd87490192409e` |
| All-Spin | 1,437 (1,209/228) | 5 | 485.957 | 537.823 | 529.125 | 558.975 | `8bb5a41faa8b7b67` |
| All-Spin+ | 1,612 (1,209/403) | 5 | 575.864 | 650.326 | 636.676 | 695.873 | `dbe5777345a4a3a2` |

The T-only fixture also ran five times with one worker: 356.772–609.141 ms,
median 557.745 ms, with the same digest and all fixed counters. The eight-worker
median is 4.81 times faster. The required over-one-second case was All-Spin+
with one worker: 1,944.466 and 2,101.008 ms, median 2,022.737 ms, again with the
same digest and counters. Its eight-worker median is 3.11 times faster.

The compatibility program's two recorded internal-search runs were 239 and
273 ms and returned 340 results. Those times exclude its JVM process startup;
the Rust values above include the typed application request but return the
20-result exact superset described earlier, so this is a profile reference and
not a claim of identical work. Even with that broader output, the eight-worker
Rust median is 2.21 times faster than the reference's 256 ms two-run mean; the
one-worker Rust median is slower, which is why the product keeps its native
multithreaded route.

External `/usr/bin/time -v` samples reported 28,160 KiB peak RSS for T-Spins
with eight workers. All-Spin+ reported 22,248 KiB with one worker and 96,688 KiB
with eight workers. The parallel route is intentionally retained because the
latency reduction is substantial; no experiment in this series met the policy
of halving peak memory for at most a ten-percent speed loss.

The accepted acceleration set is exact target-first fill, an immutable shared
operation catalog with row-key indexes, target-shape and entry preflight,
depth-ordered expansion queues, a bounded deterministic entry-result cache, and
full-key deduplication. A forced 360-to-340 count match was rejected because it
would delete the 20 valid row-dependency results. No T-only cavity or supply
rejection is used for a non-T target.

Reproduction commands:

```sh
CARGO_TARGET_DIR=/tmp/clearra-spin-structure-target cargo test \
  -p clearra-spin-structure-search --release
CARGO_TARGET_DIR=/tmp/clearra-spin-structure-target cargo build \
  -p clearra-app --release --example spin_structure_native_benchmark
/tmp/clearra-spin-structure-target/release/examples/spin_structure_native_benchmark \
  --fixture reference-t --workers 8 --repetitions 5
/tmp/clearra-spin-structure-target/release/examples/spin_structure_native_benchmark \
  --fixture reference-all-spin-plus --workers 1 --repetitions 2
```

The two compatibility logs and canonical CSV outputs are retained outside the
repository under
`%LOCALAPPDATA%\Clearra\benchmarks\spin-structure-reference-20260804`.

## Completed Input Row Correction

The benchmark table above is a historical pre-correction record. It must not be
used as the current semantic baseline when the input contains an already
completed row. The product path previously compiled target operations from the
raw snapshot even though the exhaustive oracle tracked that row as deleted.

The public search boundary now validates the raw snapshot, clears every
completed input row exactly once, compacts the remaining rows, and only then
compiles serial or partitioned work. Reports and CTK solution keys use that same
post-clear board. Discord input previews deliberately keep the original page,
so rendering a completed row does not simulate a search-side line clear.

After the correction, two eight-worker runs of `reference-t` returned the same
391 outcomes (136 Regular and 255 Mini), minimum placement count two, and
semantic digest `bb86ee1816a1a68c`. Wall times were 95.440 and 96.962 ms. The
serial/partitioned equivalence test, exhaustive oracle comparisons, and a CTK
solution-key regression all cover this boundary. Reapplying line clearing in a
renderer, task worker, or result decoder would be a duplicate and is forbidden.
The Discord default-height-eight form of the same request has a current-source
baseline of 393 outcomes (136 Regular and 257 Mini), replacing the historical
380-result pre-correction deployment expectation below.

## Discord Deployment Record

The current-source job image was built by Cloud Build operation
`d765fc52-afec-410b-a59c-16d1ca0857e1` with tag
`spin-structure-20260804-193436` and digest
`sha256:8732c60cdc9db7644fcfdb1822fb53d9f435187ccdab27560b330d8387c4e872`.
Cloud Run revision `clearra-current-job-00012-pev` receives 100 percent of
untagged service traffic in `asia-northeast1`. Its verified runtime contract is
one request per instance, an eight-vCPU and 16-GiB limit, a 300-second timeout,
minimum scale zero, maximum scale four, CPU throttling disabled, startup CPU
boost enabled, and an eight-worker health result. The public health route
returned HTTP 200, an unauthenticated job submission returned HTTP 401, and no
revision error entry was present after deployment.

The initial Discord gateway release was
`/opt/clearra/releases/discord-20260804-193436-spin-structure`. Live validation
found that a generated `spin-structure-result.ctk3` was being admitted again as
a standalone renderer input. The 380-page result correctly exceeded the
128-frame preview boundary, but the downstream path then produced a misleading
second failure reply after the successful search. Generated `*-result.ctk3`
attachments are now rejected only when authored by the bot itself, before any
download or decode. User-authored CTK3 files with the same filename and the
legacy generic deferred-result route remain accepted. The corrected active
release is
`/opt/clearra/releases/discord-20260804-201148-result-ingress`; the initial
release remains available for rollback. The service was active with restart
count zero and no warning-or-higher journal entry after the second cutover.

Global application-command synchronization succeeded in Cloud Build operation
`e829dfd6-4485-4a9d-b031-c52d61ab11ff` and verified all 29 commands, including
`spin-structure`. The command-sync build installs the declared workspace
dependencies and builds CTK3 before the final secret-bearing registration step;
this corrects the first isolated sync attempt, which could not resolve the CTK3
package.

Local ports 4194 and 8790 remain listening. Port 8790 is a persistent local
tunnel to the gateway administration surface and returned HTTP 200. The live
Discord validation fixture uses the same field and inventory as the fixed
benchmark, but the Discord contract currently applies its default height of
eight rather than the benchmark's explicit height of seven. Its expected exact
result is therefore 380 structures: 136 Regular and 244 Mini, with minimum
placement count two, eight workers, and a 380-page CTK3 attachment. Two real
Chrome/Discord invocations returned exactly those counters. The corrected
invocation produced no second renderer reply during the post-result observation
window. Its 12,960-byte attachment downloaded successfully and decoded as a
ten-column, 380-page CTK3 document whose first 136 pages are marked
`Spin: Regular` and remaining 244 pages are marked `Spin: Mini`. The full
Discord suite passed all 342 tests before the corrected cutover. Cloud Run had
no severity-ERROR entry, and the local 4194 and 8790 listeners remained
available after final validation.

### Completed-row input-contract cutover

The current completed-row correction supersedes the 380-page deployment
expectation immediately above. Cloud Build operation
`fead8d47-cb2d-4efb-bcfc-081ec1c26c2b` produced image tag
`input-row-20260804-204752` with digest
`sha256:fba226e54d485659bd85229d22872a05c20ff0fc579c63c684e9c5c82a977486`.
Cloud Run revision `clearra-current-job-00009-qvp` receives 100 percent of
untagged traffic. The prior `clearra-current-job-00012-pev` revision remains
tagged `spinstruct-0804` for rollback with no untagged traffic. The verified
resource contract remains eight vCPU, 16 GiB, concurrency one, a 300-second
timeout, minimum scale zero, maximum scale four, CPU throttling disabled, and
startup CPU boost enabled.

The matching gateway release is
`/opt/clearra/releases/discord-20260804-204752-input-contract`; its service was
active with restart count zero after the cutover. Global command synchronization
completed in Cloud Build operation `01d5ff0a-1499-473a-b626-00cd617cef18`.

A real Discord Modal invocation used a five-row field whose second row from the
bottom was completely filled. The one-page input preview decoded to the original
rows and retained `GGGGGGGGGG`, as required for rendering. The completed search
returned 408 pages (151 Regular and 257 Mini) with minimum placement count two;
this is a separate five-row boundary fixture and is not the height-eight
reference fixture above. Its downloaded CTK3 first page began with two
`GGGG___GGG` rows: the completed row was absent and the row above it had compacted
before placements were exported. The public response contained neither a worker
count nor infrastructure or engine details. The command help rendered every
command and example as an inline code node, so Discord did not reinterpret the
command grammar as message formatting.

The private administration migration now rewrites generic historical command
labels such as `Text command` to an explicit `unresolved.<source>` identity and
persists the normalized store. Live parser-resolved requests retain their exact
catalog command; later Discord metadata may replace an unresolved identity with
that exact command. The UI displays `Unresolved (<source>)` instead of a blank or
a generic command name. Local ports 4194 and 8790 remained listening after the
browser and deployment validation.

### Cross-command completed-row and command-identity cutover

The completed-input-row boundary now also covers scenario PC, score-finder, and
the two forward search modes. PC compilation clears and compacts completed input
rows before target-boundary validation and piece-window derivation while keeping
the requested target height unchanged. Forward search performs the same single
normalization at its shared configuration boundary, before either serial or
partitioned work begins. Build/setup pruning is unchanged. Cover remains a
separate contract: completed base rows are rejected, target rows are not
pre-cleared, and result compaction still occurs only when the construction is
complete.

The regression set covers raw-versus-normalized PC compilation, PC and
score-finder compatibility lowering, serial and partitioned damage/spin search,
spin-structure search, native PC and spin-structure CTK solution keys, raw input
preview retention, and the cover-target exception. Eight isolated Rust boundary
tests, all 349 Discord tests, all 21 UI tests, the in-memory Svelte/TypeScript
compile check, `cargo fmt --check`, and `git diff --check` passed. CTK Drawer
navigation now exposes at most 100 pages on either side of the current page,
supports Left/Right frame navigation, and preserves the clicked preview's
viewport position across selection and rendering.

Cloud Build operation `4ec6f22f-f2b5-4bb6-b03a-73685159e8f9` produced image tag
`completed-row-all-20260804-124807` with digest
`sha256:1e1d189408266c795129ed3fdd2d5c9f47aa7fb46497ac596bddb34a2fe6fec9`.
Cloud Run revision `clearra-current-job-00010-lcs` receives 100 percent of
untagged traffic. The first image-only deploy created the revision while the
service's explicit revision traffic map still pointed at the previous revision;
the cutover therefore required an explicit traffic update. Future deploys must
verify both `latestReadyRevisionName` and the untagged traffic target rather than
trusting the deploy command's summary. The retained resource contract is eight
vCPU, 16 GiB, concurrency one, a 300-second timeout, minimum scale zero, maximum
scale four, CPU throttling disabled, and startup CPU boost enabled. The health
route returned HTTP 200 with an eight-worker limit, unauthenticated job creation
returned HTTP 401, and the new revision had no error-severity log entry after the
live test.

A real Discord `/path` Modal request used target height two, queue `OII`, and a
three-row input whose top row was complete. The preview CTK remained three rows
high and preserved the complete row. The one-page result CTK was two rows high:
its initial cells were the eight occupied bottom cells followed by the colored
placements, with no retained third completed row. The response reported one
solution and 100 percent coverage and contained no worker count or infrastructure
terminology.

The matching private gateway release is
`/opt/clearra/releases/discord-20260804-124919-command-identity`. Its request
store, snapshot API, and table now always expose either a canonical command or an
explicit `unresolved.slash`, `unresolved.text`, `unresolved.render`, or
`unresolved.observed` identity. Empty telemetry commands and legacy generic
labels are migrated at the storage boundary, unresolved records do not inflate
repeat counts, and verified metadata can later promote them to an exact command.
The service was active with restart count zero after cutover; the administration
tunnel returned HTTP 200. Ports 4194 and 8790 remained listening. No application
command synchronization was required because the registered command schema did
not change.
