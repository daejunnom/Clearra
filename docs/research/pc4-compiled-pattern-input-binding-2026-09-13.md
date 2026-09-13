# Compiled PC4 pattern input: content binding and shared product integration

This records an input-ownership checkpoint and its subsequent integration into
the existing observation/Range/Core pipeline. It does not qualify an actual
HF profile, activate a public transport, or replace release acceptance. No
dataset revision or SHA256 is pinned by this change; the new hash identifies
a user's compiled input, not HF data. The chronological CI evidence below
separates the input-only milestone from the later product integration.

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
bits, and the complete count. No bag distribution is inferred. A page failure
commits no partial digest/cursor. Smaller page limits can be retried;
cancellation or invalid source terminates preparation and forbids a later seal.

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

Native CLI separately passed **17** real process tests (106.33 s). Its build
still reported MSVC `LNK4098` (`MSVCRTD` default-library conflict); that warning
is not resolved by a pattern-input change and needs native archive/runtime
configuration review before claiming a warning-clean release. Hosted action
logs also contain the Node `punycode` deprecation warning. Neither a successful
workflow nor these fixture timings prove production deployment readiness.

## Actual pattern/Range/product connection

`ed40161` adds a nominal finite queue family backed by the same immutable
compiler owner. Its bounded cursor reads only requested ordinals. Two
independently constructed, same-size readers are not interchangeable, and
cancellation, reader failure or allocation limits commit no partial page.

`Pc4PreparedOnlineInput::for_compiled_pattern` binds this family to the target,
actual rule profile, original board and initial hold. Request identity v2
includes the observation contract: this finite integration admits only
`FullQueueOracle`; it explicitly rejects `VisibleSeven` rather than exposing
future pieces. Only uniform original Core weights (bit-equal to `1.0/N`) are
admitted. The reveal ledger records exact `1/N` per original ordinal while
Core still consumes the unchanged original f64 weights. Nonuniform inputs
are rejected, not silently reweighted.

The family runs through the existing observation frontier, shared Range/cache
owner, placement materializer and candidate-completeness boundary. Fixed,
hidden-bag and compiled-pattern scopes cannot borrow each other's authority.
The existing Core bridge rechecks the complete compiled input identity before
consuming a candidate union; it supports these two compiled source kinds
without removing the guard for other sources. Existing App all/chance/minimum/
replay/field-score/score-minimum reducers remain authoritative. Fixed-queue
highest score remains queue-only.

The new matrix has 390 paired cases: 1..4L x five profiles x three pattern/hold
supplies x six products (360), plus five profiles x six products for prefix
duplicates (30). The latter keeps `[IO][TZ]` as I,I,O,O after one-piece
projection, with four original ordinals and two zero-hit outcomes. The graph
is a synthetic-qualified I-only fixture: these are product/denominator tests,
not evidence of real HF graph completeness. The existing 700 fixed/hold pairs
remain unchanged.

The first execution [34747158157](https://github.com/daejunnom/Clearra/actions/runs/34747158157)
passed Core 7+5, Tablebase 167, Replay 20 and Postprocess 41, but PC4 App was
84/86. Both new tests incorrectly required score-minimum success when an
original ordinal had no PC. Its existing contract requires full-universe
winning coverage, and its independent contract test explicitly rejects this
case. `2efd8e6` preserves this rejection rather than deleting zero-hit mass:
the matrix now requires 365 equal successes and 25 equal typed rejections.

The next execution [34747485379](https://github.com/daejunnom/Clearra/actions/runs/34747485379)
exposed an actual shared completion bug: the distributed score-minimum handoff
discarded the typed Core error and returned generic `source_rejected` instead
of the ordinary App response. `f28e892` fixes the common handoff used by both
WASM distributed and PC4 execution. It releases the terminal owner, preserves
the existing finalized error, and returns it once through the cooperative
cursor without starting another solver or exposing a partial portfolio.

`9406405` separately corrects the Windows C archive's default CRT selection.
Debug C library and C test executables now use `MultiThreadedDLL`, matching
their Rust consumer. Explicit caller CRT settings and debug optimization/
assertion settings are unchanged; no linker warning is suppressed. This
follows [CMake's runtime selection contract](https://cmake.org/cmake/help/latest/variable/CMAKE_MSVC_RUNTIME_LIBRARY.html)
and [Rust's CRT linkage contract](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes).
On `9406405a6a0fdea3a1c198073236ab6047c0e58d`, native job `103698376735` in
[34747626267](https://github.com/daejunnom/Clearra/actions/runs/34747626267)
passed all 17 CLI process tests (71.49 s). Its complete 398-line log contains
no LNK4098/LNK2038 and no C/Rust compiler warning; hosted action `punycode`
deprecations remain. The overall run failed the already-known PC4 completion
error before `f28e892`, so this is native-fix evidence only, not a green run or
a performance comparison.

Combined correction SHA `f28e8926d49f1a5e45d7571f50fe1130ea6fd4d6` is submitted
to non-publishing [34747762465](https://github.com/daejunnom/Clearra/actions/runs/34747762465).
Its PC4 job passed Core 7+5, Tablebase 167, Replay 20, Postprocess 41,
PC4 App **86** (17.83 s) and App replay **16** (61.73 s). This includes all
390 new paired pattern cases (365 successes and 25 equal typed rejections)
and the retained 700 fixed/hold pairs. Native job `103698741111` passed all
17 real CLI process tests (113.84 s); its complete log again has no
LNK4098/LNK2038 or C/Rust compiler warning. Source and surface jobs also
passed, so all four jobs and the overall run are **success** on this exact
code SHA. Hosted action `punycode` deprecations remain. These fixture
durations do not establish large-pattern performance or actual HF readiness.

Local source/policy checks passed 13 JS tests (one POSIX-only test skipped),
the PC4 full-solution authority validator, and all ten managed-CMake policy
tests. The latter only run CMake scripts without enabling a compiler. Native
source binaries were not compiled or launched on the UMCI-restricted local
machine; the successful executable evidence above is from Windows CI.

## Remaining integration and performance gates

Preparation and Core admission currently each audit all original queue pieces.
Preparation is cooperatively paged; the second admission audit is synchronous
with per-ordinal cancellation points. This is functional finite-family
integration, not the planned structural graph x hold x bag x preview DP or a
large-pattern speed claim. Public use still requires bounded admission and
performance evidence, restricted-observation/hidden-bag semantics, applicable
setup/tiling/finite-memory reducers, actual profile qualification, and public
transport/fallback/Discord Cloud Run bypass gates. None is silently marked
complete by the synthetic product matrix.
