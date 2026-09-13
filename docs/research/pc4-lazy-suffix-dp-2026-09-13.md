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
passed. Compiled Rust differential/A/B and App regressions are pending the
exact candidate's non-publishing CI; local Windows execution policy is not
bypassed to run Cargo/native binaries.
