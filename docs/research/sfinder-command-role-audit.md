# Sfinder Command Role Audit

This audit uses knewjade/solution-finder commit
`0e7c935a5399159a3d9c42fb8721e3c6842ae17d`. Command names are not treated as
interchangeable algorithms. Each command retains only the evidence required by
its output contract.

## Command Roles

| Sfinder command | Solver work and retained evidence | Clearra mapping |
|---|---|---|
| `percent` | Tests each queue family for existence, builds success/failure coverage, and reports probability plus failed queues. It does not need a normalized set of every tiling. | `percent` uses an exact `PatternBitSet` union with `SearchOutputPolicy::CoverageSummary`. It omits solution identities, hashes, candidate digests, and traces. |
| `path` | Enumerates perfect-pack candidates, validates build orders, groups legal piece sequences, and attaches coverage to each surviving solution. | Direct `clearra path` remains the historical alias of `pc-replay`. Sfinder semantics are isolated at `clearra sfinder path` and compile into the complete PC result surface. |
| `cover` | Consumes supplied operation or fumen solutions and evaluates which queues can build them. It does not run perfect-pack search. | Direct `clearra cover` remains the historical alias of typed `build-coverage`. `clearra sfinder cover` instead constrains PC search to the supplied colored Fumen solution set before BuildUp and coverage. |
| `setup` | Searches placements satisfying required-area and margin constraints. It is not a PC-family setup policy finder. | Direct `clearra setup` remains the historical alias of the PC-family `setup-finder`. Sfinder's colored required-area contract is isolated at `clearra sfinder setup` and maps to the build-probability surface. |
| `spin` | Uses a specialized T-spin structure search and SRS reachability. | Clearra uses the selected kick and spin profiles in its separate forward spin finder. SRS-only assumptions are not imported. |
| `ren` | Runs forward longest-combo search, with separate hold and no-hold searchers. | Clearra forward-search infrastructure is the applicable boundary; no PC packing evidence is generated. |
| `util` | Transforms fields, operations, and fumens. | Clearra codec, CTK, and Fumen tools remain outside the search core. |
| `verify` | Verifies kick and input contracts. | Clearra rule-profile verification remains a validation command and never runs packing. |

## Adopted Optimization

The `percent` output contract is existential coverage. Clearra therefore keeps:

- exact PatternBitSet coverage words;
- weighted probability and completeness;
- exact materialized, covered, and failed pattern counts;
- an explicit materialized-universe scope and completeness flag for failures;
- a bounded list of failed queue examples;
- resource and backend diagnostics.

It does not keep:

- normalized solution identities or their set hash;
- per-solution probability rows;
- representative replay state;
- candidate-set digests;
- minimum-cover inputs.

Hashing remains available to PC and `pc-replay` correctness surfaces. The output
policy cannot prune a candidate: it changes retained evidence only, so
buildability and coverage semantics stay identical.

## Fixed-Queue BuildUp Specialization

Sfinder's fixed-order BuildUp does not test every operation against every queue
position. It groups operations by piece and recursively visits only the current
or held piece. Clearra adopts that local idea without replacing inverse
lock-clear geometry search:

- inverse geometry and realization-feasibility rejection remain unchanged;
- an exact one-pattern fixed source enters witness verification from the first
  candidate rather than waiting for global coverage to become complete;
- current, occupied-hold swap, and empty-hold draw transitions select a
  precomputed per-piece operation mask;
- kick reachability, line clears, deleted-row provenance, and projection
  confirmation continue through the existing exact transition function;
- equal current and held pieces share one semantic existence branch;
- a held piece is not released after the finite source is exhausted;
- score, minimum-cover, execution-constraint, and observed-queue requests keep
  the complete BuildOrder language path.

The optimization changes candidate verification work, not the retained
solution identity. A forced full-coverage run remains an exact dual-run oracle.

## Compatibility Boundary

`clearra sfinder` accepts the Sfinder-man search spellings whose result contracts
are represented by Clearra's six product tools. The boundary normalizes legacy
queue syntax and compiles a typed request; it never starts Java or another solver.

- PC search: `path`, `chance`, `minimals`, `score`, `score-minimals`, `saves`,
  and `best-save`;
- supplied-solution coverage: `cover`;
- colored target/build analysis: `setup`, `congruent`, `congruent-cover`,
  `setup-cover`, `cover-percent`, and `special-cover`;
- forward search: `spin-cover`, `spin`, and `cat-finder`;
- PC-family setup ranking: `pc-setup`, `best-setup`, and `dpc-finder`;
- validation: `verify`.

The static colored-Fumen allow-list uses exact initial occupancy plus one
aggregate occupancy mask per piece kind. This preserves repeated equal pieces:
two exact placement decompositions are equivalent only when their colored board
identity is equal. Hashes are lookup aids and never authorize acceptance.

`ren`, `parity`, `util`, `to-gray`, `to-fumen`, `render`, and
`special-minimals` have distinct result contracts that are not represented by a
different Clearra command. They fail explicitly instead of silently returning a
different calculation. CTK/Fumen editing and rendering remain on the CTK product
surface rather than inside the search compatibility namespace.

## Worker Routing

Every represented Sfinder search command uses the worker path of its typed
Clearra target instead of maintaining a second pool:

- `path`, `percent`, `chance`, `minimals`, score variants, saves, and `cover`
  use the PC/scenario distributed geometry and verification coordinator;
- colored-target setup/cover variants use distributed build probability;
- spin and damage variants use the forward-search coordinator;
- PC setup variants use the setup coordinator.

The compatibility boundary accepts `--workers N` and its `--cpu-threads N`
alias as a fixed request. `--auto-workers N` is an adaptive ceiling: it retains
each target engine's small-work serial gate. `--use-all-cpu-threads` is required
when either selection consumes the normally reserved logical processor. Fixed
and adaptive requests are mutually exclusive. `verify` rejects worker options
because it has no search pool.

The boundary only transports resource policy. Exact canonical reduction,
candidate identities, and coverage unions remain owned by the target engine, so
worker completion order cannot change the result set.

## Rejected Direct Ports

- Sfinder strip/profile packing is not substituted for Clearra's inverse
  lock-clear geometry family and proof-carrying pruning pipeline.
- Sfinder's DFS, special I-piece paths, and SRS-only spin assumptions are not
  copied into the WASM CPU or WebGPU product paths.
- Sfinder `setup` and Clearra `setup-finder` have different product meanings.
- Clearra `pc-replay` is not behaviorally expanded into Sfinder `path` without a
  separate product contract and benchmark.

## WASM Evidence

The product browser build after the fixed-queue change has SHA-256
`36edde9f0a91e2f1fddc2e9ef97f35c3516b5a0040a47aaa3593703b2ba9fe3e`.

The fixed queue `IOTSZLJIOT` on empty 4L retained 54 solutions and
`cts1:190dabaeafc3ba19`. Two optimized runs took 554.79-581.30 ms, compared
with the 596.55-613.36 ms pre-change range. BuildOrder/coverage states fell
from 26,958 to 601, exact reachability states fell from 324,496 to 29,321, and
peak CPU memory fell from 7,635,476 to 7,026,300 bytes. Enabling per-solution
probabilities forced the complete generic path and produced the same count and
hash, while visiting 1,346,717 BuildOrder nodes and 11,784,598 reachability
states.

The small fixed queue `IIOOO` on empty 2L retained 4 solutions and
`cts1:8dc81db9bcd4bab9`; two runs took 74.94-86.12 ms. Empty-hold and occupied-
hold fixtures also matched the forced complete path exactly.

Two-run unchanged-path checks:

| Case | Time | Solutions | Normalized set hash | Coverage |
|---|---:|---:|---|---:|
| PCO | 106.99-122.79 ms | 63 | `cts1:8415f86603b3be9d` | 717 / 840 |
| Tsar Cannon | 113.27-133.51 ms | 42 | `cts1:4996a1501bbb8212` | 4,976 / 5,040 |

The percent continuation fixture materialized all 322,560 patterns, covered
53,640, counted 268,920 failures, and emitted only five requested failed queue
strings. Two runs took 237.11-252.31 ms with 7,185,375 bytes peak CPU memory.
Its result reported `solution_set_materialized=false`,
`solution_count_calculated=false`, and `sample_trace_available=false`.
On the final artifact, two confirmation runs took 276.67-277.42 ms and kept
the same counts and memory peak. A separately capped 5,040-pattern run reported
`failed_pattern_count_complete=false`, so a materialization limit cannot make a
partial failed count appear globally complete.
