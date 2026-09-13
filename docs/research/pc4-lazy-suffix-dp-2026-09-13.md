# PC4 bounded lazy graph-suffix reuse

This is an implementation step toward graph/supply/hold DP, not whole v0.9.0
completion, actual HF profile qualification, or release authority.

## Repeated work removed

The observation owner previously prepared an independent fixed-queue walk for
every reveal ordinal and hold history. Reconvergent paths repeated the same
remaining graph search even when the Range provider already had the bytes.

The new memo retains validated canonical adjacency and proved-empty suffixes.
Its request-local owner binds the qualified generation/profile/target. Keys
also contain the current field, consumed queue index, remaining placement queue,
and early-terminal contract. The consumed index is retained because it is part
of the qualified adjacency query; it is not inferred from a field ID.

Only the manifest's pure field terminal opts in. Generic/stateful terminal
callbacks keep the ordinary path. Changing terminal mode mid-cursor or sharing
a memo across qualified targets is rejected. This selects existing terminal
semantics; it does not mint completeness authority.

## Completeness and boundedness

- Every positive prefix/path is still emitted in canonical order, with its own
  reveal ordinal, exact probability, hold history and remaining observation.
- Empty is recorded only after every descendant of that suffix has completed
  with no terminal emission. A work slice with no outputs is not empty proof.
- Range failure, cancellation, stale generation, or a failing page cannot
  publish its staged facts. Successful graph-page facts may outlive an outer
  App/observation transaction retry; they never advance that owner's cursor or
  publish a candidate/result, and remain guarded by the same immutable target.
- Provider and freshness checks still run on warm-cache requests.
- Retention counts metadata, suffix pieces and target IDs, up to the smaller
  of the work budget and 65,536 elements. Capacity/allocation/poison cache misses
  repeat ordinary work, never truncate the solution family.
- Pages stage only their new facts. They do not clone the entire memo per page.
  Cursor clones share immutable facts behind a short critical section, not
  progress or probability ledgers. No new native thread/pool is introduced.

The full finite input is still enumerated by the original ranked frontier.
Concrete placement materialization is still independent and is not memoized by
this change. Those costs and public-surface Range integration remain open.

## Differential and A/B design

`lazy_fixed_queue_suffix_tests.rs` compares all exact graph paths for multiple
page budgets, both positive and negative reconvergent DAGs, different prefixes,
different terminal-depth contracts, cache saturation, failed/retried requests,
and stale/cross-profile owners. The observation test separately compares entire
path values including reveal/hold/probability provenance, not just output count.

For the two-node-per-layer depth-d fixture, ordinary adjacency queries number
`2^d - 1`; retained suffix queries number `2*d - 1`. The all-negative fixture
visits `2^(d+1) - 1` occurrences ordinarily and `4*d - 1` with sufficient memo
capacity. The positive fixture still emits all `2^(d-1)` distinct prefixes.
These identities concern this fixture topology, not arbitrary-graph or total
PC runtime bounds. Cache saturation can return to ordinary traversal cost.

The explicit non-publishing `pc4_suffix_dag_abba` test runs A/B/B/A four times
per positive/negative depth-10 family (eight runs per implementation). It emits
wall time, adjacency calls, visited occurrences and path counts. There is no
timing assertion or production-minimum performance claim.

Local checks: Rust formatting, JS workflow/jobserver contracts (13 passed,
one POSIX-only skip), PC4 authority validator contracts, and `git diff --check`
passed. Local Windows execution policy was not bypassed to run Cargo/native
binaries.

## Exact compiled evidence

Code `6d03ccbbd2037842de9facc37f4194506b9a80db` passed all four jobs of
non-publishing [34749465353](https://github.com/daejunnom/Clearra/actions/runs/34749465353).
PC4 job `103703235287` reports Core 7+5, Tablebase 178 (one A/B ignored in
ordinary selection, then explicitly run and passed), Replay 20, Postprocess
41, App PC4 87, three selected Supply tests, the compact-input A/B, and App
replay 16 passing. The pre-existing App matrix retains its 1,090 paired product
cases; the new suffix differential adds live/empty topology and paging cases.
Native job `103703235336` passed all 17 real CLI process tests (115.78s test
runtime), with no LNK4098/LNK2038 in its log. Node-host punycode deprecation
warnings remain; they are not relabelled as Rust warnings or suppressed.

| Depth-10 fixture | Baseline mean | Suffix memo mean | Adjacency calls A/B (8 runs) | State visits A/B (8 runs) | Paths A/B (8 runs) |
| --- | ---: | ---: | ---: | ---: | ---: |
| All negative | 11.748814ms | 0.319642ms | 8,184 / 152 | 16,376 / 312 | 0 / 0 |
| All positive | 12.864806ms | 12.659364ms | 8,184 / 152 | 12,280 / 12,280 | 4,096 / 4,096 |

Raw elapsed nanosecond totals were `[93990514, 2557134]` and
`[102918444, 101274912]`. This confirms a large gain for reconvergent failed
suffixes, while positive enumeration time is effectively unchanged in this
test-profile sample. Do not claim a general 36.8x PC speedup: positive path
prefix copying, transaction work, concrete materialization and final reduction
remain independent costs, and production network/WASM performance was not run.

## Shared-prefix and exact-cap follow-up

Code `26ca011167e24b54b7b04ddea231eac096ebde9b` replaces pending flat paths
with shared immutable linked prefixes. Branch extension and transactional
cursor cloning no longer clone every prior edge. Only emitted paths allocate
the flat edge list required by the existing API. Branch/canonical order,
reveal/hold provenance and suffix memo semantics are unchanged. Iterative
parent release avoids a recursive destructor for long explicit queues.

The pre-change flat implementation exists only under `cfg(test)` for isolated
A/B; both prefix variants use the same suffix memo. Exact path equality, not
only counts, is checked across depth 1/4/8 and work budgets 1/3/64.

The observation output-cap fix probes at most one additional path under the
ordinary work slice after reaching the exact output budget. Empty trailing
frontiers may establish exhaustion. An actual additional path still produces
`OutputPaths { limit: 2, attempted: 3 }` before publication, without advancing
the caller cursor. Page sizes 1/8 cover exact success and real overflow.

Non-publishing run [34750309184](https://github.com/daejunnom/Clearra/actions/runs/34750309184)
passed all four contract jobs: Tablebase 182 (two explicit A/B tests ignored in
the ordinary selection, both subsequently run/passed), Core 7+5, Replay 20,
Postprocess 41, App PC4 87, App replay 16 and native CLI 17. Its newly added
preview-only WASM job failed before compilation because a platform helper was
not loaded. `f1a77a8` fixes that workflow dependency; this first run must not be
reported as an entirely successful workflow.

The follow-up [34750402366](https://github.com/daejunnom/Clearra/actions/runs/34750402366)
at `f1a77a8230207be03fb89335fb7a913d165211b3` passed all five jobs, including
the unqualified preview WASM build. Its contract selections again passed; the
depth-10 positive prefix ABBA totals were `[122986071,74061488]` ns across
eight runs each (15.373259ms versus 9.257686ms). This repeats the same synthetic
checkpoint, not an additional real-field sample. The verified preview was
imported and served on 4194; see `pc4-empty-p7p4-4194-2026-09-13.md` for why
actual HF empty/P7P4 timing is still not available.

| Shared-prefix ABBA fixture | Flat mean | Shared mean | Queries A/B (8 runs) | Visits A/B | Paths A/B |
| --- | ---: | ---: | ---: | ---: | ---: |
| Depth 8, negative | 0.298430ms | 0.220328ms | 120 / 120 | 248 / 248 | 0 / 0 |
| Depth 8, positive | 3.246225ms | 2.077046ms | 120 / 120 | 3,064 / 3,064 | 1,024 / 1,024 |
| Depth 10, negative | 0.418920ms | 0.264279ms | 152 / 152 | 312 / 312 | 0 / 0 |
| Depth 10, positive | 15.759473ms | 9.113920ms | 152 / 152 | 12,280 / 12,280 | 4,096 / 4,096 |

Raw elapsed ns totals, in table order: `[2387438,1762624]`,
`[25969798,16616371]`, `[3351356,2114231]`, `[126075780,72911357]`.
Each side ran eight times in four ABBA cycles. Positive depth-10 time fell
about 42% in this test-profile fixture, with identical outputs and visits.
This is neither an empty/P7P4 measurement nor a production HF/WASM speedup.

## Next boundaries to close

1. Share concrete materialization beyond the now-shared graph prefixes without eagerly
   collecting/counting every replay before the first requested result.
2. The preview build/import and GUI display check are complete; the exact-cap
   execution regression above is also no longer an open bug. Actual online
   empty/P7P4 timing remains coupled to the public adapter and qualification.
3. Continue actual profile/terminal/materializer qualification and public
   CLI/Web/Desktop/Discord transport/fallback integration. The active goal and
   the plan's full-DP/release checkboxes remain open.
