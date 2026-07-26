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

`qb` adds pieces observed from the following standard bag to the normal cycle
residue:

```text
clearra setup --remaining TI --mode qb --qb OS
```

The residue and observed groups are each unordered. For the example above,
Clearra compiles the supply as:

```text
[TI]![OS]![^OS]!P2
```

The observed group is a unique subset of one standard seven-bag. Its pieces are
available to the actual setup after the residue and every observed piece must
be placed into a returned setup; an observed piece left in hold does not
qualify. Clearra infers the unobserved complement of that bag. Residue plus
observed pieces may contain at most seven pieces, matching the practical
hold/active/preview observation window.

QB changes only the known supply prefix. It uses the same inverse lock-clear
family quotient and partial BuildUp search as residue mode. Nine-piece setup
candidates are never searched or reported in either mode; internal depth-nine
and terminal states exist only as PC-completion evidence.

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

At most one piece kind may occur twice. That duplicate is an explicit initial
hold piece. When no duplicate is present, empty hold and each possible occupied
hold are separate result conditions. They are never assigned an invented shared
probability.

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
by exact union of their residual families. Placement reachability, SRS+ kick
rules, line-clear adjustment, and concrete realization identity are checked
before an edge enters the graph.

The root is not a setup. Every PC-live prefix from one through eight placements
is eligible. Placement depth is output metadata, not a pruning proof. Visible shapes
merge only after the exact product coverage described below has been computed.
Depth-nine and terminal nodes may exist internally only to prove that an
eligible prefix can complete the PC; they are never registered, covered, or
reported as setup candidates.

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

The report separates hold conditions and includes the inferred cycle, canonical
residue, cycle-reset policy, geometry-family count, partial-graph node count,
candidate completeness, and exact representative placement/hold paths.

Output limits are applied only after all shape coverage has been accumulated.
Allocation failure, cancellation, or incomplete source materialization must not
produce a complete-looking setup result.
