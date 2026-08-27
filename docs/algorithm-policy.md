# Algorithm Policy

Clearra PC search must preserve completeness before performance shortcuts. The
product path is:

`SearchProblem -> C PackingProblem -> C Geometry Skeleton Exact Cover -> Host reducer -> C BuildUp BFS -> CoverageRow -> CoverageMatrix -> ObjectiveResult`

## Forbidden Search Families

Meet-in-the-middle PC search is not part of Clearra. Independent
front-half/back-half PC joins are forbidden for the same reason: line clears,
y adjustment, grounded placement, queue order, and hold transitions make those
joins unsound. Exact component composition is allowed only after the placement
hypergraph proves that no legal row crosses the separator; its piece-count
signature join is not an arbitrary board half-join.

The following names are forbidden in product source:

- `MeetInTheMiddlePacking`
- `mitm_pc_backend`
- `half_join_pc`
- `front_half_packing`
- `back_half_packing`
- `complement_join_pc`
- `mitm_static_tiling_in_search_path`

The completion marker is `architecture_validation_rejects_mitm_pc_backend`.

## Geometry Skeleton Exact Cover

The product geometry engine compiles every concrete inverse lock-clear
realization once, quotients realizations with the same piece and canonical cell
ownership into placement skeleton rows, and builds an immutable cell-support
CSR. CPU and GPU executors borrow that same catalog; they do not regenerate or
copy operation tables per worker or batch.

Search uses Bitset Algorithm X with a deterministic global minimum-domain pivot.
The continuation key compares remaining cells, exact used-piece state, and every
active projection/constraint/component identity. Hashes select buckets only.
Negative residual memo entries are authoritative only after an exhaustive search
with the complete exact key; saturation becomes an insertion skip.

Independent placement-hypergraph components may be solved separately and joined
by exact piece-count signatures. Every predecessor solution family is retained
through immutable Append/Union/Product DAG nodes. No representative path may
replace a family, and no queue, hold state, score, spin, fumen, or replay payload
belongs to geometry search.

## Non-Backend Verification Helpers

The following names are allowed only as verification helpers, not alternate PC backends:

- `SmallComponentExactCover`
- `AreaFeasibilityChecker`
- `ComponentExactCoverVerifier`

These helpers may validate a compiled component or fixture independently. Product
component exact-cover remains part of Geometry Skeleton Exact Cover; a helper may
not turn its own candidate into a product solution, BuildVariant, CoverageRow, or
exact probability source.

## Required Deletion Proof

Search candidate deletion is allowed only when the reason is exact. The
strongest queue/hold deletion proof is:

`BuildOrders(P) intersection HoldReachableOrders(Q) is empty`

That proof means every build order for the packing candidate is incompatible
with every hold-reachable order for the active piece source. Anything weaker is
not a deletion proof.

The current explicit-order language module is a test-only scaffold, not an
independent deletion proof. An already accepted BuildVariant must never be
converted to one synthetic token and inserted into both languages. Until
BuildOrders and HoldReachableOrders have independent generators, product
coverage uses pattern-specific BuildUp and no language-intersection candidate
deletion is authorized.

The completion marker is `BuildOrders(P)_intersects_HoldReachableOrders(Q)_empty_proof`.

## Forbidden Heuristic Pruning

The following signals must not remove search candidates:

- MCTS low score
- rare piece heuristic
- bad shape heuristic
- probably impossible
- no immediate placement
- target-frame floating
- spin classifier unknown
- score below threshold
- first witness missing
- representative order failed
- Bloom filter false positive
- resource cap reached

The completion marker is `architecture_validation_rejects_heuristic_prune_reason`.

## Coverage And Witness Guard

Coverage and probability require accepted BuildVariants and PatternBitSet union.
A representative order, first witness, or failed representative replay is never
enough to prove complete coverage, no coverage, or impossible search.

The completion markers are:

- `architecture_validation_rejects_representative_order_only_coverage`
- `architecture_validation_rejects_first_witness_coverage`
