# Compiled PC4 pattern input: bounded content binding

This is an input-ownership checkpoint, not graph completeness, an enabled
profile, or release acceptance. Production pattern reduction remains closed
until the observation/graph family consumes this exact source and preserves
the original Core result semantics. No dataset revision or SHA256 is pinned
by this change; the new hash identifies a user's compiled input, not HF data.

## Source findings

`Pc4HiddenQueueSource::Pattern` identifies input provenance. Its bag/reveal
scope is not a compiled arbitrary pattern expression. Also,
`SearchProblemId` does not include the concrete queue constraints: the real
compiled `II;IO` and `II;IT` fixtures have the same problem ID. Numeric
PieceSource/PatternUniverse/WeightModel IDs are not content proofs either.

The existing compiler owns a `MaterializedPatternUniverse` which may be
explicit, factorized, or lazily ranked standard-seven-bag storage. Queue
prefix projection can preserve several original ordinals with equal visible
sequences. Deduplicating those ordinals or counting successful ones alone
would change the probability denominator.

## Implemented boundary

`Pc4CompiledPatternPreparation::begin` accepts an existing immutable
`Arc<SearchProblem>`, not caller-provided sequences, weights, or claimed IDs.
It rejects unsupported source kinds, truncated universes, count mismatches,
and out-of-budget count/sequence sizes before lazy queue reads. This stage
supports compiled PatternExpression and Standard7Bag input for 10-wide PC
boards of 1..4 lines. Fixed and hidden-observation contracts stay separate;
ordinary offline PC support of 1..6 lines is unchanged.

Each bounded `advance` hashes original normalized pattern syntax, effective
source length/lookahead, ordered concrete queues, their original f64 weight
bits, and the complete count. No bag distribution is inferred and no weights
are converted into invented rational probabilities. A page failure commits
no partial digest/cursor. Smaller page limits can be retried; cancellation or
invalid source terminates the preparation and forbids a later seal.

Only exhaustive preparation produces `Pc4CompiledPatternSource`. The source
retains the same Arc and reads a single queue by ordinal on demand. It does
not clone the original universe, create a second Cartesian expansion, test
PC success, prune a zero-hit queue, or multiply weights by hold choices.
It is deliberately unable to mint candidate-completeness evidence.

Preparation has O(total compiled queue pieces) hashing cost with bounded
cooperative slices; it is not an O(1) pattern lookup or the planned graph x
hold x bag x preview DP. Large factorized-source performance must include
this cost. A later structural shortcut needs an equally strong binding to
the immutable compiler-owned representation, not a switch back to coarse IDs.

## Tests and evidence

The focused tests cover:

- Equal coarse problem IDs but different constraints and content identities.
- Original prefix-duplicate ordinals and exact probability weight bits.
- P7: all 5,040 queues/weights compared to Core, with bounded preparation
  pages, unchanged retained source capacity, and shared owner identity.
- Page-size-independent digest, no early completion, and cancellation after
  staged records preventing both cursor commit and later resurrection.
- Truncated source, excessive count/sequence budgets, fixed-source relabeling,
  and weight changes under identical caller-supplied numeric IDs.

On `a468b32982938485fda94acecc40065bae121f74`, non-publishing run
[34745754165](https://github.com/daejunnom/Clearra/actions/runs/34745754165)
passed Core 7+5, Tablebase 163, Replay 20, Postprocess 41 and 82/83 PC4 App
tests. The new P7 test incorrectly treated
`lazy_sequence_storage_retained_bytes().is_some()` as a general laziness
predicate. That method only reports the observed-seven-bag variant, so
factorized storage legitimately returns None. `94d044e` changes the test to
assert `FactorizedQueueExpression { sequence_len: 7 }` directly and verify
that the source's complete retained capacity stays unchanged. It does not
change product code or weaken the queue/weight comparison. The corrected
test execution is recorded below.

On `94d044ed2e82d22b998b8c6d4c4429b54117d3c0`, non-publishing run
[34745966391](https://github.com/daejunnom/Clearra/actions/runs/34745966391)
completed all four jobs successfully: source, native-cli, pc4-contracts and
surface-contracts. The PC4 job passed Core 7+5, Tablebase 163, Replay 20,
Postprocess 41, PC4 App **83** (9.21 s) and App replay **16** (46.62 s).
This includes all eight new input tests and the existing 700 paired
Range/hold/product cases. These durations describe fixture groups, not
production pattern preparation or PC search benchmarks. The P7 ordinal test
also confirmed bounded page-size-independent preparation, identity sharing
with the original owner, and unchanged source capacity after all reads.

## Remaining integration

Bind the resulting source to the qualified target, original board/hold,
observation policy and candidate request identity. Feed its queues or an
equivalent proven structural DP into the existing shared Range/cache owner;
retain zero-hit outcomes and the original denominator in the ordinary Core
reducer. Reject source swaps between different constraints, weights or
effective windows. Only that completed, tested path may replace the current
`UnsupportedRequestSource` guard. Actual profile qualification and public
transport/fallback/Discord Cloud Run bypass remain separate gates.
