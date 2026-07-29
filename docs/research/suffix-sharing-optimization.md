# Suffix Sharing Optimization

This record prevents rejected cache and sharing experiments from being repeated.
Raw benchmark events are stored outside the repository under
`%LOCALAPPDATA%\Clearra\reports\suffix-sharing-optimization`.

Final verified WASM artifact:
`8b19b4324e196bb21d70ce97f202b7ecd2b71eec0b69331d79284096c470cf64`.

## Exactness anchors

| Workload | Expected result |
|---|---|
| Empty 4L P7P3 | 208,437 solutions, `cts1:938c9254b965d33b` |
| Large build probability (`P7[^T]4`) | 3,018 solutions, `cts1:4a1f5df1599fc97a`, 1,814,400 / 1,814,400 patterns |

All retained experiments preserved these anchors.

## Baselines

| Workload | Runs | Median elapsed | Reported peak |
|---|---:|---:|---:|
| Empty 4L P7P3 | 32.38 s, 29.12 s | 30.75 s | about 3.71 GiB |
| Large build probability | 25.43 s, 24.54 s | 24.98 s | 348,445,702 bytes |

Elapsed time is the product-reported search time. Browser runner startup and
shutdown time is excluded.

## Retained changes

### CountUnique operation/piece edge sharing

For a standard-board CountUnique graph, each operation selects at most one
reachable lock and each operation creates a distinct child subset. The
piece-projection edge stream is therefore exactly the operation edge stream.
The graph now aliases that immutable stream instead of copying it.

The optimization is restricted to `ClearToEmpty`. Applying it to exact target
board build-probability searches measured slower, so those searches retain the
previous separate piece-edge stream.

P7P3 after the restricted change:

| Run | Elapsed |
|---:|---:|
| 1 | 27.87 s |
| 2 | 29.93 s |
| 3 | 30.04 s |

Median: 29.93 s.

### Realization-feasibility proof reuse

The realization-feasibility pass already stores exact failed subsets. A
generation token now binds those proofs to the same candidate and completion
contract. BuildOrderGraph skips a child only when that exact token proves the
child cannot complete. Scoring graph construction, which does not run the
feasibility producer, cannot consume these proofs.

The total P7P3 build-node count fell from 53,361,322 to 53,335,509.

| Run | Elapsed |
|---:|---:|
| 1 | 28.77 s |
| 2 | 28.74 s |
| 3 | 27.68 s |

Median: 28.74 s.

### Canonical-operation partial dependency graph

The optional `--build-dependency-dag` path retains the complete live subset
graph produced by realization feasibility instead of reducing it to
must-precede intersections. Each edge is labeled by canonical operation ID.
Diamonds such as `A -> AB <- B` preserve both parents, and the existing exact
BuildOrder subset node remains the shared child. Frequency, arrival order, and
representative paths are not pruning inputs.

BuildUp reuses the live operation mask before reachability and skips its
redundant failed-child lookup while the generation-bound graph remains valid.
Missing allocation or a stale generation uses the baseline BuildUp path.

Empty 4L P7P3, same WASM and browser worker:

| Mode | Runs | Midpoint |
|---|---:|---:|
| Disabled | 39.65 s, 33.16 s | 36.41 s |
| Enabled | 35.21 s, 33.61 s | 34.41 s |

Both modes returned 208,437 solutions with
`cts1:938c9254b965d33b` and `count_complete=true`. Enabled execution reduced
reachability lock queries from 132,718,961 to 51,906,735 and partial
reachability searches from about 119.75 million to about 49.41 million.
Complete feasibility work rose from 26,827,381 to 75,376,539 states, which
limits the end-to-end gain. Peak memory showed no repeatable increase; one
enabled run was about 2.3% higher and subsequent runs were level with disabled.

A fixed 2L `IIOOO` check measured 93.43 ms disabled and 91.81 ms enabled; both
returned four solutions with `cts1:8dc81db9bcd4bab9`. The option remains
disabled by default because candidate shape and workload determine whether the
complete-graph precomputation pays for itself.

Final verified WASM after the redundant child-proof lookup was removed:
`8b19b4324e196bb21d70ce97f202b7ecd2b71eec0b69331d79284096c470cf64`.

### Extended topological edge sharing

The extended-board producer emits at most one edge per operation. Distinct
operations have distinct child subsets, so its piece transitions are unique.
`from_topological_parts` now accepts this explicit contract, validates adjacent
duplicate transition keys, and aliases the operation edge stream.

Large build-probability runs:

| Run | Elapsed |
|---:|---:|
| 1 | 26.17 s |
| 2 | 25.20 s |
| 3 | 24.96 s |

Median: 25.20 s. The reported peak remained 348,445,702 bytes because another
retained structure determines the peak.

## Rejected changes

### First-use coverage cache admission

One diagnostic run observed:

| Metric | Value |
|---|---:|
| Cache hits | 0 |
| Cache misses | 5,080 |
| Keys seen once | 5,050 |
| Keys seen twice and admitted | 15 |

First-use admission would retain 5,065 entries to avoid only 15 second
computations. Keep the existing two-use admission policy.

### Unrestricted piece-edge sharing

Sharing the operation edge stream in exact-target build-probability searches
measured 26.61 s and 26.01 s versus the 25.43 s and 24.54 s baseline. Keep the
optimization restricted to the PC `ClearToEmpty` path.

## Already shared or intentionally repeated

- CandidateProjection memoizes board and deleted-row state across feasibility
  and BuildOrderGraph.
- Geometry residuals use an immutable solution-family DAG.
- Piece languages are canonicalized and interned.
- Standard-bag products and reachability use exact caches.
- Identical horizontal-mirror fields do not start a second pass.
- A hard-drop miss is passed to the full reachability path instead of repeating
  the hard-drop query.
- Mirror-distinct fields and separate worker WASM memories are not the same
  computation and must not be merged without a new exact contract.
