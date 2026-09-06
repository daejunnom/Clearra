# Minimum-cover random-choice width audit — 2026-09-06

Status: the initial read-only finding below is now corrected in the working tree and a same-native-binary width A/B has completed. **The correction is not yet a measured GUI performance fix or release evidence.** Fresh WASM validation remains separate from the source/unit and native timing evidence.

Implementation checkpoint: `CoverRandomChoice::index` now takes the remainder in `u64` before converting the bounded result to `usize`. All eight cooperative/blocking heuristic choice sites use it. Product builds have no legacy-width switch; diagnostic builds can snapshot `CLEARRA_PROBE_RNG_WIDTH=32|64` through the isolated matrix harness, independently of `CLEARRA_PROBE_REPAIR_WORD_MASKS=0|1`. The two fixed-width/positive-authority tests, three word-scorer tests, and three cooperative/blocking witness tests passed (8/8). The fixed-width test explicitly simulates u32 narrowing on either host, so this is not merely a native-width self-comparison.

## Confirmed architecture-dependent choice

The exact solver's positive-only incumbent heuristics maintain a `u64` xorshift state, but several candidate selections convert that state to `usize` **before** taking a remainder:

```text
random_u64 as usize % candidate_count
```

On a 64-bit native host the full state participates. On `wasm32` the cast first drops the upper 32 bits. Consequently, matching input matrices, seed values, budgets, and scoring formulas do not imply matching repair choices across these targets.

Initial source references, before the correction, in `crates/clearra-coverage/src/cover/exact_minimum_cover.rs`:

| Location | Responsibility |
| --- | --- |
| `RandomizedCoverSearchSession::advance_one`, around line 1251 | Restricted greedy candidate selection |
| `FixedCardinalityCoverSearchSession::advance_one`, around lines 1598, 1656, 1690 | Pivot, removed row, and swap choice |
| `improve_randomized_compact_cover_with_memory_guard`, around line 8129 | Blocking reference's matching greedy choice |
| `improve_fixed_cardinality_cover_with_memory_guard`, around lines 8412, 8473, 8503 | Blocking reference's matching pivot/remove/swap choices |
| `next_breakout_random`, around line 8559 | Common fixed-width xorshift state transition |

The perturbation expression `random_u64 % 100 < 8` already takes its remainder in `u64` and does not have this specific width difference.

For the fixed seed `0x9e3779b97f4a7c15`, the first xorshift state is `0xdc1b77ae0bf34dad`. With an illustrative 25 candidates, full-width remainder chooses index **14**, while truncating to 32 bits first chooses index **9**. This integer example is not a claim that this exact candidate count occurs at the first step of the product fixture.

## Why this matters to the current measurements

The actual published Clearra matrix has 246 candidates and 5,040 concrete queues. The current native common-matrix diagnostic reaches `AtMost(25)` through its global positive warm pass in 11 reported work units, so only `AtMost(24)` needs an external proof wave. The earlier published `512225…` WASM/Node diagnostic instead exported an `AtMost(25)` wave taking approximately 34 seconds.

That contrast **must not be attributed entirely to native-versus-WASM arithmetic speed** before checking algorithmic execution equivalence. The native harness advances preparation with budget 128. The recorded Node harness calls `distributed_finish_advance(1, 128)`. Core `ParallelOracle::advance_warm` caps each call against the same remaining global cursor budget, currently 1,000 work units. These nominal budgets do not rule out differences in host admission, input hints, source revisions, or per-call lifecycle. They only identify the candidate-selection cast as a concrete additional difference.

The word-mask scorer A/B used the same native executable and produced the same initial warm work counts and exact/canonical result. It did not compare a native 64-bit random-choice policy against the old wasm32 truncating policy.

## Smallest proposed correction and verification boundary

The source correction, focused tests and matrix A/B below have been implemented; fresh GUI measurements are still pending at this checkpoint.

For a positive candidate count representable in `u64`, take the remainder **before** narrowing the result:

```text
choice = usize(random_u64 % u64(candidate_count))
```

The remainder is strictly smaller than the already-valid `usize` candidate count, so its final conversion is in range. Zero candidates must continue to be rejected or excluded by the existing caller contract. Any helper should keep these preconditions explicit and should cover both cooperative and blocking-reference heuristic paths; updating only one would invalidate their parity tests.

This would preserve the existing native64 choice sequence and make wasm32 use it as well. It changes the old wasm32 heuristic traversal, not the definition of an exact cover or the canonical portfolio order. Positive proposals must still be replay-validated, and missed or cancelled heuristic searches must still provide no negative authority.

Before accepting the proposal:

1. Add cross-width integer tests over fixed/high-bit/random states and non-power-of-two candidate counts. Compare explicit old-u32, old-u64, and proposed remainder semantics without depending on the test host's pointer width.
2. Preserve PRNG state updates, scoring, tie order, random-call count, budget accounting, and protected rows. Retain cooperative/blocking parity tests.
3. Use a diagnostic-only old-wasm32/proposed mode to compare the **same actual matrix** in one controlled executable. Do not inject the known optimum or a successful witness.
4. Record warm admission, hit/miss, work counts, exported proof/canonical queries, and exact final identity. Verify that any warm-path change actually explains the observed divergence.
5. Rebuild WASM once the source is frozen, then measure the real GUI separately. Native success or matching counts alone cannot satisfy the GUI performance target.

No speedup is predicted quantitatively here. Even eliminating an expensive positive wave would leave exact negative proof and canonical-selection work that needs separate measurement.

## Native controlled result

The same release executable (`70d1c284acee06f77fd6c5d1ba8cd917d375ad43e024c6e5eaad763e561fff3c`) was run sequentially with old32 and fixed64 choice, using the actual published Clearra matrix, lexical order,11 compute workers, factor4, assist0 and word scoring enabled in both arms. Solver time was48.818s versus16.771s. Both proved25 and returned identical canonical25 keys. Old32 reproduced the AtMost25 warm miss at1,000 work units; fixed64 found it after11 work units. This establishes a choice-width effect under the controlled native runtime, not a GUI20-second acceptance result. Full stage/hot-cost measurements and artifacts are in `qnia-minimum-cover-stage-comparison-2026-09-06.md`.
