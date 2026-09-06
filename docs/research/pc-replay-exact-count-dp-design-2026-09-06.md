# PC replay exact-count DP candidate — 2026-09-06

Status: **read-only design candidate; not implemented, benchmarked, or release Go**.
The active Rust/WASM source freeze is unchanged. This document does not replace
the [product performance audit](parallel-product-audit-2026-09-06.md) or claim that
the minimum <=20s / full Build ~0.4s GUI criteria have been met. The earlier
Build ~0.1s target was superseded by the user's later clarification.

## Problem and preserved meaning

`PcReplaySourceBuildSession::advance` currently visits every geometry-pattern
cell. `materialize_geometry_pattern` materializes each graph location, unions its
executions, and deduplicates `(pattern_id, trace_identity)`. The manifest retains
counts and a first page, but constructing it still visits and replays every raw
terminal path. Bounded live memory therefore does not imply bounded startup time.

Preserve:

- Exact distinct canonical replay count within each `(geometry, pattern ID)`.
  The same visible replay under different pattern IDs remains a separate member.
- Geometry ordering, pattern ordering, then **canonical string lexical ordering**;
  numeric tuple ordering is not equivalent (for example, decimal `10` and `2`).
- Existing `trk1` keys, current visible cursor/hold projection, all four hold
  decisions, standard-bag lookahead, terminal acceptance, and PC chain validation.
- Page size 100 for rendering; explicit export still means the selected geometry's
  entire member family, with cancellation and source ownership checks.
- Exact failure semantics: malformed/incomplete evidence, overflow, cancellation,
  or exhausted admitted memory must not produce a falsely complete count.

## Source facts and proof obligations

| Boundary | Current fact | Consequence |
| --- | --- | --- |
| `clearra-core-executor/.../wasm_cpu/buildup.rs`, `BuildOrderGraph::build` | A node is an operation subset; subset fixes deleted logical rows and physical board. Every edge adds one operation. | Standard producer is a finite, depth-increasing DAG. |
| `.../wasm_cpu/catalog.rs`, `instantiate_realization_raw` | Physical placement is lifted through the remaining logical rows and accepted only when `projected == target_cells`. | At one node, equal piece/rotation/x/y determines the same target operation. |
| `clearra-core-domain/.../normalized_tiling_solution.rs`, `from_placements` | Empty/overlapping operation masks are rejected. | Distinct candidate operations cannot share that lifted placement. |
| `buildup.rs`, scoring graph construction | Exact edges are deduplicated; subset consistency is checked while converting to scoring nodes. | Standard single-graph transitions are visible-label deterministic, subject to the supply label rules below. |
| `clearra-replay/src/scoring_execution.rs`, `ExactScoringExecutionGraph::new` | Public constructor does not certify the producer invariants. | Generic or serialized graphs cannot inherit the fast-path proof without validation. |
| `clearra-app/src/pc_replay_page_source.rs`, `materialize_geometry_pattern` | One geometry can union multiple `GraphLocation`s, then deduplicate again. | Counts from graph locations must not simply be added; their visible languages can overlap. |
| `clearra-postprocess/.../exact_scoring_execution_materializer.rs`, `visit_complete_replay_paths` | Checks forward child indices, supply acceptance, replay construction, and clear counts; final PC projection checks empty board. | DP must preserve these validity obligations, not silently discard invalid evidence as a zero-count branch. |

The graph paths are not the product identities. `edge.to`, operation index,
movement/kick evidence, and other hidden metadata are absent from `trk1`. Deduping
equal visible labels by arbitrarily choosing one destination is unsound unless
the destinations are equal or their suffix languages are proven equal.

### Canonical label versus supply state

`execution_supply.rs` carries the actual `(node, cursor, hold)` used to accept a
fixed queue. In the current replay materializer, `replay_path` instead builds the
visible `PieceDecision` through `SolutionTraceBuilder`: its cursor fields are
`step_index` / `step_index + 1`, and input/output hold fields are `None` / `None`.
The real `HoldDecision` enum is still included in the canonical key.

The counter must therefore retain actual supply state internally while generating
labels with the **existing replay projection**, not by serializing the internal
supply cursor/hold. Changing that pre-existing projection is a separate semantic
change and is explicitly outside this candidate. `ReleaseHeldAtTerminal` is a
placement transition, not an epsilon transition.

## Proposed counter architecture

Keep language counting in postprocess, canonical step projection in replay, and
page/source ownership in App. CLI, GUI, and Discord continue consuming the same
App capability; do not implement an independent GUI solver.

1. Validate graph spans/root/forward edges and the required board/depth invariants.
   Reuse supply transition helpers and canonical step projection rather than
   maintaining another interpretation of hold or formatting.
2. For a validated deterministic single graph, memoize suffix count by actual
   supply state, plus depth/board when they are not certified node properties.
   Count each distinct label once even if generic input repeats an identical
   label/destination edge. Deterministic destination alone does not justify a sum
   over raw edges.
3. Otherwise determinize the union of all locations for the geometry-pattern
   cell. A constituent is `(location, node, actual cursor, actual hold)`; location
   retains that batch's sequence and policy authority. Group transitions by exact
   visible label and union their successor constituents before counting.
4. A subset can accept the empty suffix once if any constituent accepts. Accepting
   nodes have no further transitions in this language, matching the old visitor.
   The root's empty replay remains invalid rather than becoming a public witness.
5. Store a checked suffix count for each canonicalized subset. For every distinct
   label, add the count of its one union-successor, not the counts of its members:

   ```text
   count(S) = indicator(any accepting constituent in S)
              + sum over distinct labels a: count(union(successors(S, a)))
   ```

This counts distinct words: languages under different first labels are disjoint,
and subset union removes nondeterministic duplicate paths under the same label.
Memoization shares suffix calculations, not prefix multiplicity. A count from
two different pattern IDs is still added separately.

Equal visible prefixes from a fixed initial board determine the same physical
board. The optimized key may omit it only after proving node-board consistency;
the generic fallback must include it or reject inconsistent evidence. The same
qualification applies to depth. Do not assume every arbitrary graph node has the
standard producer's unique depth or board.

### Lazy selection and bounds

Use lexical outgoing labels and suffix counts for rank/select: skip whole suffix
families until the requested offset and materialize only the requested members.
Construct and validate actual `ReplayTrace`s for the first page and later pages
through the existing replay/PC projection boundary. Keep a deterministic valid
hidden witness path for a selected visible word; never mix edges from unrelated
constituents during reconstruction.

Count-only DP followed by whole-cell materialization on every page would leave
large-cell latency unresolved. Rank/select is therefore part of the candidate,
not an optional correctness substitute for lazy paging.

Subset construction can be exponential in the worst case. No O(1), polynomial,
or target-time guarantee is claimed. Use checked `u128` arithmetic, actual
capacity admission for transitions/subsets/memo/reconstruction scratch, bounded
cooperative work units, and cancellation. If the memory budget cannot admit a
new state, fail explicitly or use a separately admitted exact fallback; do not
truncate and mark complete. Existing raw-terminal-count caps versus new distinct
count/state caps need an explicit policy decision, not an accidental redefinition.

Whole-live admission must include the still-retained original Core result,
immutable Arc source, count manifest, overlapping old/new memo and subset storage,
rank/select scratch, and partial public-page/carrier reservations. Keep the current
100-member public projection reserve (including its existing 16x carrier policy)
unless that policy is separately changed and verified. A 64MiB cap applies to
these owners together, not only to the DP cache. Counts are proposed as `u128`,
but current `PatternManifest.witness_count`, `end_offset`, and page arithmetic use
`usize`: require checked conversions with explicit rejection, or migrate those
internal fields end-to-end. Never cast or saturate an oversized exact count.

## Versioned source digest instead of trace-stream digest

Current source `v1` SHA hashes each ordered candidate ID, pattern ID, key length,
and full canonical key. A suffix count cannot reproduce that digest without
processing the trace-string stream. An intermediate key-only streaming approach
could reduce replay allocations but would not eliminate full trace traversal.

A proposed `clearra.pc-replay-source.v2` digest can bind:

- Canonical query/projection context: board, layout, cursor/hold, hold/lookahead
  policies, and kick/rule identities; do not rely only on an opaque problem ID.
- Ordered pattern IDs/sequences and geometry grouping/order.
- The immutable source's roots, nodes, edges, and all retained edge evidence,
  including hidden metadata that can affect the reconstructed witness.
- Counting, canonical projection, and page-ordering versions, with final exact
  count metadata if it is part of the page source contract.

Use explicit tags, lengths, and integer byte encodings, never memory layout,
pointer addresses, Vec capacity, or a hash of a noncanonical debug representation.
The digest and all pages must remain bound to the same immutable App-owned source.

This is a **source identity**, not a proof that graph-isomorphic representations
or different worker partitions have the same semantic set hash. Stable identity
across those representations requires a separate normalization requirement.

`pcReplayPager.ts` already compares the source SHA, global counts, current
generation, requested ordinals, and geometry metadata. Preserve those checks.
Its exact `pc-replay-member-page.v1` check means a digest-contract migration must
update the page version and both producer/consumer validators together. Do not
reuse the old version while silently changing what its digest commits to.

## Required focused acceptance tests

| Fixture or fault | Expected result |
| --- | --- |
| Small valid deterministic graph | DP count and rank-selected keys equal old exhaustive sort/dedup; use the old path as test oracle. |
| Same visible label, different hidden nodes, overlapping suffixes | Count the language union once; edge-to/operation IDs do not inflate count. |
| Repeated identical label/destination edge | Generic deterministic fast path counts the label once, not every raw duplicate edge. |
| Same label, disjoint suffixes | Retain both suffix families; choosing one edge must fail the oracle comparison. |
| Two prefixes merging into one suffix state | Preserve both distinct complete words while sharing suffix memo work. |
| Multiple graph locations for one geometry | Remove cross-location duplicates; do not add location counts blindly. |
| Identical trace under different pattern IDs | Keep both members and their separate pattern counts. |
| Current/swap/store/terminal release; bag lookahead | Exact supply acceptance and unchanged canonical projection match exhaustive output. |
| Equal board but different hidden operation subsets | Preserve future language until deterministic equality or subset union resolves it. |
| Invalid root/span/edge/cycle/board/clear count; empty replay | Explicit incomplete/error, never successful zero-count omission. |
| Count overflow, requested/actual capacity limit, cancellation | Fail closed with all source ownership intact; no falsely complete manifest. |
| Lexical boundaries and pages 1/2/final; explicit export | Same ordered keys, no duplicates/gaps, full selected-geometry export. |
| Source/query/pattern/policy/version changed; stale response | New source identity or rejection; old pages cannot enter the active family. |
| CTK3 `ctk3_w0kCQBjwwAMPPAD37g`, P7 | Exact counts and first/last/sample pages agree with admitted oracle; separately measure manifest time, page time, memory, and cancellation. |

## Handoff: completed TS observation plumbing, not performance evidence

The following checks were completed before this documentation-only handoff:

- Parent reported `LocalSearchProfile.contract.ts` PASS for the local-only helper
  and route. It permits only `local-recovery` / `local-audit`, filters numeric
  known fields, caps waves at 128, and displays only the latest terminal profile.
- Worker owner ran `ClearraVerifierPool.contract.ts` and
  `DistributedWasmJobRunner.contract.ts`: PASS, including opt-in transport profile
  without WASM stage-profiling, default-off behavior, worker counts 1/3/11, bounded
  history, lease-first dispatch, selective cancellation, and late-local retry drain.
- `tsc --noEmit -p apps/clearra-web/tsconfig.contract.json`: PASS.
- Actual web `src` TypeScript check using the project's config/options and all
  19 source entry files plus transitive imports: **19 files, 0 diagnostics**.
  This includes `clearraWorker.ts`, which contract-only checking did not cover.
  The stock all-file web config also includes Node test/tooling files without
  their required Node ambient types; no claim is made that that broader invocation
  passed unchanged.
- `clearraWorker` passes the local-mode option to `ClearraProductJobRunner.run`;
  the distributed runner publishes `search_profile` on the existing final event.
  Ordinary Pages remains off. No journal durability or scheduling authority was
  weakened to collect timings.

Transport spans overlap; summing them is not elapsed time. In particular,
`run_grant_to_reply` includes event queues and reply transport as well as worker
work. New GUI measurements must distinguish initialization, durable offer/start
ACK waits, actual shard work, and final drain before selecting another optimization.
The existing source changes are not evidence that the live WASM includes them or
that the visible worker tail is resolved. No Rust/TS code was changed for this note.

## Independent review obligations

This section is a source-only adversarial review, not implementation or performance
evidence. It found no immediate counterexample to the standard producer's
operation-subset proof, but that proof must become an explicit checked admission
condition at the DP boundary. A public `ExactScoringExecutionGraph::new` value does
not carry the private producer's certificate.

### Count and replay equivalence

- A duplicate visible label with the **same child** still contributes only once.
  Summing raw edges overcounts even this simple case. Add a duplicate-edge oracle
  fixture in addition to the different-hidden-child union fixtures above. The
  current `BuildEdge::canonical_key` includes destination and operation index;
  it is not itself the public canonical-label equality relation.
- Before publishing an exact count, validate placement bounds, collision,
  line compaction, recorded clear counts, and terminal empty-PC conditions for
  **every reachable DP transition and accepting state**. Graph spans and forward
  indices alone are insufficient. Materializing and validating only the first
  100 or rank-selected witnesses could leave invalid unselected paths counted
  as complete. Reuse the semantics of `replay_path`, `SolutionTraceBuilder`, and
  the App PC chain validator; retain actual supply state separately from visible
  synthetic cursor/hold fields.
- The fast path must either certify node board/depth consistency and unique
  visible-label successor language, or decline to the checked union path.
  Equal destination IDs do not justify retaining duplicate label multiplicity;
  equal visible labels with different destinations do not justify selecting an
  arbitrary child. Reconstruct one coherent hidden path for the selected word.

### Whole-live memory and owner lifetimes

The original **64 MiB** App product budget remains the authority. A successful
DP-local allocation check is not a replacement for that whole-live budget.
The implementation must provide checked requested-capacity admission before
allocation and actual-capacity checks afterward, including reallocations and
state replacement overlaps. Existing immutable cached capacity sums may be
reused only while their owner is unchanged.

| Owner | Live interval / replacement boundary | Required accounting and release |
| --- | --- | --- |
| Original Core result and its DAG evidence | Source construction and cooperative finalization | Keep the original Core projection/reservation; do not count only the cloned page source. Release only when its actual owner is dropped. |
| Immutable App page source / Arc | Manifest preparation, page requests, and export owner lifetime | Count graph, pattern, identity and manifest capacities once per owned allocation, including Arc control storage; sharing does not create a free allocation. |
| Completed exact manifest and initial public 100 | Retained while later cells are counted and pages are served | Include sparse prefix counts and every retained first-page nested String/step allocation; cached sums must be invalidated on mutation. |
| Current DP memo and deterministic state tables | Current geometry-pattern count or rank request | Count actual memo buckets/entries and nested state storage. Do not silently retain all cells' memo tables after only their counts are needed. |
| Old and replacement memo | Cache refresh / new cell admission | Either drop the old owner before allocating the new one, or admit both simultaneously. The still-live partial public page does not disappear during a cache switch. |
| Canonical labels, subset constituents, interning and sorting storage | State expansion and memo insertion | Include nested capacities, container overhead, temporary sorting buffers and old/new backing allocation overlap. Pointer identity or a borrowed source reference does not account for new interned storage. |
| Rank/select reconstruction scratch | Until the selected trace is reconstructed and validated | Include path, board/depth, supply and coherent constituent provenance storage; retain cancellation and bounded work units. |
| Partial public page, one projected witness, and App/Host/JSON carriers | During each requested page of at most 100 members | Preserve the current public-page carrier reserve and original source reservation; do not reapply an eager whole-cell multiplier to unprojected DP state. |
| Browser whole-geometry export accumulator | Explicit copy from first member page through the last | This owner is outside WASM's 64 MiB allocation authority. Preserve all-member semantics, but provide a separate explicit export admission or streaming policy before allowing newly unbounded DP counts. |

Cancellation, exhaustion, or allocation failure must release new optional owners
without replacing the last valid source or reporting a complete partial count.
Add exact-cap and cap-minus-one tests for memo replacement while a partial public
page remains live, not just an isolated empty DP table.

### Count-width and complete export boundaries

The proposed counter uses `u128`, while the current App `PatternManifest`
`witness_count` / `end_offset`, geometry witness count and page offsets use
`usize`, which is 32-bit in the actual WASM target. Decide whether to preserve
the admitted limits or widen the complete rank/prefix/request chain. Unchecked
casts, saturation, or reducing an exact total to the first page would silently
lose members. Test count and offset boundaries above `u32::MAX` without creating
that many replay objects.

`collectPcReplayGeometryExportPages` deliberately fetches every declared member
page and checks both final witness count and distinct-pattern count. Keep those
checks: copying 100 visible members is not copying the selected geometry's
complete family. Its current `output.push` accumulator has no byte admission;
if DP removes the old raw execution cap, a newly cheap enormous exact count can
make explicit copy a new browser-memory failure path. Preserve the old cap until
the replacement policy is explicit, or provide admitted/streamed complete export
with honest failure. No empirical browser OOM is claimed by this source review.

The proposed versioned source digest does not itself demonstrate query binding.
Validate the source/query/pattern/policy contract before hashing, then retain the
existing generation, source-SHA, global metadata and selected-geometry metadata
checks on every page and copy operation. With those checks preserved, this review
identified no new stale-source authority loss caused solely by changing from a
trace-stream digest to an explicitly versioned immutable-source digest.
