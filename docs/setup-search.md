# Setup Finder

Setup Finder searches buildable prefixes of the fixed empty 10x4 perfect-clear
family. It does not generate arbitrary intermediate boards and does not rerun a
post-PC search for every visible candidate.

```text
empty 10x4 PC geometry family
-> family quotient partial BuildUp
-> exact hold/bag product coverage
-> exact fixed-tiling coverage
-> one ranked state per visible board
```

## Setup Search Modes

`--mode oracle` is the default shape/residue mode described below. Its input is
an unordered cycle residue.

`qb` conditions the next bag on the piece group currently visible to the
player. For example, cycle-five residue `TI` with observed next pieces `OS`
uses:

```text
clearra setup --remaining TI --mode qb --qb OS
```

Observed QB pieces are distinct and must fit in the same seven-piece bag as the
current residue. Their input order does not fix their draw order. They are
available to partial BuildUp, but a setup does not have to lock every observed
piece. Clearra represents the example as an exact conditioned pattern language
equivalent to:

```text
[IT]![OS]![^OS]!P2
```

Both `oracle` and `qb` may independently constrain the exact supply left when
the current PC finishes:

```text
clearra setup --remaining TI --next-cycle-remaining OOSITZ
clearra setup --remaining TI --mode qb --qb OS \
  --next-cycle-remaining OOSITZ
```

`--next-cycle-remaining` is the complete inventory at the next PC-cycle
boundary:

```text
next-cycle hold, when occupied
+ unconsumed suffix of the active standard bag
```

Its order is irrelevant. The exact count is derived from the current cycle:

| Current PC cycle | Next-cycle remaining pieces |
| ---: | ---: |
| 1 | 4 |
| 2 | 1 |
| 3 | 5 |
| 4 | 2 |
| 5 | 6 |
| 6 | 3 |
| 7 | 7 |

One duplicated kind is legal in this terminal inventory because one copy can be
in hold. Clearra derives the terminal hold-and-suffix inventory for each exact
supply state and retains only compatible patterns. The retained patterns keep
their original universe IDs and weights, so the result is not renormalized into
a false conditional 100%.

QB uses the same inverse lock-clear family quotient and partial BuildUp search
as residue mode. Observed QB conditioning and the optional terminal inventory
filter are separate axes and may be combined.

## Queue Knowledge

Setup generation mode and future queue knowledge are independent:

```text
--mode oracle|qb
--queue-knowledge oracle|visible-7
```

`--queue-knowledge oracle` is the compatibility default. It assumes the whole
materialized future queue is known, so each complete pattern may choose a
different legal placement path.

`--queue-knowledge visible-7` exposes the current hold and the next seven queue
pieces. Queues with the same observation must choose the same placement/hold
action. After a lock consumes source pieces, newly visible pieces create the
next exact observation class and the policy may branch there.

Visible-seven coverage is evaluated on the complete materialized pattern
universe. It does not discard hidden suffixes or replace them with one
representative queue.

## Residue And Hold Contract

The input is the complete supply remaining before the next bag boundary:

```text
current bag remainder
+ current hold piece, when one exists
```

The input syntax accepts setup-queue tetromino letters only. Letter case is
ignored, spaces and commas may be used as separators, and written order does
not prescribe placement order. Do not append `P7`, bracket expressions, `!`, or a
later PC completion queue. Clearra derives completion supply from the residue
count and bag-cycle contract.

The residue count determines the PC cycle.

| Remaining pieces | PC cycle |
| ---: | ---: |
| 7 | 1 |
| 4 | 2 |
| 1 | 3 |
| 5 | 4 |
| 2 | 5 |
| 6 | 6 |
| 3 | 7 |

The residue also determines the probability denominator. For example,
`--remaining I` means that I is the guaranteed current-cycle residue, so a
legal one-I setup has 100% Build coverage. With `--remaining IOTSZJL`, a
one-I-only setup has 2/7 Build coverage from an empty hold: I must be the
current piece or the next piece after storing the current piece. Future bag
patterns do not replace or dilute the explicit residue prefix.

Product UI requests always start with an empty initial hold and require each
residue piece kind to be unique. The CLI alone may request an occupied initial
hold:

```text
clearra setup --remaining SIOS --initial-hold S
```

The selected piece must occur in `--remaining`; Clearra removes one matching
copy from the queue remainder and starts it in hold. A request without
`--initial-hold` never expands into multiple occupied-hold searches.

Cycle-seven borrowing is provenance-based. By default, only the current
three-piece remainder and the next complete bag may be placed. The explicit
cycle-seven option permits one additional setup piece from the bag after that
reset. Seeing a piece in preview is not equivalent to placing it in a setup.

## Geometry And Partial BuildUp

The WASM CPU backend compiles the inverse lock-clear skeleton catalog once and
represents all complete tilings as an immutable geometry solution family.
Removing a concrete placement from that family produces a quotient family. The
partial graph therefore stores:

```text
exact board after lock and clear
deleted-row state
residual geometry family
concrete placement edges
```

Nodes with the same future-relevant board and deleted-row state may merge only
by exact union of their residual families. Placement reachability, the selected
SRS+/SRS/SRS-X/Jstris 180 kick profile, line-clear adjustment, and concrete realization identity are checked
before an edge enters the graph.

The root is not a setup. Every PC-live prefix from one through ten placements
is represented. Placement depth is output metadata, not a pruning proof.
States that leave the same occupied board remain separate through coverage when
their concrete placement-row set or deleted-row state differs. After coverage,
the requested ranking policy selects one exact state for each visible board.

`--max-setup-pieces N` selects which represented depths may enter the result,
with `1 <= N <= 10`. The product default is `9`, preserving nine-piece setups
while avoiding a result list dominated by terminal ten-piece perfect clears.
Selecting `10` deliberately includes complete PC solutions, whose conditional
PC probability is necessarily 100%.

## Exact Coverage Product

The selected supply condition owns one materialized `PatternUniverse` and
`WeightedPatternSet`. Coverage runs over:

```text
partial-build node
x queue cursor
x hold state
x one optional empty-hold extra draw
x pattern word
```

For every exact state:

```text
BuildCoverage = forward-reachable patterns
PcLiveness = patterns with a legal completion
JointCoverage = BuildCoverage AND PcLiveness
```

Only then are supply-state words OR-unioned within one exact partial-build
state. A candidate identity retains the canonical concrete placement-row set,
deleted-row state, piece-count state, visible board, and placement count.
Coverage from a different tiling state or a shorter setup is therefore never
inherited merely because it leaves the same board after line clears. After all
exact states have been evaluated, the selected ordering policy chooses one
state for each visible board card.
Computing `OR(BuildCoverage) AND OR(PcLiveness)` after grouping is forbidden
because it can combine incompatible temporal states.

Variant counts are never probabilities. Build and joint probabilities are
measured from `PatternBitSet` union coverage with the condition's weight model.
Each candidate reports:

- `Build`: probability that the selected fixed setup tiling is buildable.
- `Joint`: probability that the setup is buildable and the same exact state can
  complete the PC.
- `Conditional`: `Joint / Build`.

Candidate ordering has two independent axes and never changes coverage:

- `--priority all` ranks `Joint` (`Build * Conditional`) first.
- `--priority build` ranks `Build` first.
- `--priority pc` ranks `Conditional` first.
- `--setup-length longer` prefers the greatest legal placement count when the
  primary probability ties.
- `--setup-length shorter` prefers the least legal placement count when the
  primary probability ties.
- `--setup-length auto` chooses `longer` for `all` and `build`, and `shorter`
  for `pc`.

The resolved setup-length preference also selects the displayed representative
path. Placement count affects ordering and representative selection only. It
is never a candidate-removal proof, because distinct shapes and coverage sets
remain semantically relevant.

The report identifies either `full-future-oracle` or
`visible-seven-policy`. Build and Joint are each the exact maximum weighted
coverage for the selected queue-knowledge contract. `Conditional` is the
reported Joint-to-Build coverage ratio; under visible-seven these two maxima
may be attained by different policies, so it is an analytical ratio rather
than a claim that one policy attains both optima.

## Product Boundary

The product route is:

```text
CLI or web command
-> SetupAppCommand
-> setup validation
-> WasmSetupSearchBackend
-> SetupFinderReport
```

The report preserves the selected CLI hold condition and includes the inferred
cycle, canonical residue, cycle-reset policy, geometry-family count,
partial-graph node count, candidate completeness, and exact representative
placement/hold paths. Product UI requests contain only the empty-hold
condition.

The setup card renders the selected fixed tiling as the existing field. Its
opaque setup ID encodes the board, deleted-row state, and canonical placement
set. Opening solution detail materializes only the exact suffixes that continue
from that same state to an accepting perfect clear. These `solution_paths` are
PC completion placements retained by the canonical partial-build graph, not
alternate prefixes for constructing the setup and not queue/hold execution
histories.
The browser reuses both the completed WASM worker and the matching immutable
partial-build graph for this follow-up request. A different setup query evicts
that one-entry graph cache before it starts, and terminating the worker releases
the cache.

Output limits are applied only after all exact-state coverage has been accumulated.
Allocation failure, cancellation, or incomplete source materialization must not
produce a complete-looking setup result.
