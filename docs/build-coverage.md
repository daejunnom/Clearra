# Build Coverage

Build coverage imports a template, derives slot domains, solves slot assignments, builds a pattern coverage matrix, and computes union probability. The build-coverage crate owns build-specific query and limit types.

## Coverage Universe Identity

Coverage rows must carry enough identity to prove that OR union is meaningful.
Two rows with the same `pattern_count` are not necessarily compatible. Rows may
be unioned only when all of these fields match:

```rust
pub struct CoverageRow {
    pub candidate_id: CandidateId,
    pub row_kind: CoverageRowKind,
    pub pattern_universe_id: PatternUniverseId,
    pub pattern_count: usize,
    pub pattern_weight_model_id: PatternWeightModelId,
    pub coverage_bits: PatternCoverageBitSet,
}
```

`pattern_universe_id` identifies which queue/build/setup universe the bit
indexes refer to. `pattern_weight_model_id` identifies the probability measure
applied to those bits. A matrix reducer must reject universe or weight-model
mismatches before OR-ing rows.

## CoverageRowKind

Coverage rows share the same probability invariant, but their purpose is
explicit:

```rust
pub enum CoverageRowKind {
    Pc,
    Setup,
    Build,
    SpinTarget(SpinTargetId),
    ScoreCell(ScoreObjectiveCellId),
}
```

`SpinTarget` and `ScoreCell` rows are still pattern coverage rows. They do not
own a separate probability model and must use the same `PatternBitSet` OR union
path as PC/setup/build coverage.

## SpinTarget Coverage

Spin target probability is a coverage problem:

1. enumerate accepted `BuildVariant` rows for each packing candidate;
2. replay each variant;
3. classify spin evidence from replay and kick evidence;
4. apply `SpinTargetPredicate`;
5. emit `CoverageRowKind::SpinTarget(spin_target_id)` for matching patterns;
6. compute probability from `PatternBitSet` OR union.

Spin target probability must never be computed by summing variant
probabilities. A packing candidate is not enough evidence for a spin target;
the predicate is applied only after BuildUp and replay.

## Score Cell Coverage

Score-aware objectives may create `CoverageRowKind::ScoreCell` rows for
pattern-best score cells. Score contribution changes candidate ranking, not the
underlying pattern probability. The reducer must count each covered pattern
once, then attach score/attack expectation from the selected pattern
contribution.

## C Coverage ABI Capacity

The current fixed-size C coverage structures are a bridge detail, not a product
invariant. Product contracts must allow sparse or owned bitsets beyond the
current fixed capacity:

```c
typedef struct clr_pattern_bitset_view {
    uint64_t pattern_universe_id;
    uint64_t pattern_weight_model_id;
    uint32_t pattern_count;
    uint32_t word_count;
    const uint64_t *words;
} clr_pattern_bitset_view;

typedef struct clr_owned_pattern_bitset_c {
    uint64_t pattern_universe_id;
    uint64_t pattern_weight_model_id;
    uint32_t pattern_count;
    uint32_t word_count;
    uint32_t word_capacity;
    uint64_t *words;
} clr_owned_pattern_bitset_c;

typedef struct clr_coverage_row_view {
    uint64_t candidate_id;
    uint32_t row_kind;
    uint32_t coverage_pattern_id;
    uint64_t pattern_universe_id;
    uint64_t pattern_weight_model_id;
    clr_pattern_bitset_view coverage_bits;
} clr_coverage_row_view;
```

The coverage status namespace must include:

- `CLR_COVERAGE_PATTERN_UNIVERSE_MISMATCH`
- `CLR_COVERAGE_WEIGHT_MODEL_MISMATCH`
- `CLR_COVERAGE_CAPACITY_EXCEEDED`
- `CLR_COVERAGE_ROW_KIND_UNSUPPORTED`
- `CLR_SCORE_MATRIX_CAPACITY_EXCEEDED`
- `CLR_SPIN_COVERAGE_CAPACITY_EXCEEDED`

Rust FFI views must copy scope-bound C words into owned snapshots before the C
scope can be released. A `clr_pattern_bitset_view` pointer may be inspected only
inside the active C memory scope; Rust output, coverage, and objectives must use
owned `PatternBitSet` / `OwnedCorePatternBitSetSnapshot` data.

## PatternBitSet Dynamic Word Allocation

`PatternBitSet` uses dynamic words in Rust, but allocation is still budgeted by
the caller. C bridge defaults may use a `1024` pattern budget, while product
coverage can choose a larger budget or an unbounded Rust-owned matrix. Exceeding
a configured dynamic word budget must return `WordCapacityExceeded`; silent
truncation is forbidden.

## Spin And Score Matrix Budgets

`SpinCoverageMatrix` and `ScoreCellMatrix` are wrappers over the same typed
coverage matrix invariant. They add row and word budgets for product safety:

- `SpinCoverageMatrixBudget` limits spin-target coverage rows and pattern words.
- `ScoreCellMatrixBudget` limits score-cell rows and pattern words.
- `SpinCoverageCapacityExceeded` and `ScoreCellCapacityExceeded` are hard
  budget failures unless the caller explicitly requested an incomplete result.

Score-cell rows may rank candidates or compute expectations, but they must not
change coverage probability. Spin rows and score rows must still carry
`pattern_universe_id` and `pattern_weight_model_id`.

## Observed Queue Truncation And Probability Mass

Observed queue expansion may truncate the materialized pattern universe. When
that happens, output must preserve `materialized_probability_mass` and set
`probability_complete=false`. Clearra must not renormalize a truncated observed
universe to `1.0`.

## Forbidden Conflations

- Do not OR rows solely because `pattern_count` matches.
- Do not treat the C fixed `1024` pattern capacity as a product limit.
- Do not use `ShapeUnionMask` as `PatternCoverageBitSet`.
- Do not calculate SpinTarget probability by summing variant probabilities.
- Do not add pattern probability twice through score contributions.

## Required Tests

- `coverage_row_rejects_universe_mismatch`
- `coverage_row_rejects_weight_model_mismatch`
- `c_coverage_capacity_exceeded_reports_status`
- `shape_union_mask_is_not_pattern_coverage`
- `spin_probability_uses_pattern_bitset_union`
- `score_cell_coverage_does_not_change_probability`
- `pattern_bitset_dynamic_word_allocation_scope_is_enforced`
- `spin_coverage_matrix_memory_budget_rejects_word_overflow`
- `score_cell_matrix_memory_budget_rejects_word_overflow`
- `observed_queue_truncation_keeps_materialized_probability_mass`
