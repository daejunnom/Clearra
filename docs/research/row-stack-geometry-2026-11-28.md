# Row-stack geometry and exact-search complexity

Date: 2026-11-28.

This note separates the guarantees of the new geometry generator from physical
verification, supply coverage, and observed execution time. It also records two
local simplifications whose exactness does not depend on the new generator.
The row-stack integration described here is an opt-in local experiment, not a
claim of deployment or a measured speed improvement.

## 1. Input model and the meaning of a clear

Let `n` be the number of tetrominoes placed for this target, `W` the board width,
and `H` its height in the original, uncleared coordinate frame. The asymptotic
comparison uses fixed `W = 10` and an input family with `H = O(n)`. It is a
parameter analysis of the algorithm, not a claim that the current fixed-size
carriers support unbounded boards.

The documented P7P4 fixture is an empty **10-column, 4-row** field: ten placed
tetrominoes and an eleven-piece source window. Ten columns, ten placed pieces,
and ten rows are different restrictions. Its original queue universe is
`7! * 7 * 6 * 5 * 4`, independently of how geometry is generated. The source and
Hold meaning are described in
[the placement-graph note](placement-graph-p7p4-2026-11-20.md).

In the original frame, a row can be deleted at most once. This permits many
locks to clear different rows, including intermediate skims. It does not mean
that the whole execution has exactly one line-clear event. For a fixed exact
candidate, the existing model is

```text
Clear(r, E) = all selected contributors to completed original row r are in E
D(E) = the original rows satisfying Clear(r, E)
B(E) = Compact(initial cells UNION cells of E, D(E))
```

Several rows completed by one lock clear atomically. A partially filled final
row is not a clear event. See
[placement_conditions.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_conditions.rs),
[buildup.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/buildup.rs), and
[extended_board.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/extended_board.rs).

An actual restriction to one final clear event and no earlier clear is much
stronger. A standard tetromino can complete at most four rows in that event.
If it leaves the board empty, then

```text
initial occupied cells + 4n = W * cleared rows, with cleared rows <= 4
```

so `n <= W`. This cannot silently replace the general PC contract. Likewise, a
fixed 10-by-10 original target frame contains at most 25 placed tetrominoes;
that finite bound differs from gameplay that repeatedly reuses physical rows.

If initial-board size, `H`, source-description size, or explicit queue length
can grow independently of `n`, a bound in `n` alone omits input work. The
following bounds state where those parameters enter.

## 2. Baseline costs and what the local changes remove

A standard tetromino occupies at most four local rows. For a fixed kind,
rotation, and x offset, monotone inverse row projection chooses at most four
original rows. The raw realization count therefore satisfies

```text
R = O(W H^4) = O(n^4), for fixed W and H = O(n).
```

This is a record-count bound. Catalog sorting, grouping, and associated index
construction give an `O(R log R)` upper bound under the current constant-size
record model. An unbounded-height implementation can retain that model with
four-cell coordinate keys on a word RAM. If each record instead acquires a
growing dense bitmask, comparisons and copies must include its word count.
The present representation cannot simply be treated as a constant-size
unbounded board.

Let `S <= R` be the number of distinct skeletons and `B = WH` the number of
target-frame cells. Incidence belongs to skeletons, not to every realization
of a skeleton.

The catalog still sorts realizations and other derived data. The changes in
[extended_inverse_catalog.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/extended_inverse_catalog.rs)
remove narrower redundant work:

- Skeleton IDs are visited in increasing order, and a bitset iterator visits
  each of a skeleton's four cells once. Each per-cell support list is already
  strictly increasing. Removing its sort and dedup preserves both contents and
  order. Support assembly is `O(S + B)` with exactly four incidences per
  skeleton. This was already the relevant extended-catalog incidence design;
  a Board64 full-catalog-per-cell scan is not the baseline for large inputs.
- A required deleted-row interval `[a, b)` is the bits below `b` excluding the
  bits below `a`. The new helper handles empty intervals and the exclusive
  endpoint 32 without shifting a `u32` by 32. It replaces the per-gap row loop;
  there are at most three gaps per tetromino.

These changes do not make the whole catalog linear in `n`. Explicitly emitting
`R` records already requires `Omega(R)` work. Removing redundant sorting also
does not imply a measured logarithmic speedup: a sorting implementation can
already recognize ordered input.

A conservative upper bound for explicitly enumerating all raw exact-cover
candidates is `R^n * poly(n) = 2^O(n log n)`. This is not a tight bound proved
for every current execution path.
[geometry.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry.rs)
already memoizes residual cells and piece counts, compiles a shared family,
and can skip physically impossible prefix subtrees. Its
[prefix dependency closure](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_prefix_pruning.rs)
can reject crossed-gap cycles before a complete candidate is visited. A
comparison between that upper bound and the new generator's bound must not be
presented as a proven tight lower bound on the old implementation.

### Count per-domain work separately from visited search states

Let `P` be the number of residual states at which a domain is built, and `G`
the number of target seven-kind count vectors. For the current extended domain
implementation, a conservative per-state bound is

```text
O(S + B^2 + G*n)
```

with fixed width, seven kinds, the existing constant-size board carrier, and
the fixed projection-work budget. Summed skeleton support scans use at most
four incidences per skeleton. The disjoint SameTile groups choose different
first cells, so their parent-support scans also sum to at most `4S`; treating
every cell as a fresh scan over the entire catalog overcounts this part.
Remaining-cell scans repeated for those groups provide the `B^2` term.
Target feasibility work is conservatively bounded by `G*n`. Thus the residual
domain contribution is `O(P * (S+B^2+G*n))`, not this polynomial once for the
whole search. APDP index construction is a separate `O(S log S)` safe upper
bound. The changed APDP parent recheck adds no new selective power and its
removal does not by itself change this worst-case domain bound.

Using `B=O(n)`, `S=O(n^4)`, and
`G <= binomial(n+6,6)=O(n^6)` yields the loose `O(n^7)` per-domain bound.
For a small fixed target set, that substitution would exaggerate the target
term. A generalized dense `d`-word board adds its word cost to skeleton tests;
removing the fixed projection-work budget would also require a separate
projection-convolution term. These are upper bounds for the stated loops,
not tight bounds or measured costs for every domain invocation.

## 3. APDP parent rechecking is an identity in the changed branch

For the current residual field and feasible piece-kind mask, define

```text
V(q) = { full catalog parents p : q subset cells(p) subset remaining,
                                kind(p) is currently feasible }
A(q) = { parents supported by the static APDP index for q }
```

SameTile has already computed the exact full-parent domain for the chosen
three-cell group `q`, either by a scan or by a certified exact support summary.
The historical APDP recheck enters only after `apdp_domain_is_complete` has
visited every member of `V(q)` and established both static exactness and
membership in `A(q)`. Thus the branch establishes `V(q) subset A(q)` and then
rechecks `V(q) intersect A(q)`, which is exactly `V(q)`.

In the Board64 path, recounting cannot reduce the already exact parent count
or kind mask. In the extended path, assigning `apdp_required_cells = q` adds a
later predicate already true for every surviving parent. Empty or incomplete
domains do not enter that branch. In particular, an inverse-clear parent that
lacks static APDP support makes the completeness check fail; it must remain a
valid alternative instead of being replaced by a contiguous same-kind parent.

The default now skips this parent recheck in
[geometry_domain.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_domain.rs)
and
[extended_geometry_domain.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/extended_geometry_domain.rs).
The `local-search-ab` reference mode is
`CLEARRA_APDP_PARENT_RECHECK=1`. This is a proof about the full-parent recheck
after SameTile, not removal of the APDP index, a new rule for every APDP
consumer, or a reduction in the worst-case polynomial degree of all domain
propagation. Bumper, Hall, and projection predicates keep their own meaning.

The inspected `codex/v091-placement-dependency`,
`codex/combinatorial-dependency-proof-20261122`,
`codex/fixed-tiling-proofs-20261122`, and
`codex/dependency-primary-path-20261122` histories still contained this
redundancy in the extended domain/APDP modules. The earlier
`f1c57e4` change made PC placement dependency analysis exclusive of BuildUp;
it was not this parent-recheck removal. This records the branches actually
inspected, not a claim that no other historical branch or commit exists.

The differential tests compare the old and new domain modes against scalar
full-parent membership, including static and inverse-clear alternatives,
multiple same-kind parents, exhausted kinds, empty residuals, changing
workspaces, and cells across a `u64` boundary. They compare allowed row IDs and
pivot information, not only aggregate solution counts. The catalog tests also
compare all interval endpoints from 0 through 32 with the original scalar
loop, and compare support lists and emitted realization masks on 8- and
24-row fixtures. Those bounded checks do not constitute an all-input product
or performance qualification.

## 4. Why a row stack is a complete necessary geometry filter

[row_stack_geometry.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/row_stack_geometry.rs)
joins consecutive occupied original rows of each placement by arcs. Standard
tetromino shapes have consecutive occupied local rows. If an arc spans `(a,b)`,
every original row strictly between its endpoints must have been deleted
before that placement executes.

Suppose two placements have strictly crossing arcs `a < c < b < d`. The first
placement requires row `c` deleted, so the placement contributing at `c` must
execute first. The second requires row `b` deleted, so the first placement must
execute first. This is a cycle. A buildable exact candidate therefore has no
strictly crossing pair. Shared starts, shared ends, touching endpoints, and
nested intervals are permitted by this necessary condition.

The generator scans original rows from bottom to top. At each row it:

1. Closes a suffix of the open-arc stack and places those pieces' next local
   row fragments.
2. Covers the remaining target cells with new pieces' first row fragments.
   The leftmost uncovered cell fixes the next fragment's x offset.
3. Pushes outgoing arcs in each admitted order and advances to the next row.

Closures happen before openings at the same row. For any noncrossing candidate,
arcs opened at the same row can be pushed in decreasing endpoint order, with a
fixed left-to-right source-cell tie order for equal endpoints. Arcs ending at
the current row then form a stack suffix. This constructs a valid stack history
for every such candidate. Conversely, LIFO closure cannot create a strict
crossing. The tie rule removes duplicate histories for parallel arcs without
merging distinct placements.

The endpoint-domain check is only a necessary local condition: it asks where a
piece's later fragments could fit with other pieces absent. It does not commit
to a physical execution or a line-clear history. Completed pieces are mapped
back to the existing catalog's concrete row IDs. Their complete realization
alternatives remain owned by that catalog.

Two implementation choices bound avoidable work without adding a global memo.
The native partitions claim a canonical first-row prefix after at most two
new piece starts, or when a shorter first row is full. Nonowners backtrack
before constructing the rest of that row or its outgoing permutations. All
later choices of the owned prefix stay with that partition. A reversible
column budget also reserves all four cells of a newly started shape, including
its future row fragments. Row deletion never changes x, so exceeding a target
column's remaining cell count is a sound immediate rejection. The reservation
is restored only when that piece start is removed, not when its next fragment
advances. Neither change proves physical feasibility.

### Noncrossing does not imply even deletion-dependency feasibility

Consider two disjoint J pieces in the registry's `Right` rotation, projected
into a 10-by-5 original frame:

```text
P cells = {(0,0), (0,2), (0,3), (1,3)}
Q cells = {(3,1), (3,2), (3,4), (4,4)}
```

Fill every other cell with initial blocks. Each profile is the projection of
local row occupancies `(1,1,2)`. P occupies rows `{0,2,3}` and Q occupies
`{1,2,4}`. Their arcs are `(0,2)`, `(2,3)`, `(1,2)`, and `(2,4)`; none strictly
cross. However, P needs original row 1 deleted, which requires Q first. Q needs
original row 3 deleted, which requires P first. Neither can execute first.
Shared endpoints allow this cycle despite the stack embedding.

The generator is consequently a geometry filter. Grounding, reachability,
ordered kicks, atomic clears, B2B/scoring conditions, source/Hold legality,
observation policy, and queue probability retain their exact downstream owners.

## 5. Branching, time, and retained state

At a fixed row there are at most `W+1` choices for how many open arcs close.
There are 19 distinct normalized standard tetromino shapes. At most `W` pieces
start, and each shape choice has a uniquely determined x offset. The outgoing
pieces occupy disjoint current-row cells, so at most `W` arcs are pushed, in at
most `W!` orders. A sufficient bound on completed row decisions is

```text
C_W = (W + 1) * 19^W * W!
T_generator = C_W^H * poly(n, H, G, log R)
```

Here `G` is the number of target piece-count vectors. Distinct seven-kind
vectors summing to `n` number at most `binomial(n+6, 6)`. The polynomial factor
includes scanning waiting pieces, finding future row domains, testing targets,
catalog lookup, and sorting an emitted candidate's row IDs. Fixed `W` and
`H = O(n)` give `2^O(n)` time for this generator. The constant is deliberately
loose and supplies no prediction of practical speed.

Its private reversible DFS stores the current row path and partial pieces:
`O(H+n+G)` words for fixed width, excluding the borrowed catalog and downstream
owners. It avoids constructing the old shared exact-cover family first. It
does not make the whole search polynomial-space: catalog storage, physical
subset states, language caches, source coverage, and retained results remain.

The current next-row domains fit in a `u32` under the 24-row implementation
limit. An unbounded-height port cannot count an arbitrarily long domain bitmap
as one word. It can preserve the stated linear DFS storage by finding the
latest compatible endpoint on demand from row fragments, at an additional
`O(H)` query cost already covered by the polynomial time factor. Retaining
full domain bitmaps at every frame instead requires their extra storage to be
counted explicitly.

The same word-model condition applies to a partial piece's cells. Keeping four
cell coordinates preserves constant-size piece records in an unbounded-height
port; expanding every `PartialPiece.cells` into a dense `d`-word board instead
costs `O(nd)` words. The current four-word carrier is bounded and should not be
silently generalized as a constant-size infinite board.

For a fixed candidate, the existing exact physical relation has at most `2^n`
executed-operation subsets and `n` possible next operations per subset. With a
fixed movement profile, the pose graph has `O(WH)` states and bounded outgoing
move/kick choices. A conservative `O(n^3 2^n)` bound includes ordinary physical
projection and row processing when `H = O(n)`. This describes the physical
relation layer, not every optional evidence or output product. Multiplying it
by the new generator's bound gives `2^O(n)` for a design that actually builds
that relation once per candidate. It does not prove the same execution bound
for the current entire product pipeline.

The currently qualified PC route instead compiles
`SymbolicPrefixOrders::compile_product_completion_with_source`, constructs a
piece language, and intersects that language with the source. Its deleted-row
frames and operation-subset words add axes beyond the ordinary physical
relation. In particular, the present `frames_by_word` storage and frame scans
are bounded by fixed implementation constants; an unbounded-height port must
account for those dimensions. Moreover,
[symbolic_prefix_lanes.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/symbolic_prefix_lanes.rs)
gathers and scatters each 64-lane batch by scanning the full subset-word
array. If that fixed array is generalized to `M=ceil(2^n/64)` words, this
reindexing alone admits an `O(n * 2^n * M)` bound. The ordinary `O(n^3 2^n)`
relation bound above is therefore not an exact bound for that whole
implementation. A conservative envelope for its subset/frame and packed-lane
dimensions is `poly(n,H) * 2^O(n+H)` before separately counted source/language
and output work. That envelope is still single-exponential under `H=O(n)`, but
does not assert the particular base or polynomial factor of the ordinary
relation algorithm.

Supply must be counted separately. The
[coverage product](../../crates/clearra-core-executor/src/backend/wasm_cpu/coverage_product.rs)
has eight held-piece states and two extra-draw states, but its source universe
and language size are not constant. With seven kinds and queue length `O(n)`,
the number of relevant queues is at most single-exponential. This count does
not bound the work of an implementation that revisits the same language/source
product state. In particular,
[standard_bag_coverage.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/standard_bag_coverage.rs)
continues exact recursive computation when an optional memo insertion is not
admitted. A whole-pipeline `2^O(n)` runtime claim therefore needs a separate
revisit/cache-admission proof. The generator's own `2^O(n)` bound and the
availability of a single-exponential Boolean DP design do not provide that
proof for every current execution policy.
Exact probability is a weighted union of successful original queues, not a
sum of successful physical witnesses. CountAll, observed-information policies,
and materialized replay evidence have different contracts. Explicitly emitting
`Z` records requires at least `Omega(Z)` work; a scalar probability has no such
lower bound in the number of underlying queues merely because those queues
exist.

### An explicit exponential family of geometry outputs

For width 10 and height `4k`, split every four-row band into two 4-by-4 blocks
and one 2-by-4 block. Tile each 4-by-4 block with either four horizontal I
pieces or four vertical I pieces, independently. Tile the 2-by-4 block with two
O pieces. Every band has the same counts, eight I and two O, and four choices.
Across `k` bands there are `n=10k` pieces and `4^k=2^(n/5)` distinct sets of
four-cell placements. All pieces occupy consecutive original rows, so all
their arcs are noncrossing.

Thus an algorithm explicitly emitting these individual geometry candidates
has exponential output work on this family, even with width fixed and the
piece-count vector fixed. The claim is about placement sets, not piece-colored
boards: horizontal and vertical I tilings have the same cell colors. It is
also not a proof about the number of physically executable solutions for one
particular queue, nor a lower bound for Boolean PC decision or scalar
probability. Compression or a decision algorithm may avoid enumerating these
outputs.

## 6. Current integration and validation scope

The generator supports catalogs up to width 10 and height 24, subject to the
existing board carrier. The current
[local adapter](../../crates/clearra-core-executor/src/backend/wasm_cpu/geometry_row_stack.rs)
is compiled with `local-search-ab` and requested by
`CLEARRA_ROW_STACK_GEOMETRY=1`. Its activation is further qualified by
`physical_pc_prefix_mode` in
[result.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/result.rs).

That port is limited to an internal, untouched Board64 geometry search for the
qualified clear-to-empty PC route, unique counting, at most eleven placed
pieces, and the admitted output policies. It excludes the separately capped or
externally partitioned routes and options requiring unsupported observation,
identity filtering, per-solution probability, or additional evidence. The
extended catalog trait implementation and its unit fixture do not by themselves
establish product integration for every extended-board entry point. Existing
physical verification, supply reduction, and worker ownership remain in place.

The generator's unit oracles enumerate small exact-cover families independently
and then filter only strict crossings. They compare concrete candidate keys,
including nested/shared endpoints, base cells, resumable work quanta, root
partitions, cancellation/node debit, and extended-word mapping. A separate
equal-deadline fixture checks that canonical endpoint ordering avoids exploring
factorially many equivalent first-row orders. These are geometry and state
ownership checks, not a substitute for end-to-end physical/source parity.

## 7. When exponential decision can become polynomial or linear

A fixed candidate can avoid subset search if a sufficient certificate proves
that its legal operation orders are exactly the linear extensions of a
precedence DAG. With unrestricted supply, the remaining existence test is
acyclicity plus terminal acceptance: `O(n+e)` after certification.

[placement_poset.rs](../../crates/clearra-core-executor/src/backend/wasm_cpu/placement_poset.rs)
requires both directions: precedences necessary for every execution, and an
exact ILC/grounding/movement witness for every allowed ideal transition. A
failed certificate returns fallback, not infeasibility. Its present compiler
enumerates deleted-row masks with `0..(1 << height)` under a six-row limit.
Generalizing height therefore does not give a polynomial certificate compiler.
The honest total bound is `T_certificate + O(n+e)`; dense `e` can itself be
quadratic.

Other sufficient restricted cases are precise but narrower than the product:

- Exact positive AND/OR enable conditions, free supply, and no execution that
  disables another enabled transition admit a worklist closure linear in the
  represented condition incidences. OR alone is not a hardness argument.
- A certified single operation chain and an explicit queue with one-slot Hold
  admit Boolean feasibility in `O(n+L)`: progress fixes the draw position up to
  the bounded Hold offset. This does not count distinct executions or output
  every witness.
- A certified poset with a supplied fixed-size chain cover has at most
  `product(length_i+1)` progress states. A fixed number of chains yields a
  polynomial state space, with supply/Hold state included rather than assumed
  away.

General ordered kicks have negative conditions: an earlier kick must fail for
a later kick to be selected. Initial support alternatives, collisions, and
source labels can invalidate simple precedence reasoning. For example, with
`a1:I -> b:J` and independent `a2:I`, the queue `IJI` without Hold succeeds when
`a1` is chosen first and fails when `a2` is chosen first. An arbitrary topological
order is not a queue-feasibility algorithm. No NP-hardness claim for Clearra's
particular fixed-width rules follows from these examples.

### Unimplemented proposal: share laminar deletion-interval AND nodes

The noncrossing generator supplies nested or disjoint gap intervals. A possible
next representation has one deletion event per original row, an AND of that
row's concrete contributors, and shared AND nodes for laminar gap intervals.
Each placement requires the interval nodes for its gaps. It need not expand
every interval into all pairwise placement precedences.

There are at most three gap intervals per placement and four placement-to-row
incidences per placement. If the generator emits a sorted laminar nesting,
equal intervals are shared, and row leaves are assigned to their innermost
containing interval, an `O(n+H)` size and construction bound is plausible for
this necessary-condition graph. Arbitrary unsorted interval input would need
its ordering cost counted. Empty intervals, shared endpoints, equal intervals,
initial/precleared rows, and simultaneous deletion events require an exact
construction proof before this is an implementation claim.

The graph must still detect cycles, including the two-piece example above.
Even an acyclic graph only certifies the encoded mandatory deletion
prerequisites. It supplies no positive proof of grounding, ordered-kick
reachability, source/Hold legality, or queue probability. This design is not
implemented by the row-stack generator.

## 8. Benchmark definition and interpretation

The finite P7P4 fixture provides an end-to-end comparison with a known source
contract and ten placed pieces. Compare separate processes from the same
optimized binary and source revision, holding the input, kick/spin/B2B profile,
Hold and observation policy, output policy, worker count, and resource boundary
fixed. Process-level mode caches make changing environment variables inside
one running process an invalid A/B procedure.

Separate the catalog/domain changes from the generator change. The APDP
reference switch isolates its redundant parent recheck; the row-stack switch
isolates geometry generation while retaining the ordinary verifier. A baseline
from another binary or revision is descriptive rather than an isolated causal
comparison.

Correctness comparison uses completed normalized solutions and original-queue
coverage under the same source semantics. Raw candidate order, raw candidate
count, and raw candidate-stream digests can differ because the generator omits
geometrically unbuildable families. Compare total elapsed time and stage work,
including catalog creation, root preparation/partitioning, all worker work,
physical reachability, and source reduction. Repeated root preparation must
not be hidden by reporting only a worker's descendant traversal. Record peak
retained memory separately from elapsed time and distinguish complete results
from time/node/resource-limited runs. An interrupted run provides a lower
bound on its completion time, not a completed timing or an exact-result parity
claim.

A smaller asymptotic branching bound can still lose on this finite fixture
through constants, rejected partial states, root-order preparation, or reduced
reuse. Measured outcomes therefore determine whether the local adapter is a
useful execution policy; the bound alone does not establish a speedup.

## 9. Executed results and adoption decision

### Environment and exact input

Runs used Linux, Rust/Cargo 1.98.1, a release build with
`parallel,local-search-ab,wasm-stage-profiling`, nine available logical CPUs,
and the supervised `benchmark-search` process profile. Only one memory-heavy
run was active at a time. This is not a measurement of the Discord service's
eight-vCPU machine.

The unchanged P7P4 input was an empty 10-by-4 board, ten placed pieces,
eleven supplied pieces, Jstris 180 movement, empty Hold enabled, unique
solutions, full-future-oracle queue semantics, and Summary output. The
original source universe contains 4,233,600 queues. Standard-bag union memo
storage was `compact`; other existing product pruning/saturation settings
were held at their defaults. Neither the input nor the output contract was
reduced to make the new generator finish.

### Complete and censored end-to-end runs

| Revision / route | Search elapsed | Outcome | Supervisor sampled peak RSS |
| --- | ---: | --- | ---: |
| Untouched `8ac5cc5`, existing generator | 14.224784 s | Complete, validated | 2,994,073,600 bytes |
| `acaf2e3`, existing generator | 14.563137 s | Complete, validated | Not used for the matched final comparison |
| `acaf2e3`, initial row stack | 600 s limit | Timeout; no completed result | 15,310,848 bytes |
| `c361bb1`, existing generator | 15.822273 s | Complete, validated | 2,767,290,368 bytes |
| `c361bb1`, early partition + column reservation row stack | 600 s limit | Timeout; no completed result | 1,617,547,264 bytes |

The last two rows used the same binary, SHA-256
`d5980fd8dab80d15621e5a8449a6fc834297a5a52df9cdfbb1be8d096010794d`.
The final row-stack process was stopped after 600.126 s of supervised
process lifetime, returned 143, and left no descendant processes. It produced
no complete benchmark JSON. A timeout is not a zero-solution result or a
successful correctness comparison. Its lower observed RSS is also not a
completed-search memory improvement.

All three complete unprofiled P7P4 reference runs agreed on:

- 29,856,840 raw geometry candidates;
- 28,161,783 candidates rejected by shared physical prefix certificates;
- 1,695,057 candidates reusing the surviving ILC prefix certificates;
- 456,459 normalized unique solutions;
- 4,233,600 covered original queues, hence 100% global coverage;
- full ordered solution-identity SHA-256
  `08c477a2f71455a72778f076f9942789a9997ab3b3008fc2a3d043f02458f062`;
- coverage-bitmap SHA-256
  `0d7794467640c727b89ccd5f170fc4430228e0aba6fa9d7f8657b0e653ef3912`.

The fixture's catalog contains 764 skeletons and 1,132 realizations. Its
existing empty-origin route built 154,851 minimum domains and **zero advanced
APDP parent-recheck domains**. Consequently these timings cannot establish a
speed benefit from removing that APDP recheck. The bounded nonempty P7 and
eight-I reference/skip comparisons also completed with equal exact result
hashes, but their APDP static-check count was zero. The direct domain unit
oracles, including a complete static-parent domain, provide the pertinent
equality check for the removed branch.

These are finite individual runs, not a repeated-run distribution or a p95
estimate. Their roughly 14–16 s reference variation must not be attributed to
the APDP change. The 600 s non-completions are sufficient to reject adopting
this prototype as the current P7P4 execution policy.

### Stage diagnostic: the new generator is the observed bottleneck

The earlier profiler captured its worker token from inside the worker pool,
where request TLS was absent. That omitted worker stages. The fix captures
the request on the submitting thread and clones its token into jobs; a real
pool regression test verifies collection and scope restoration. Untouched
baseline worker-stage output is therefore not used for this comparison.

Revision `a8f6551` adds only a finite cancellation diagnostic to the benchmark
harness, with no further solver changes after `c361bb1`. Both profiles below
used its release binary, SHA-256
`31ce5c1e292b113f8fc612a432557a8771a9b687a83ce99384fa6cdd9ba1a2aa`.
The reference completed in 14.255634 s. The row stack was cooperatively
cancelled after 20.013693 s and returned a separate diagnostic schema. The
cancelled report is not accepted as a complete result or timing-adoption
sample.

| Stage | Existing route, complete | Row stack, first 20 s only | Time meaning |
| --- | ---: | ---: | --- |
| Catalog construction | 1.357 ms | 1.088 ms | Caller elapsed span |
| Supply construction | 2.828 ms | 6.095 ms | Caller elapsed span |
| Geometry owner preparation | 3.160 ms | 5.977 ms | Caller elapsed span |
| Parallel plan / family preparation | 725.960 ms | 0.086 ms | Caller elapsed span |
| Geometry traversal | 28.703 s | 185.276 s | Sample-expanded worker accumulated estimate |
| PC dependency verification, inclusive | 47.352 s | 1.902 s | Sample-expanded worker accumulated estimate |
| Piece-language construction | 10.559 s | 0.111 s | Nested within dependency verification |
| Source/coverage language product | 13.748 s | 1.071 s | Nested within dependency verification |
| Result merge | 62.650 ms | Not completed | Caller elapsed span |
| Final solution canonicalization | 633.873 ms | Not completed | Caller elapsed span |

Worker values are accumulated elapsed spans, not CPU-time measurements. Hot
spans are sampled and expanded; the estimate may exceed the actual aggregate
worker wall time. Spans nest and overlap across workers. Do not sum the table
as end-to-end elapsed time, or compare a complete workload with a cancelled
prefix as if they had processed the same candidates. In the sampled prefix,
geometry dominates the new route's observed work; no claim about its complete
downstream workload is possible because it did not finish.

The source explains an additional downstream risk: the adapter resets the
old physical-prefix owner and emits candidates without its ILC dependency
certificate. The ordinary verifier remains exact, but now repeats candidate
ILC checks and no longer shares the old family's prefix rejection work.
This loss is real; the diagnostic indicates that the new geometry traversal,
rather than that downstream cost, is the dominant problem in the observed
20-second interval. No infinite cursor loop or failed parallel handoff was
found in the inspected paths.

At dispatch the 36 row-stack partitions together retained **116,424 bytes**
of private generator backing. That is not total engine memory. The cancelled
20-second profile observed 425,664,512 bytes of RSS; the complete reference
profile observed 3,049,029,632 bytes. At the reference's peak observation,
last-published logical worker standard-bag capacities summed to
2,655,710,696 bytes, versus 6,030,752 bytes for the dispatch shared geometry
family. These are different, potentially overlapping logical/accounting
quantities, not an additive allocator attribution. They show why making only
the generator's DFS small does not establish linear-space whole-product
execution.

### Completed smaller parity and build checks

The final binary also completed a paired empty-4L, ten-I fixture under nine
workers. Both routes produced exactly 14 full solution identities and the
same single covered queue. The Rust comparator returned `equivalent: true`,
with raw-candidate comparison explicitly disabled across the different
candidate domains. All 36 row-stack partitions completed, emitting 14
candidates after 47,862 generator actions, with 56,376 private dispatch bytes.
Observed search times were 8.306 ms and 8.568 ms, respectively; single
millisecond-scale runs do not establish a speed difference.

Executed focused tests:

| Scope | Passed |
| --- | ---: |
| Row-stack geometry and independent small exact-cover oracle | 8 |
| APDP parent-domain differential checks | 4 |
| Extended catalog interval/support parity | 3 |
| Stage profiler, including actual worker-pool propagation | 6 |
| Benchmark admission, comparison, cancellation and evidence helpers | 39 |
| Total focused unit tests | 60 |

`cargo check --locked -p clearra-core-executor --lib --features parallel`
also passed without `local-search-ab`. Formatting and `git diff --check`
passed. This is not a full CLI/GUI/Discord option-matrix or release
qualification. The generator remains opt-in behind `local-search-ab`; normal
CLI, desktop, and Discord release feature declarations do not enable it.
APDP skip, catalog simplification, and profiler corrections are committed on
this research branch, not asserted to be deployed.

### Reproduction and retained evidence

Reviewed source changes are on
`codex/row-stack-geometry-apdp-20261128`. The implementation revisions are
`acaf2e3`, `c361bb1`, and `a8f6551`. Raw generated evidence stays under
`build/research/row-stack-20261128/`; this document records selected results
for version control. Relevant outputs are:

- `baseline-unprofiled.json`, `reference-run1.json`,
  `reference-final-run1.json`, and their stderr/receipt references;
- `row-stack-run1.stderr` and `row-stack-final-run1.stderr` for the two
  supervised timeouts; their JSON destinations are empty, not result reports;
- `reference-diagnostic-binary-profile.json` and
  `row-stack-cancel-profile.json` for the matched diagnostic binary;
- `ten-i-final-mode-0.json`, `ten-i-final-mode-1.json`, and
  `ten-i-final-comparison.json` for completed final small parity;
- the five focused test logs and `production-feature-check.log`.

Build an optimized, source-bound harness before running experiments:

```sh
CLEARRA_SOURCE_COMMIT="$(git rev-parse HEAD)" cargo build --locked \
  -p clearra-core-executor --example placement_graph_bench --release \
  --features parallel,local-search-ab,wasm-stage-profiling
```

Use a fresh process per axis. For an unprofiled ordinary P7P4 run, set
`CLEARRA_STANDARD_BAG_UNION_MEMO=compact`,
`CLEARRA_APDP_PARENT_RECHECK=0`, and `CLEARRA_ROW_STACK_GEOMETRY=0` or `1`,
then run the example with `--summary` through the finite supervised
`benchmark-search` profile. To inspect a slow prefix, additionally set
`CLEARRA_SEARCH_STAGE_PROFILE=1`, `CLEARRA_SEARCH_MEMORY_PROFILE=1`, and
`CLEARRA_SEARCH_DIAGNOSTIC_CANCEL_MS=20000`. Keep the outer supervisor timeout
finite and longer than the cooperative deadline. This diagnostic does not
export or validate solutions, coverage, or parity even if the backend happens
to finish before the timer.

**Decision:** retain the row-stack implementation as an explicit research
prototype. Do not promote it to the default engine or deployment on the basis
of its asymptotic generator bound. The next algorithmic target is an exact,
shared representation of deletion conditions and a bounded-cost physical
certificate, with certification cost and queue semantics included in its
proof. The proposed laminar condition graph remains unimplemented and does
not yet establish polynomial-time general PC decision.
