# Tsar Half-Bag Geometry Bottleneck

## Reproduction

- Product surface: browser WASM worker
- WASM SHA-256: `bc1d0d89477f53bf6f377464347e9eb1d19d0b1506f57229efd14d81139f78a5`
- Field height: 8
- Base: `0x0`
- Target: `0xc0300e678ffbfcffbfdf`
- Target cells: 52, or 13 tetrominoes
- Supply: `P7[^T]!`
- Rule: SRS+
- Mirror: included
- Requested workers: 11

Raw reports are stored under `%LOCALAPPDATA%/Clearra/reports/geometry-regression-tsar-half`.

The 180-second bounded run reached 4,536,194 geometry nodes and emitted zero
candidates. Consequently all verifier workers remained idle and BuildUp performed
no work. The observed bottleneck is entirely before candidate verification.

## Geometry Certificate

The target consists of two spatially disjoint components with 24 and 28 cells.
Every physical column contains at least one target cell, so there is no empty
vertical separator column.

The baseline extended component implementation did not exploit this certificate:

- `COMPONENT_MAX_CELLS` is 16, so the 24-cell owner component is rejected.
- `SEPARATOR_JOIN_MAX_CELLS` is 24, but that join is only attempted for a fully
  empty vertical separator column. It therefore never sees this irregular split.
- The fallback explores the complete 52-cell residual as one exact-cover search.

## Additional Costs

1. Height 8 selects the extended path solely by visible height.
2. The extended hot residual uses four `u64` words even though the static target
   has only 52 canonical cells and can be densely represented by one `u64`.
3. The extended residual memo uses `HashMap<(Board256, [u8; 7]), u32>`. The compact
   engine instead uses packed piece counts and a cache-oriented chunked memo with
   exact confirmation.
4. The producer compiles the complete immutable solution-family DAG before it
   creates an enumerator. No verifier worker can help until compilation finishes.
5. Hall, column projection, and checker parity are disabled when `initial_board`
   is empty. This target is irregular but has an empty base, so those counters are
   necessarily zero on the active path.

## Implemented General Fix

The extended catalog now applies the handoff's static-geometry dense cell mapping:

```text
WorldCellCoord <-> CompactUniverseIndex
```

The 52 target cells use the compact `u64` exact-cover state while physical
coordinates and inverse-lock-clear realization data remain in cold catalog tables.
BuildUp continues to use the extended board contract.

Component composition consumes exact placement-hypergraph components rather than
only empty vertical columns. Each component is enumerated independently and the
component tables are joined by exact seven-piece count signatures. All tiling
families under a signature are retained in the immutable family DAG. The 24/28
split is therefore a benchmark instance of the general algorithm, not a
fixture-specific rule.

The dense residual memo uses an exact `(remaining_u64, packed_piece_counts)` key.
Piece counts occupy one byte each, so the key remains collision-free for the full
24-row contract. Hashes select buckets only; entries are confirmed by both exact
fields. If the component compiler reaches its optimization budget, it rewinds its
temporary family nodes and resumes the monolithic exact compiler instead of
dropping candidates.

After dense geometry and component signature join, candidates are streamed to the
existing persistent verifier workers. Geometry DAG compilation remains
producer-owned; replicating that compiler across workers would duplicate its
state and would not address the primary algorithmic loss.

## Measurement After The Fix

- Source snapshot SHA-256: `a6ab340fd9938f0bc2f3e64d06cea3fdb5ed53a4f2889f81be845c01754fc755`
- WASM SHA-256: `4b0b285a12f7fe800738ff1d9a1b60368c5b2916ff09e98d67c140157f7d7c93`
- Dense geometry isolation, two runs: 8.243s and 8.743s
- Geometry states per symmetry pass: 37,021
- Exact geometry family paths per symmetry pass: 1,815,415,498

The baseline emitted no candidate in 180 seconds after 4,536,194 geometry states.
The corrected path reaches a candidate in under 8.75 seconds, so observed
latency-to-first-candidate improves by more than 20.6x. Geometry state expansion
falls by 99.18%, or about 122.5x.

A separate 30-second bounded product run recorded this progress at 25.023s:

```text
candidates emitted: 603,904
candidates verified: 601,088
active verifier workers: 10 / 10
```

This confirms that the extended candidate stream reaches the browser's existing
multiworker verifier path. The remaining end-to-end bottleneck is no longer
geometry compilation: it is traversal and exact BuildUp verification of the
1.815-billion-path family. The bounded run intentionally does not claim a complete
probability result.

Small compact-path checks preserve their exact identities:

| Case | Runs | Solutions | Solution hash | Nodes | Peak CPU bytes | Wall time |
|---|---:|---:|---|---:|---:|---:|
| PCO | 2 | 63 | `cts1:8415f86603b3be9d` | 355 | 6,227,054 | 125.130-130.265ms |
| Tsar Cannon | 2 | 42 | `cts1:4996a1501bbb8212` | 631 | 7,158,590 | 144.480-205.100ms |

The compact cases do not enter the extended engine. Their state counts and peak
memory exactly match the pre-change records. Browser wall time includes Worker
creation and WASM loading, so the isolated 205ms Tsar sample is retained as startup
variance rather than removed from the report.

Final raw reports are under:

```text
%LOCALAPPDATA%/Clearra/reports/geometry-regression-tsar-half/dense-u64-count-key-final
%LOCALAPPDATA%/Clearra/reports/geometry-regression-tsar-half/small-count-key-final
```

The complementary 28-cell interpretation was also measured during diagnosis but
is not this benchmark and must not be used as a performance or correctness result.

## Producer And Verifier Balance Audit

The browser progress counters do not show a verifier outrunning the producer.
With 11 requested workers, one worker owns production and ten workers verify
256-candidate batches. The verifier pool admits one in-flight batch per worker and
`enqueue()` waits when all clients are busy. Consequently these two steady-state
backlogs have precise meanings:

```text
2,560 = 10 verifier batches in flight
2,816 = 10 verifier batches in flight + one produced batch waiting for a client
```

All sampled runs held `active_workers = 10 / 10`. Near-equality between emitted
and verified counts is therefore bounded backpressure, not evidence that
verification is faster. Candidate traversal is fast enough to saturate the
verifier pool, and aggregate exact verification throughput controls the stream.

Two fresh 30-second runs were compared with the two earlier reports using the
same command, the same 37,021 geometry states, and the first five progress samples:

| Artifact | Verified candidates/s | BuildUp nodes/s |
|---|---:|---:|
| Earlier dense join | 29,963.2 | 55,974.1 |
| Earlier final | 29,862.6 | 55,710.5 |
| Current run 1 | 30,091.8 | 56,312.3 |
| Current run 2 | 28,637.2 | 51,356.4 |

The earlier mean is 29,912.9 candidates/s and the current mean is 29,364.5
candidates/s, a 1.8% difference inside the approximately 5% spread of the two
fresh runs. This does not establish a performance regression. BuildUp work per
candidate also differs over the bounded prefix, so raw BuildUp-node throughput is
not an independent regression signal.

The current progress-enabled WASM SHA-256 is
`43f1c04ba0a6c3145146edbbc7a29e33bd23a2799f0061c8ccea51a4622c4f8a`.
Its exact family-count reads occur at the 50ms progress boundary rather than in
the candidate or BuildUp inner loops. No producer/verifier scheduling change is
warranted by this audit.

Fresh raw reports are under:

```text
%LOCALAPPDATA%/Clearra/reports/producer-verifier-regression/current-r1.json
%LOCALAPPDATA%/Clearra/reports/producer-verifier-regression/current-r2.json
```

## Tiling Family Semantics And Uniqueness Audit

`P7[^T]!` has one packing multiset for this 13-piece target:

```text
I=2, O=2, T=1, S=2, Z=2, J=2, L=2
```

That supply identity is not the geometry family cardinality. A geometry tiling is
the order-independent set of `(piece kind, final four-cell mask)` placements that
partitions every required cell exactly once. Every tiling fills the same final
board, but different cell partitions or piece assignments remain different
tilings. Build order, hold decisions, kick evidence, and inverse-lock-clear
realizations are downstream variants and are not multiplied into this count.

The values measured for one original-field pass therefore have distinct meanings:

```text
packing multiset groups: 1
memoized exact-cover states expanded: 37,021
unique tiling-family paths represented by the DAG: 1,815,415,498
```

The family DAG is a compressed set expression. `Append` preserves cardinality,
`Union` adds disjoint pivot branches, and `Product` multiplies independently
certified hypergraph components. Its large path count is not its stored node
count.

Duplicate tilings are excluded by these invariants:

1. The inverse catalog deduplicates skeletons by exact `(piece, final cells)`;
   rotations and lock-clear realizations with that same geometry share one row.
2. Each exact-cover residual chooses one deterministic pivot cell. A completed
   tiling contains exactly one selected row covering that pivot, so sibling
   branches are disjoint and a placement set has one canonical traversal order.
3. Repeated pieces are count-vector entries, not labelled copies, so exchanging
   two identical piece instances does not create factorial duplicates.
4. Residual memo lookup compares both the full remaining-cell mask and exact
   packed piece counts; hashes only select buckets.
5. Component products join disjoint cell components once, and a tiling's piece
   count vector selects exactly one supply target group.

A manual product-coordinator audit additionally canonicalized the first 1,000,000
emitted candidates by sorted skeleton-row set and found zero duplicates. The
audit used the same command and confirmed `geometry_nodes = 37,021` and
`candidate_family_count = 1,815,415,498`. The diagnostic code was removed after
the audit; no product-path deduplication or hot-loop validation was added.
