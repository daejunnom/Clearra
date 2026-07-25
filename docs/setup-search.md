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

## Residue And Hold Contract

The input is the complete supply remaining before the next bag boundary:

```text
current bag remainder
+ current hold piece, when one exists
```

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

At most one piece kind may occur twice. That duplicate is an explicit initial
hold piece. When no duplicate is present, empty hold and each possible occupied
hold are separate result conditions. They are never assigned an invented shared
probability.

Cycle-seven borrowing is provenance-based. By default, only the current
three-piece remainder and the next complete bag may be locked. The explicit
cycle-seven option permits one additional lock from the bag after that reset.
Seeing a piece in preview is not equivalent to locking it into a setup.

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
by exact union of their residual families. Placement reachability, SRS+ kick
rules, line-clear adjustment, and concrete realization identity are checked
before an edge enters the graph.

The root is not a setup. Every PC-live prefix from one through nine locks is
eligible. Lock depth is output metadata, not a pruning proof. Visible shapes
merge only after the exact product coverage described below has been computed.

## Exact Coverage Product

Each hold condition owns a materialized `PatternUniverse` and
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

The report separates hold conditions and includes the inferred cycle, canonical
residue, cycle-reset policy, geometry-family count, partial-graph node count,
candidate completeness, and exact representative placement/hold paths.

Output limits are applied only after all shape coverage has been accumulated.
Allocation failure, cancellation, or incomplete source materialization must not
produce a complete-looking setup result.
