# APDP, column M4, and checkerboard joint-domain audit

Status: v0.8.1 implementation evidence; this note does not authorize a release,
legal-board activation, or a change to ILC/BuildUp/reachability authority.

## Result

The column residue and checkerboard imbalance can safely share one small
necessary-condition domain. Clearra now compiles that domain from the actual
ILC Geometry rows. The source-level APDP implementation should remain separate:
it is an exact parent-row index for a proved three-cell Same-Tile domain, not a
global additive-state cache.

## Proof of the joint filter

For a catalog row `r`, define

- `m(r) = sum(x for (x, y) in r) mod 4`, and
- `c(r) = (sum((-1)^(x+y) for (x, y) in r)) / 2`.

Both are integers for a four-cell tetromino row and both are additive over
disjoint cells. A valid completion with rows `r_1, ..., r_n` therefore has

`(m(E), c(E)) = sum_i (m(r_i), c(r_i))`,

where `E` is the normalized residual target-frame cell set. For each remaining
piece kind, the implementation records every pair emitted by an actual catalog
row and convolves those pairs according to the remaining piece multiplicities.
The convolution deliberately ignores overlap and exact column demand. It is
therefore a superset of valid completions: absence of the demanded pair proves
impossibility, while presence proves nothing. This establishes the required
no-false-negative direction.

Let `D` be that joint domain. The old independent test accepted when
`m(E)` was in the first projection of `D` and `c(E)` was in the second
projection. The two witnesses could be different rows. The new test requires
the pair `(m(E), c(E))` itself to be in `D`, so it can never be weaker. The
executable strict-dominance fixture has two T rows with pairs `(0,+1)` and
`(1,-1)`: the independent filters accept `(0,-1)`, while the joint domain
rejects it.

For a certified standard tetromino catalog, all non-T rows have checker value
zero and T rows have checker value `+1` or `-1` in these half-units. The product
path consequently uses the existing four-bit M4 convolution for every non-T
piece and retains a four-by-`u128` joint state only across T copies. This keeps
the common per-node work bounded and allocation-free. Ten-wide 1..6L PC has at
most fifteen remaining tetrominoes; the implementation supports sixteen. A
larger extended state or a non-standard checker catalog fails open to the
existing M4-only condition.

The relation already documented for even-cardinality cell sets,

`V(S)/2 = |S|/2 + M4(S) (mod 2)`,

means the M4 coordinate still subsumes the proposed vertical parity. The joint
domain adds checkerboard information; it does not reintroduce vertical parity
as a separate filter.

## Why APDP is not merged into this cache

`geometry_apdp.rs` maps an exact three-cell partial to complete parent row IDs.
`geometry_domain.rs` uses that index only after the current feasible-row domain
has proved the Same-Tile premise complete. Its key is a concrete partial-cell
mask and its result is a parent placement domain.

By contrast, the projection key is a remaining piece-count vector and its
state summarizes the whole residual target. Adding APDP flags to that key would
not create a stronger necessary condition: after choosing an APDP parent row,
the next Geometry node already subtracts that row and reapplies the joint
residual filter. Precomputing the same test per APDP parent would duplicate work
and would need the remaining-cell mask, inventory, target-frame identity, and
catalog identity in the cache key. Omitting any of those owners can incorrectly
transfer a static local conclusion across line-clear realizations.

APDP and the additive filter therefore remain ordered, independent reducers:

1. normalize the initial field and target frame;
2. run the allocation-free M4/checker necessary condition;
3. derive exact APDP parent rows only where Same-Tile completeness is proved;
4. retain ILC exact cover, BuildUp, reachability, and terminal clear validation
   as final authorities.

No legal-board index is enabled by this change, and no rotation/kick profile is
special-cased. SRS, SRS+, SRS-X, Jstris-180, and no-kick catalogs all derive the
projection pairs from their own emitted occupancy rows.

## Executable evidence and A/B seam

The focused tests in `geometry_projection.rs` provide three independent checks:

- exhaustive comparison of the optimized convolution with brute-force row
  selection for every seven-piece count vector of total size at most four;
- a strict A/B witness that the independent M4 and checker marginals both
  admit while the joint domain rejects;
- realizable normalized nonempty initial fields for every 10-wide target from
  1L through 6L, all of which remain admitted.

These tests prove safety and strict logical pruning. They are not a product
latency benchmark and must not be reported as a measured end-to-end speedup.
Default-on use is justified for the certified 1..6L standard catalog because
the filter is allocation-free, bounded, and fail-open outside its proof window.
An ABBA product benchmark remains the authority for deciding whether a future
implementation should change its evaluation order.
