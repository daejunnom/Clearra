# BuildUp

BuildUp is the boundary where a `PackingCandidate` becomes an accepted
`BuildVariant`. Packing proves that a set of operation masks can fill the target
region. BuildUp proves that queue, hold, order, line clear, reachability, and
goal constraints can realize that candidate.

## BuildUp Execution Modes

```rust
pub enum BuildUpExecutionMode {
    VerifyFirst,
    EnumerateVariants,
    CountVariants,
}
```

`VerifyFirst` is a quick feasibility and representative replay mode.
`EnumerateVariants` is the coverage-producing mode. `CountVariants` is a
trace-light counting mode for percent/count-heavy objectives.

## BuildUp Variant Enumeration

Coverage, all-solution output, minimum cover, score summary, and SpinTarget
coverage must use `EnumerateVariants` or an equivalent coverage-producing path.
They must not use a single `verify_first` witness as if it represented every
queue/hold branch.

`EnumerateVariants` continues traversing after an accepted variant and records
every reachable operation-order and queue/hold branch until the configured
variant budget is reached. `VerifyFirst` is the only mode allowed to stop after
the first success. `CountVariants` uses the same traversal contract without
retaining replay traces or BuildVariant rows.

```c
clr_buildup_status clr_buildup_verify_first(
    const clr_buildup_problem *problem,
    clr_build_variant_buffer *out_first);

clr_buildup_status clr_buildup_enumerate_variants(
    const clr_buildup_problem *problem,
    const clr_buildup_enumeration_limits *limits,
    clr_build_variant_buffer *out_variants);

clr_buildup_status clr_buildup_count_variants(
    const clr_buildup_problem *problem,
    const clr_buildup_count_limits *limits,
    clr_buildup_count_report *out_report);
```

Mode ownership:

- `verify_first`: representative replay, quick feasibility, visual witness.
- `enumerate_variants`: coverage rows, all solutions, minimum cover,
  score summary, SpinTarget coverage.
- `count_variants`: percent, count-heavy objective, trace-light counting.

Enumeration limits are part of the product contract:

```c
typedef struct clr_buildup_enumeration_limits {
    uint32_t max_variants;
    uint8_t preserve_hold_branches;
} clr_buildup_enumeration_limits;
```

Rust `BuildUpRunner` must derive `max_variants` from the compiled
`SearchProblemBudget`, not from a hard-coded unlimited value. If the C core
returns `CLR_BUILDUP_ENUMERATION_TRUNCATED`, output must report incomplete
probability or count evidence instead of treating the partial variant buffer as
complete.

## Hold Branch Enumeration

`VerifyFirst` may use deterministic first-path selection. `EnumerateVariants`
must preserve branch identity for current-piece use, hold-piece use, and empty
hold store decisions. A build variant must carry enough queue/hold decision data
for replay and score/spin evidence.

## KickEvidence Export

Exact spin classification requires kick evidence to leave the C core and reach
Rust replay/scoring:

- rotation request;
- from/to rotation;
- kick index;
- kick dx/dy;
- kick table/profile ids;
- first-success confirmation;
- predecessor and result anchors.

If the C core validates reachability but does not export this evidence, exact
kick-sensitive spin output is not available.

Kick evidence has its own buffer budget. `CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED`
means the BuildVariant evidence is incomplete; exact kick-sensitive spin
classification must not proceed from that variant unless the caller explicitly
allows estimated output with a diagnostic.

## Score Event Basis Generation

BuildVariant replay is the source for score events. Score event basis data
includes placement events, clear events, drop-distance basis, spin basis, and
kick evidence when the selected profile requires it. The scoring layer remains
post-processing; it must not alter BuildUp acceptance or pattern probability.

## Coverage-Producing BuildUp Path

```rust
pub struct BuildVariant {
    pub operation_order: Vec<OperationId>,
    pub queue_hold_decisions: Vec<QueueHoldDecision>,
    pub consumed_pattern_id: PatternId,
    pub coverage_pattern_id: PatternId,
    pub replay_seed: ReplaySeed,
    pub score_event_basis: ScoreEventBasis,
    pub kick_evidence_list: Vec<KickEvidence>,
    pub trace_completeness: TraceCompleteness,
    pub backend_witness_metadata: BackendWitnessMetadata,
}
```

Only accepted BuildVariants may produce coverage rows. Rejected candidates and
raw packing candidates must not contribute coverage, score cells, or spin target
hits.

## FFI Lifetime And Variant Buffers

C BuildVariant buffers are scope-owned. Rust FFI views must copy kick evidence
and other pointer/count payloads into owned Rust storage before exposing them to
replay, scoring, coverage, or output. Pointer identity from C must not become a
Rust product key; stable ids come from operation set keys, candidate ids,
coverage pattern ids, and trace keys.

## Required Tests

- `verify_first_result_is_not_used_for_coverage`
- `enumerate_variants_preserves_hold_branches`
- `count_variants_reports_complete_count_without_retaining_all_traces`
- `buildup_variant_exports_kick_evidence`
- `spin_target_coverage_uses_enumerated_build_variants`
- `buildup_enumeration_truncation_reports_diagnostic`
- `buildup_enumerate_variant_limit_comes_from_problem_budget`
- `kick_evidence_buffer_budget_rejects_exhaustion`
- `ffi_build_variant_view_copies_kick_evidence_to_block_pointer_escape`
