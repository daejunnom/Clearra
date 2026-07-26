# Setup Finder

Setup Finder searches buildable prefixes of the fixed empty 10x4 perfect-clear
family. It does not generate arbitrary intermediate boards and does not rerun a
post-PC search for every visible candidate.

```text
empty 10x4 PC geometry family
-> family quotient partial BuildUp
-> exact hold/bag product coverage
-> setup-shape coverage union
```

## Search Modes

`oracle` is the default residue mode described below. Its input is an unordered
cycle residue, and coverage may select a different legal path for each complete
future pattern.

`qb` constrains the exact supply that must remain when the current PC finishes.
The input is the next PC cycle's complete remaining inventory:

```text
next-cycle hold, when occupied
+ unconsumed suffix of the active standard bag
```

For example, cycle-five residue `TI` reaches a next-cycle inventory of six
pieces. One duplicated kind is legal because one copy can be in hold:

```text
clearra setup --remaining TI --mode qb --qb OOSITZ
```

The order of `--qb` letters is irrelevant. Its exact count is derived from the
current cycle:

| Current PC cycle | Next-cycle remaining pieces |
| ---: | ---: |
| 1 | 4 |
| 2 | 1 |
| 3 | 5 |
| 4 | 2 |
| 5 | 6 |
| 6 | 3 |
| 7 | 7 |

Clearra keeps the broad current-cycle bag universe, derives the terminal
hold-and-suffix inventory for each exact supply state, and retains only the
compatible patterns. The retained patterns keep their original universe IDs
and weights, so QB probability is not renormalized into a conditional 100%.
This reverse terminal filter constrains current-cycle queues without requiring
the requested inventory to be locked into the setup.

QB uses the same inverse lock-clear family quotient and partial BuildUp search
as residue mode. It does not append the QB letters as a mandatory consumed
prefix.

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
Visible shapes merge only after the exact product coverage described below has
been computed.

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

Only then are exact-state words OR-unioned into a setup shape. Computing
`OR(BuildCoverage) AND OR(PcLiveness)` after shape grouping is forbidden because
it can combine incompatible temporal states.

Variant counts are never probabilities. Build and joint probabilities are
measured from `PatternBitSet` union coverage with the condition's weight model.
Each candidate reports:

- `Build`: probability that the setup shape is buildable.
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

The stable semantics are Oracle coverage: each concrete pattern may choose its
own legal path. An online observation-policy result is not exposed.

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

The setup card renders the setup mask as the existing field. Opening its
solution detail materializes only the exact suffixes that continue from that
field to an accepting perfect clear. These `solution_paths` are PC completion
placements retained by the canonical partial-build graph, not alternate
prefixes for constructing the setup and not queue/hold execution histories.
The browser reuses both the completed WASM worker and the matching immutable
partial-build graph for this follow-up request. A different setup query evicts
that one-entry graph cache before it starts, and terminating the worker releases
the cache.

Output limits are applied only after all shape coverage has been accumulated.
Allocation failure, cancellation, or incomplete source materialization must not
produce a complete-looking setup result.
