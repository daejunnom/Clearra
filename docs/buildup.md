# BuildUp

Candidate verification establishes whether queue, hold, order, line clear,
reachability, and goal constraints can realize a `PackingCandidate`. Packing
alone proves that operation masks fill the requested target. The verifier must
produce the evidence required by its consumer: exact pattern coverage for a
Boolean query, or concrete variants and execution evidence for richer queries.

## Current Rust product contract, 2026-11-22

Product execution uses the Rust solver in native and WASM hosts. The preserved
C implementation is an independent reference, not a product fallback. The
historical C signatures below describe that concrete/reference contract; they
do not require every current product query to enumerate a C `BuildVariant`.

With `placement-dependency-pc`, eligible compact Boolean requests use the
placement dependency producer automatically. The legacy
`precompute_build_dependencies=false` setting does not disable this route.
`SearchProblem::pc_placement_dependency_eligible` and the candidate verifier
remain the admission authorities. The supported consumer boundaries are:

| Consumer | Current producer and boundary |
| --- | --- |
| PC `CountUnique` | Exact dependency relation for 10-wide, 1–6L, fixed-horizon 1–11-operation candidates with materialized supply. It produces accepted order languages and original-pattern coverage without a `BuildOrderGraph`. |
| Fixed queues, queue expressions, other materialized universes | The language uses the existing supply/Hold product, including original ordinals, weights, initial Hold/cursor and lookahead semantics. Complete standard bags retain their existing symbolic representation. |
| Verified imported ordered kicks for eligible PC | `CompactKickAuthority` binds the complete ordered table and checks its coordinate/index, finite-domain and compilation bounds. A profile name or tag cannot substitute for the submitted contents. Other consumers keep their own kick admission. |
| Compact Boolean Build | The same relation accepts an explicit `ExactBoardAfterLineClears(board)` target, including nonzero boards, within 10-wide 1–6L/1–11 operations. The automatic extension requires `CountUnique`, materialized supply, oracle observation, no finesse/spin coverage, score or execution constraints, and no explicit memory cap. |
| PC visible-seven | The observation consumer keeps concrete physical lock commitments before reveal. Kind-language coverage alone is insufficient to authorize a policy. |
| CountAll, finesse, scoring/spin/replay families | Existing concrete producers retain path multiplicity, input/rotation evidence and execution identity. A representative witness is not complete family evidence. |
| Setup, extended boards, explicitly budgeted contracts | Existing producers and admission remain. Setup's partial paths, residue and terminal supply are not replaced by the fixed-final-board Build extension. PC node/candidate/frontier limits or an explicit memory cap exclude the automatic dependency route; Build's extension also excludes explicit memory caps. |

Coverage is the original-pattern bitset union with its original weights. Equal
kind languages may share a Boolean supply product; that equality does not
preserve CountAll multiplicity or hidden-information action choices. A selected
dependency proof that is cancelled, incomplete or outside its certified domain
must not become a negative result or silently start another verifier.

Build construction and candidate verification share
`uses_build_placement_dependency`; only a `ClearToEmpty` consumer uses PC
eligibility. A PC-eligible underlying request does not authorize a Boolean
Build path when its separate aggregation requires finesse or spin evidence.
Public Build still rejects imported profiles whose retained nested allocations
have no admitted memory projection. An internal ordered-table owner, when
present, binds the actual Build catalog dimensions. Snapshot, Setup, extended
and Tiling consumers without such an owner require complete canonical ordered
table equality; PC's fixed/no-hold playable-piece exception stays PC-local.

An eligible representative walks the proved operation relation and the original
queue/Hold state, then recovers concrete poses only for the chosen locks. It
does not require a candidate-wide BuildUp graph. `BuildUpWorkspace` still owns
shared languages, caches and retained concrete consumers, so its name does not
mean every request builds a graph.

See the [consumer cleanup proof and integration record](research/dependency-consumers-cleanup-2026-11-22.md)
for exact supply/completion/suffix ownership arguments, the physical-observation
fallback status and validation evidence. The bounded implementation scope is
not a full see-7/P7P4 performance qualification or deletion of every BuildUp
consumer.

## Concrete variant and C reference contracts

The following mode, buffer and FFI contracts apply when a consumer needs
concrete variants or executes the preserved reference implementation.

### BuildUp Execution Modes

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

### BuildUp Variant Enumeration

Coverage must come from a complete coverage-producing path, which may be the
exact Boolean language product described above. Concrete all-path output,
score summary, and SpinTarget evidence require their richer variant/execution
producer. A single `verify_first` witness cannot represent every queue/Hold
branch or establish complete coverage.

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

When the C reference adapter uses Rust `BuildUpRunner`, it must derive
`max_variants` from the compiled `SearchProblemBudget`, not from a hard-coded
unlimited value. If the C core returns `CLR_BUILDUP_ENUMERATION_TRUNCATED`,
output must report incomplete probability or count evidence instead of treating
the partial variant buffer as complete.

### Hold Branch Enumeration

`VerifyFirst` may use deterministic first-path selection. `EnumerateVariants`
must preserve branch identity for current-piece use, hold-piece use, and empty
hold store decisions. A build variant must carry enough queue/hold decision data
for replay and score/spin evidence.

### KickEvidence Export

On the C reference path, exact spin classification requires kick evidence to
leave the C core and reach Rust replay/scoring:

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

### Score Event Basis Generation

BuildVariant replay is the source for score events. Score event basis data
includes placement events, clear events, drop-distance basis, spin basis, and
kick evidence when the selected profile requires it. The scoring layer remains
post-processing; it must not alter BuildUp acceptance or pattern probability.

### Coverage-Producing BuildUp Path

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

On this concrete variant path, only accepted BuildVariants may produce coverage
rows. The Boolean dependency producer may also produce exact coverage after its
complete placement and supply proof. Rejected candidates and raw packing
candidates must not contribute coverage, score cells, or spin target hits.

### FFI Lifetime And Variant Buffers

C BuildVariant buffers are scope-owned. Rust FFI views must copy kick evidence
and other pointer/count payloads into owned Rust storage before exposing them to
replay, scoring, coverage, or output. Pointer identity from C must not become a
Rust product key; stable ids come from operation set keys, candidate ids,
coverage pattern ids, and trace keys.

### Concrete variant/reference regression contracts

These checks qualify the corresponding variant or FFI boundary. They are not
a requirement to route Boolean Rust coverage through the C reference.

- `verify_first_result_is_not_used_for_coverage`
- `enumerate_variants_preserves_hold_branches`
- `count_variants_reports_complete_count_without_retaining_all_traces`
- `buildup_variant_exports_kick_evidence`
- `spin_target_coverage_uses_enumerated_build_variants`
- `buildup_enumeration_truncation_reports_diagnostic`
- `buildup_enumerate_variant_limit_comes_from_problem_budget`
- `kick_evidence_buffer_budget_rejects_exhaustion`
- `ffi_build_variant_view_copies_kick_evidence_to_block_pointer_escape`
