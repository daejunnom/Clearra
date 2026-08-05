# Architecture

Clearra uses a Rust product shell with a C hot-path core. Rust owns product contracts,
validation, diagnostics, probability invariants, objectives, replay, and output. The
C core owns immutable geometry-catalog compilation, exact-cover graph search,
and build-up search work.

The request flow is:

user input
-> Rust CLI
-> Query Assembler
-> clearra-app
-> Validation
-> clearra-problem
-> SearchProblem
-> clearra-core-executor
-> C PackingProblem
-> core-c Geometry Catalog / Exact-Cover Graph
-> Host Reducer
-> core-c BuildUp
-> CoverageRow
-> Rust Coverage / Objectives
-> Rust Replay / Output

The product boundary is fixed by `docs/dependency-boundary.md`. The complete
pipeline is `CLI / GUI / WASM Command Runtime -> AppRequest -> clearra-app ->
validation -> SearchProblem compile -> clearra-core-executor -> C
PackingProblem -> C Geometry Skeleton Exact Cover -> Host reducer -> C BuildUp BFS ->
CoverageRow -> CoverageMatrix -> ObjectiveResult -> Replay / PostProcess /
Scoring / Fumen / Render / Output -> AppResponse`.

Search and PostProcess are separate layers. Search proves PC/build feasibility.
PostProcess consumes accepted search evidence for replay, scoring, fumen,
render, explanation, and output. Fumen-like is an adapter, not an internal
search model.

Connected post-processing accepts owned `CandidateExecutionAggregate` values,
each containing candidate identity and real `ReplayTrace` executions. It
materializes a `ScoreMatrix` from those traces and the selected score profile;
field-average scoring is complete only when that matrix and its pattern weights
are complete. Optional PostProcess GPU work is isolated behind the
`webgpu-postprocess` feature and currently performs PatternBitSet union only.
`PostGpuCapabilityState` reports `Connected`, `Unavailable`, or
`RejectedMismatch`; a connected result must be `TrustedDeterministic` or
`TrustedCpuSampleConfirmed` before it is exact. Fallback stays attached to an
unavailable PostProcess GPU outcome with its reason. The search and postprocess
backend reports remain separate, and postprocess does not change PC coverage
probability. Count-only replay/evidence/spin batch constructors are not product
APIs.

Internal field data is occupancy-only. Rust uses `OccupancyField { width, height, mask }`
and C mirrors it with `clr_occupancy_field { mask, width, height, reserved }`.
Text fields are parsed from top-down rows into bottom-up row-major masks where
bit index is `y * width + x`. Search and packing operations use target-frame
coordinates; BuildUp applies deleted-line y adjustment before replay emits
lock-frame events. Color, piece owner, cleared-cell owner, render frame, and
fumen page state belong to replay/render/fumen adapters, not search core state.

Packing and supply are separated. C Geometry Skeleton Exact Cover owns placement
geometry only; it compiles one immutable realization/skeleton/support catalog and
must not carry queue or hold state in its continuation key. Product
supply lowers to `PieceSource`, and BuildUp verifies order through
`HoldAutomatonState`. The ABI mirrors are `clr_piece_source_descriptor` and
`clr_hold_automaton_state`, and the BuildUp memo key includes piece source id,
cursor, hold piece/empty state, bag epoch, bag remainder key, and provenance.
GPU packing descriptors carry only PieceSource/pattern identity and a piece
multiset window; no ordered queue preview is present in the product ABI.

Geometry, family, and BuildOrder scratch uses fixed-size, non-moving chunks.
Growing this scratch allocates one additional chunk and keeps prior chunks
address-stable; it never copies the complete frontier or graph through a
geometric `realloc`.
`max_frontier_states` is a logical resource limit rather than a reservation.
The full-artifact runner defaults to the indexable range and allocates on
demand, while an explicit frontier or memory budget remains authoritative.
Allocation exhaustion produces an incomplete resource report, never a native
access violation or a silent candidate drop.

The full-artifact runner applies the same policy to retained candidates. Its
default has no arbitrary five-million candidate cutoff: candidate and frontier
auto limits use the maximum safely indexable range, storage grows only on
demand, and allocation exhaustion remains an explicit incomplete result.
User-supplied candidate, frontier, and memory budgets are still authoritative.

The C problem descriptor boundary is explicit. Rust `SearchProblem` lowers to
`clr_packing_problem` with `piece_window`, `piece_multiset_window`,
`piece_source`, rule, budget, backend, checkpoint, and goal policy. It does not
own queue cursor or hold piece state. `clr_buildup_problem` wraps the packing
descriptor and separately owns `piece_source` plus `initial_hold_automaton`.
C views do not cross into Rust output as borrowed buffers; FFI views are copied
to owned Rust values after pointer/count bounds are checked.

Opening PC is not a separate solver. It is a `SearchProblem` preset. Scenario PC is also a `SearchProblem` preset. Setup and build coverage use the same problem family.
PackingCandidate is not a solution until BuildUp and replay/output contracts have
turned it into coverage rows and traceable output.

## Rust Product Crates

These crates own the Rust product and domain layers:

- `clearra-core-domain`
- `clearra-profiles`
- `clearra-piece-registry`
- `clearra-geometry` metadata, layout, and schema-facing views
- `clearra-rules` profile/import/export/verification contracts
- `clearra-supply`
- `clearra-coverage`
- `clearra-objectives`
- `clearra-build-coverage`
- `clearra-setup-search` high-level family, tiling, build, and coverage models
- `clearra-scoring`
- `clearra-output`
- `clearra-fumen`
- `clearra-render`
- `clearra-validation`
- `clearra-app`
- `clearra-cli`
- `clearra-ui-schema`
- `tests/fixtures`
- `tests/golden`
- the diagnostic code system

Executable packaging belongs to `clearra-cli`. Library crates must not own Windows
executable manifests or other packaging adapters. The Windows manifest is kept as a
CLI packaging asset, and `scripts/verify.ps1` injects the absolute Windows MSVC
linker flag needed for test harnesses.

## C Core Responsibilities

The C core owns the hot path:

- compact `PackingProblem` representation
- packing candidate enumeration
- host-side candidate reduction boundary
- BuildUp search from candidates
- runtime compact rule/reachability data
- hot-path board placement, line clear, reachability, and candidate generation

Rust must call the C core through `clearra-core-executor`; it must not grow a second
hot-path solver in parallel.

## M5 C Board64 Core

The Board64 hot path is implemented in C across `board64.c`, `board_layout.c`,
`row_mask.c`, `collision.c`, `place.c`, `line_clear.c`, and `board_hash.c`.
The public Board64 core owns cell index, single cell mask, row mask, occupied
mask validation, row full detection, collision check, place operation, line
clear, line clear count, board hash, and board equality.

The API names occupied mask validation and line clear after placement as fixture
contracts because both are correctness boundaries for candidate and BuildUp code.

The M5 fixture file is `core-c/tests/test_board64.c`, with
`core-c/tests/board64_tests.c` kept as the compatibility fixture body. The
required fixture coverage is empty board, single cell mask, row full detection,
single line clear, multi line clear, piece placement collision, line clear after
placement, hash stable, and board equality.

## M6 C Piece / Operation Table

The C core owns the standard tetromino operation table directly. Piece metadata
and operations live under `core-c/src/piece`: `tetromino.c`, `rotation.c`,
`operation.c`, `operation_table.c`, and `operation_set.c`, with the internal
contract exposed through `operation.h`.

The operation table uses the public `CLR_PIECE_I`, `CLR_PIECE_O`,
`CLR_PIECE_T`, `CLR_PIECE_S`, `CLR_PIECE_Z`, `CLR_PIECE_J`, and `CLR_PIECE_L`
ids, four rotation states, cell offsets, computed bounds, deterministic
operation id, and Board64-backed operation mask generation. Operation ids are
stable by piece order and rotation: `(piece - CLR_PIECE_I) * 4 + rotation`.

The M6 fixture is `core-c/tests/operation_table_tests.c`. It verifies I/O/T/S/Z/J/L
exist, each piece has four rotation operations, operation mask stable behavior,
bounds correct behavior, piece area = 4, operation id deterministic behavior,
and the generated 28-operation table. Candidate and reachability code should
consume this table instead of inventing new standard tetromino geometry as the C
hot path grows.

## M7 C Rule / Kick Compact Model

Rust `clearra-rules` remains the owner of rule profile source data,
import/export, verification, registry metadata, and profile capability reports.
Rust clearra-rules remains the owner of those source-side contracts.
The C core owns only the compact runtime view consumed by packing, candidate, and
reachability hot paths.

The compact runtime files are `core-c/src/rules/rule_profile.c`,
`srs_kicks.c`, `no_kick.c`, `kick_table.c`, and `spawn_profile.c`, with
`rules.h` as the internal C contract. `clearra_rule_profile_from_descriptor`
converts `clr_rule_profile_descriptor` values into `ClearraCompactRuleProfile`.
The compact runtime supports SRS, SRS+, Jstris 180, and NoKick built-ins.
SRS-X reaches this boundary only through a verified imported descriptor; ASC,
ARS, unverified imported, and custom rules return explicit unsupported status
instead of falling back.

The C descriptor conversion preserves kick transition count, 180 support flag,
NoKick zero-offset-only behavior, unsupported rule status, and SRS+ capability
reported state. SRS has 56 compact transitions, SRS+ has 80 compact transitions,
Jstris 180 has 72 compact transitions with no O rotation entries, and NoKick has
56 zero-offset transitions.

## M8 Sfinder-Compatible Candidate

C candidate generation owns the first compact `Candidate.search(board,
active_piece, rule)` surface. The source files are
`core-c/src/candidate/candidate_search_dispatch.c`, `harddrop_candidate.c`,
`locked_candidate.c`, `locked180_candidate.c`, and `candidate_cache.c`.
Candidate generation consumes the C operation table instead of local piece
geometry, accepts a compact rule profile through `clearra_candidate_search`, and
returns a deterministic possible Action/Operation list.

The M8 packing fixtures live in `tests/fixtures/packing`:
`harddrop_candidates.json`, `locked_candidates.json`, and
`locked180_candidates.json`. The C fixture test verifies harddrop candidate
matches fixture behavior, locked candidate matches fixture behavior, locked180
candidate matches fixture behavior, candidate cache key includes board/rule/piece,
and duplicate candidate removed behavior. Duplicate removal is by final operation
identity `(piece, rotation, x, y, mask)` so multiple actions cannot inflate the
operation list.
M8 explicitly verifies harddrop candidate matches fixture, locked candidate
matches fixture, and locked180 candidate matches fixture. The fixture set also
pins Sfinder-compatible edge cases: harddrop impossible but locked reachable,
collision-free but unreachable placements are rejected, a locked180-only
placement is accepted only by the 180-capable path, and kick first-success
earliest valid offset ordering wins over later valid offsets.

## M9 C Reachability

C reachability is not a collision-free placement check. The stable runtime
policies are `HarddropOnly`, `LockedReverseGraph`, and
`Locked180ReverseGraph`. Spawn-aware movement is unsupported and has no stable
enum value. The C surface names connected policies through
`ClearraReachabilityPolicy` and routes modes through
`clearra_reachability_policy_for_mode`.

The reachability hot-path files are `core-c/src/reachability/reachability_checker.c`,
`harddrop_reachability.c`, `locked_reachability.c`, `kick_first_success.c`, and
`reachability_cache.c`. Harddrop checks that the operation is grounded and that
the vertical path is clear. Locked reachability uses a reverse movement graph
over Board64 states. Locked180 reachability additionally permits half-turn kick
predecessors and reports `used_180` when the successful path depends on one.

The M9 negative/positive fixtures are represented in
`tests/fixtures/packing/reachability_*.json` and mirrored by
`core-c/tests/reachability_tests.c`: collision-free but unreachable, harddrop
reachable, locked reachable via multiple movements, kick reachable only with
first-success offset, 180 reachable, and kick order mismatch rejected.
M9 explicitly verifies harddrop reachable and collision-free but unreachable
cases as separate outcomes.
The first-success fixtures cover both "earlier offset collides, later offset is
used" and "first-success earliest valid offset is chosen even when later offsets
are also valid." The 180 fixture asserts that locked90 rejects the same target
before Locked180ReverseGraph accepts it and records `used_180`.

Candidate/reachability oracle markers: harddrop impossible but locked reachable;
collision-free but unreachable; locked180-only placement; kick first-success earliest valid offset; locked90 rejects the same target.

## M10 Geometry Skeleton Exact Cover

Product packing compiles an immutable placement catalog with
`clearra_geometry_catalog_compile`, searches a pointer-stable solution-family
graph with `clearra_geometry_exact_cover_search_graph`, and passes borrowed row
IDs directly to pattern-specific BuildUp through
`clearra_geometry_solution_graph_stream_buildable_task` or
`clearra_geometry_catalog_rows_buildable_to_sink`. A row set is materialized as
a `PackingCandidate` only after exact BuildUp acceptance. Rejected geometry
paths never allocate a product candidate and never produce coverage rows.

The catalog owns target-frame geometry and realization families. The graph owns
exact predecessor constraints and parent references without copying complete
operation tuples into every frontier state. BuildUp owns PieceSource order,
hold, deleted-line state, reachability, and the exact operation-order proof.
Opening and scenario presets lower to the same `clr_packing_problem`; queue order
and hold state never enter geometry packing state.

Problem descriptor packing uses `board.search_height` for the Board64 layout;
`visible_height` is not allowed to clip placement/search masks.
`goal_region_mask`, `required_fill_mask`, `forbidden_mask`, and `initial_board`
are validated against the search-height mask universe.

The product boundary streams accepted rows into `NativeCandidateReducer`; it
does not create a full raw candidate buffer before BuildUp. Exact tuple compare
remains mandatory after hash matches, so distinct tilings and operation sets are
preserved. `packing_enumerator_cpu.c`, the materializing GPU readback adapter,
and their raw candidate buffer are `BUILD_TESTING` checkpoint/oracle surfaces,
not product entrypoints.

M10 verifies immutable catalog identity, pointer-stable graph storage, exact
piece-count residual memo keys, exact predecessor propagation, pattern-specific
BuildUp acceptance, accepted-row-only materialization, deterministic candidate
identity, and collision-safe exact reduction.

## M11 Host Reducer

`NativeCandidateReducer` receives only candidates materialized from
BuildUp-accepted catalog row sets. It uses compact bucket heads and next indices
for lookup, then calls `exact_candidate_matches` before dropping a duplicate.
Canonical IDs are assigned after exact reduction, so producer order from CPU or
WebGPU cannot become solution identity.

The reducer's memory belongs to the host worker and is included in the resource
report. Candidate or memory budget exhaustion is incomplete execution, never a
dedupe proof. The older C `clearra_packing_host_reduce` and raw-to-canonical
table remain `BUILD_TESTING` backend-equivalence checkpoints only. Hash matches
in either implementation are accelerators; exact operation tuples and final
state remain authoritative.

## M12 BuildUp Problem Builder

`CBuildUpProblemTemplate` compiles immutable board, supply, rule, kick, and
budget descriptors once per `SearchProblem`. Geometry workers attach the shared
catalog to private scratch problems; C receives borrowed catalog row IDs and
exact predecessor constraints through `ClearraBuildUpOperationSource`. This is
the product existence gate and does not copy a complete candidate per geometry
path.

`clr_buildup_problem` carries the initial board, operation set, representative
order hint, queue view, hold state, bag window, rule/kick profile, line clear
policy, piece window, goal, and coverage pattern id. After the existence gate
materializes an accepted candidate, the same template may configure that
candidate for full variant enumeration, replay, and coverage. This downstream
conversion cannot authorize initial candidate materialization. Representative
order remains a priority hint; all legal operation, hold, reachability, and line
clear branches required by the requested mode remain searchable.

## BuildUp Verify / Enumerate / Count Split

BuildUp has three execution modes: `CLR_BUILDUP_MODE_VERIFY_FIRST`,
`CLR_BUILDUP_MODE_ENUMERATE_VARIANTS`, and `CLR_BUILDUP_MODE_COUNT_VARIANTS`.
`verify_first` returns one representative witness for replay or a quick
possibility check. It must not source coverage, all-solution output, or
min-cover. `enumerate_variants` is the coverage-producing path and preserves
current, swap-held, and store-current hold branches through `hold_branch_kind`.
`count_variants` is the count-heavy path and reports total count without
retaining BuildVariant traces. BuildVariant export preserves `final_board`,
`operation_set_key`, `coverage_pattern_id`, `placed_count`, `queue_cursor`,
`hold_piece`, `hold_empty`, `cleared_lines`, `hold_branch_kind`,
`kick_evidence` pointer/count, and `trace_completeness_flags`.

Completion markers: `buildup_verify_first_returns_single_witness`,
`buildup_enumerate_variants_preserves_hold_branches`,
`buildup_count_variants_reports_total_count_without_retaining_traces`,
`verify_first_result_not_used_for_min_cover`,
`build_variant_exports_hold_branch_kind`, and
`build_variant_exports_kick_evidence`.

## M2 C Memory Scope

The C core hot path owns memory through scope-based pseudo-GC. The public memory
surface is `core-c/include/clr_memory.h`; the implementation is split across
`core-c/src/memory/clr_mem_context.c`, `clr_scope.c`, `clr_allocators.c`,
`clr_release_queue.c`, `clr_gpu_buffer_lifetime.c`, and
`clr_memory_debug.c`.

The supported scope kinds are `SearchScope`, `BatchScope`, `WorkerScope`, and
`GpuTransferScope`, represented in C as `CLR_SCOPE_SEARCH`, `CLR_SCOPE_BATCH`,
`CLR_SCOPE_WORKER`, and `CLR_SCOPE_GPU_TRANSFER`. Search allocations must live
inside one of those scopes and be released by scope release, scope abort, or an
epoch release queue drain. Debug canary and poison checks must fail loudly rather
than hiding corrupted hot-path memory.

`ClrScopeState` makes deferred release explicit: `CLR_SCOPE_ACTIVE`,
`CLR_SCOPE_PENDING_RELEASE`, `CLR_SCOPE_RELEASED`, and `CLR_SCOPE_ABORTED`.
A scope deferred into the release queue cannot be released, aborted, or deferred
again until `clr_release_queue_drain` reaches the release epoch. GPU transfer
buffers carry fence epochs through `clr_gpu_buffer_set_fence_epoch`; release
before the fence leaves `pending_gpu_buffer_releases` visible in the leak report
until the safe epoch drains it.

Rust wrappers live in `clearra-core-ffi/src/memory` and executor-facing guards
live in `clearra-core-executor/src/memory/scope_guard.rs`. Rust code should use
RAII guards for scope ownership, and product code must not hand-roll C memory
release calls outside this wrapper boundary.

The Rust wrapper is split into a contract layer and a feature-gated native
layer. `ContractCoreContext`, `ContractSearchScope`, and `ContractBatchScope`
are Rust-side lifetime contract wrappers used by tests and executor guards.
`CoreContext`, `SearchScope`, and `BatchScope` name the contract wrappers.
`MemoryBackendKind` distinguishes contract-only and native-bound execution;
an unavailable native binding returns `BindingUnavailable` rather than a
successful placeholder context.
`NativeCoreContext`, `native_scope.rs` (`NativeSearchScope`, `NativeBatchScope`,
`NativeScopeKind`), `native_memory_bindings.rs`, `native_memory_error.rs`, and
`native_leak_report.rs` own the safe-wrapper boundary for the
`clr_mem_context_*` binding. Raw C pointers and `extern "C"` declarations are
private to `native_memory_bindings.rs`; public callers only see RAII wrappers
and owned reports. The binding is gated by `native-memory-binding`, which uses
the existing runner-owned native link policy and does not reintroduce Cargo
build scripts.

## M3 SearchProblem Canonical Model

`clearra-problem` owns the canonical model between validated command/query
adapters and executor input. Opening, scenario, setup post-PC, and build coverage
bridges all lower to one `SearchProblem` shape before the C executor sees them.
There is no product solver split based on whether the user started from `pc`,
`pc-scenario`, setup, or cover.

The canonical query/preset files are:

- `query/pc_query.rs`
- `query/scenario_query.rs`
- `query/setup_query.rs`
- `query/build_query.rs`
- `preset/opening_preset.rs`
- `preset/scenario_preset.rs`
- `preset/setup_preset.rs`
- `preset/build_preset.rs`
- `compile/problem_compiler.rs`
- `compile/packing_problem_compiler.rs`
- `compile/compile_error.rs`

`SearchProblem` owns the executor-facing contract fields: initial board,
visible/search height, piece window, queue/hold/bag supply provenance,
piece-set and rule/kick/spawn profile selection, goal, budget, backend request,
output policy, replay/trace policy, continuation policy, objective/count policy,
and labels.

`visible_height` is presentation and input-display metadata. C packing/search
layout requires `search_height` and uses it as the Board64 cell universe.
`initial_board`,
`goal_region_mask`, `required_fill_mask`, and `forbidden_mask` are interpreted
against the `search_height` universe; masks above `visible_height` but inside
`search_height` are valid scenario/setup cells, while masks outside
`search_height` are invalid.

The required lowering contracts are:

- `pc --lines 2` -> `OpeningPreset` -> `SearchProblem`
- `pc-scenario` fixture or inline query -> `ScenarioPreset` -> `SearchProblem`
- setup search -> `SetupPreset` -> `SearchProblem`
- setup post-PC -> post-PC board/queue/hold -> `ScenarioPreset` -> `SearchProblem`
- build coverage bridge -> `BuildPreset` -> `SearchProblem` and
  `PackingProblemSpec`
- continuation token -> canonical opening/scenario query -> `SearchProblem`

Setup query ownership is canonical in `clearra-problem`. The setup-search crate
may re-export `SetupSearchQuery`, `SetupLimits`, `SetupHoldPolicy`,
`SetupProbabilityFilter`, `PieceBudget`, `GroupingMode`, and `SetupQueueInput`,
but it must not define a competing query contract. Build coverage similarly
passes through `BuildQuery`/`BuildTemplateBridge` instead of pushing raw template
or fumen-like parsing into the executor boundary.

## M4 C Compact Problem Descriptor

`SearchProblem` lowers to compact C-readable descriptors before executor work
starts. The public C descriptor headers are `clr_problem.h`, `clr_board.h`,
`clr_piece.h`, `clr_piece_source.h`, `clr_hold_automaton.h`, `clr_rules.h`, and
`clr_supply.h`; they define `clr_packing_problem`, `clr_buildup_problem`, board
descriptors, piece windows and multisets, PieceSource identity, HoldAutomaton
state, rule/kick/spawn ids, budget, backend request, goal, count policy,
objective, and checkpoint metadata.

The M3 SearchProblem to C compact descriptor mapping table is:

| SearchProblem field | C descriptor field |
| --- | --- |
| board profile | `clr_board_descriptor` |
| initial board mask | `initial_mask` / `initial_mask_hi` |
| piece window | `clr_piece_window_descriptor` |
| supply identity and pattern universe | `clr_piece_source_descriptor` |
| packing piece availability | `clr_piece_multiset_window` |
| BuildUp queue/hold state | `clr_hold_automaton_state` |
| rule profile | `clr_rule_profile_descriptor` |
| budget | `clr_problem_budget` |
| backend request | `clr_backend_request` |
| goal | `CLR_GOAL_CLEAR_TO_EMPTY` |
| checkpoint labels | `clr_checkpoint_spec` metadata |

Descriptor conversion failures are surfaced as `FfiProblemError`, including
invalid board layout, unsupported board backend, incomplete PieceSource where
exact input is required, unsupported or unverified rule/kick profile, and
impossible budget values. Unsupported board descriptors must not silently fall
back to Board64.

The C source split is:

- `core-c/src/problem/packing_problem.c`
- `core-c/src/problem/buildup_problem.c`
- `core-c/src/problem/problem_defaults.c`

The Rust FFI builder split is:

- `clearra-core-ffi/src/problem/packing_problem_builder.rs`
- `clearra-core-ffi/src/problem/buildup_problem_builder.rs`
- `clearra-core-ffi/src/problem/ffi_problem_error.rs`

The builder converts `SearchProblem` into `CPackingProblem` without exposing raw
Rust query structs to C. It preserves board width and height, initial board
mask, piece window and exact piece policy, piece multiset, PieceSource identity,
rule profile id, effective kick profile id, budget, backend request, goal, count
policy, and objective. `CBuildUpProblemBuilder` separately attaches the concrete
PieceSource pattern reader and initial HoldAutomaton state. C receives numeric
compact ids and bounded owned pattern data, never Rust strings or heap-owned
query objects.

The compact descriptor explicitly carries the initial board mask, rule profile
id, effective kick profile id, budget, backend request, piece multiset, and
PieceSource identity as C-readable fields.

`clearra-core-executor` calls `CPackingProblemBuilder::from_search_problem`
before dispatching native execution. If the C core is unavailable, product
execution returns `E_NATIVE_CORE_UNAVAILABLE` with no selected backend and no
result. There is no alternate direct-constructor product path.

## Versioned Continuation Compatibility

Continuation decoding supports the explicitly versioned `pc1` and `sc1` token
formats. This decode-only support is stable and has no scheduled removal;
current encoders emit `pc2`, `sc2`, or `sr2`. Version 1 parsing is isolated in
`continuation_token_v1.rs`, outside the solver hot path. The existing
`v1_tokens_migrate_to_current_encoding` test decodes version 1 input, emits a
current token, and parses that current token again.

## T0 Dependency Boundary Validation

Architecture validation owns the crate responsibility boundary. It must reject
workspace dependency cycles and direct production imports that would let a
user-facing shell, renderer, scoring layer, or web surface bypass the typed
product API.

The following boundaries are release-blocking:

- `clearra-app` must not depend on `clearra-cli` or the raw C FFI layer.
  `clearra-app` owns typed `AppRequest`/`AppResponse` orchestration, not raw
  C pointer lifetime.
- `clearra-gui-host` must not depend on `clearra-cli`, `clearra-core-ffi`, or
  `clearra-core-executor`. GUI execution flows through
  `GuiRequestBuilder -> AppRequest -> clearra-app -> AppResponse`; it must not
  spawn the CLI or parse CLI text.
- `clearra-render` must not depend on `clearra-core-executor`, `clearra-cli`,
  or runtime raw SVG renderer modules. The renderer consumes replay/render
  contracts and PNG atlas/manifest data, not solver internals or raw SVG at
  runtime.
- `clearra-wasm` must not import native path, filesystem, or process APIs.
  Browser command compatibility is typed-request based and must not inherit
  native process semantics.
- `clearra-webgpu` must not expose user-provided shader loading. WebGPU uses
  reviewed embedded shader contracts with hash/version reporting.
- `clearra-scoring` must not depend on a search implementation crate or direct
  C FFI bindings. Scoring is replay post-processing and cannot be part of the
  core search path.
- `clearra-coverage` must not depend on `clearra-scoring`. Probability remains
  a pattern-bitset union invariant, not a scoring-side calculation.

The dependency gate reports these contract markers when it catches regressions:
`architecture_validation_rejects_dependency_cycle`,
`architecture_validation_rejects_gui_to_cli_dependency`,
`architecture_validation_rejects_render_to_solver_dependency`, and
`architecture_validation_rejects_scoring_in_core_search_path`.

## T1 C Core Unit / Fixture Test Matrix

The C core hot path is guarded by fixture-centered CTest groups, not by one
opaque smoke executable. The required matrix is:

- `memory_tests`
- `board64_tests`
- `board_backend_dispatch_tests`
- `operation_table_tests`
- `rule_profile_tests`
- `supply_tests`
- `cache_identity_tests`
- `candidate_tests`
- `reachability_tests`
- `packing_tests`
- `gpu_tests`
- `scheduler_tests`
- `buildup_tests`
- `coverage_tests`
- `scoring_event_tests`

Root CTest always registers the aggregate `clearra_core_all_tests` executable.
`CLEARRA_CORE_SPLIT_TESTS=ON` additionally registers each C test group as its
own executable so `COnlySplit -ExecutionSurface Trusted` can prove the split
matrix. ManagedLocal configures `BUILD_TESTING=OFF` and does not generate that
surface. The
sanitizer surface is explicit: `CLEARRA_CORE_ENABLE_ASAN=ON` and
`CLEARRA_CORE_ENABLE_UBSAN=ON` build ASAN/UBSAN variants where the selected C
compiler supports those flags. Capacity and truncation evidence must stay in
the C matrix through `build_up_count_reports_truncation`,
`enumerate_variants_sets_count_complete_false_when_truncated`,
`autotune_never_drops_coverage_rows_silently`,
`partial_result_reports_truncation_reason`, and coverage capacity status tests.

T1 closure requires: ctest aggregate passes, split c tests pass, asan build
passes where available, ubsan build passes where available, and
capacity_exceeded_tests_pass.

## T2 Rust FFI Safety Tests

Rust FFI safety is verified at two layers: compile-time feature gating and
owned wrapper behavior. The default build keeps native binding unavailable.
`native-memory-binding`/`native-c-core` builds may call C, but raw pointers stay
inside `clearra-core-ffi` native binding modules and product crates consume only
RAII wrappers and owned snapshots.

The required Rust FFI safety tests are:

- `native_memory_binding_is_feature_gated`
- `native_core_context_drop_releases_c_mem_context`
- `native_search_scope_drop_releases_c_scope`
- `native_batch_scope_drop_releases_c_scope`
- `native_memory_leak_report_maps_to_diagnostic_material`
- `owned_snapshot_survives_scope_release`
- `borrowed_view_cannot_escape_scope`
- `ffi_build_variant_rejects_kick_evidence_count_above_c_limit`
- `ffi_build_variant_copies_kick_evidence_to_owned_vec`
- `ffi_build_variant_preserves_hold_branch_kind`

The completion criteria are: default build keeps native binding unavailable,
native-memory-binding feature uses RAII, no borrowed view escapes scope, and
malformed pointer/count rejected before deref. `CBuildVariantView` must check
`kick_evidence_count` against `C_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT` before
checking or reading the pointer, then copy valid evidence into an owned `Vec`.

## T3 Coverage / Probability Invariant Tests

Product probability is always measured from a `PatternBitSet union`; variant
probabilities are never summed directly. The required invariant tests are:

- `coverage_row_rejects_universe_mismatch`
- `coverage_row_rejects_weight_model_mismatch`
- `coverage_row_rejects_piece_source_mismatch`
- `coverage_union_does_not_sum_variant_probability`
- `setup_probability_uses_pattern_bitset_union`
- `build_coverage_uses_union_probability`
- `spin_probability_uses_pattern_bitset_union`
- `score_does_not_change_coverage_probability`
- `observed_queue_truncation_not_renormalized`

T3 closure requires: probability never exceeds 1.0, the same pattern covered by
multiple variants counted once, score-aware objective does not modify
probability, and observed queue truncation keeps `probability_complete=false`
with `renormalized=false`.
Rows with a nonzero `piece_source_id` must share that source before they can be
OR-unioned; matching only `pattern_count` is not a valid coverage identity.

T3 marker phrases: same pattern covered by multiple variants counted once;
score-aware objective does not modify probability.

## T4 Product E2E / Golden Tests

User-facing output is guarded by product fixtures and marker goldens rather than
by brittle full-stdout byte snapshots. The T4 fixture set is:

- `pc_2l_fixed_queue`
- `pc_4l_fixed_candidate_budget`
- `scenario_clear_to_empty`
- `path_representative`
- `percent_uniform_bag`
- `cover_template_basic`
- `continue_token_basic`
- `rules_verify_basic`
- `render_capability_exact`

The golden surface spans `tests/golden/product/*.json`,
`tests/golden/ux/*.txt`,
`tests/golden/render/*.json`, and `tests/golden/diagnostics/*.json`. Product
JSON goldens pin stable contract fields, UX text goldens pin concise human
summary markers, render goldens pin exact capability and encoded pixel hashes, and
diagnostics goldens pin code,
severity, suggested next step, and evidence.

T4 closure markers: `json_contract_stable`,
`text_output_human_summary_stable`, `diagnostic_output_contains_evidence`, and
`unsupported_features_show_disabled_reason`.

## T5 Security Regression Tests

Security fixes are release blockers, not TODO-only documentation. T5 keeps the
specific regression tests that cover the current memory, FFI, GPU, render, GUI,
WASM, and WebGPU boundary risks:

- `memory_context_double_release_does_not_deref_freed_memory`
- `ffi_kick_evidence_count_exceeded_rejected_before_pointer_read`
- `gpu_worker_missing_memory_ticket_rejected`
- `gpu_buffer_release_without_fence_rejected`
- `gpu_unconfirmed_probability_rejected`
- `runtime_raw_svg_rejected`
- `gui_subprocess_forbidden`
- `wasm_user_shader_rejected`

The architecture task `T5 Security Regression Tests` remains available through
`Validate`. `ReleaseAcceptance` begins with `NoProductDebt`, which executes the
current architecture validation set before its runtime debt probes. These
checks form the closure:
`security_regression_tests_are_part_of_Local_or_Strict_gate` and
`release_acceptance_cannot_pass_when_security_regressions_fail`.

## M13 CPU BuildUp Verifier

M13 adds the CPU BuildUp verifier that decides whether a `PackingCandidate` is an
actual buildable solution candidate. Packing remains a static operation-set
enumeration; only BuildUp-verified rows become `BuildVariant` rows.

The verifier is implemented in `core-c/src/buildup` and keeps each rejection
reason in a narrow file:

- `buildup_worker.c` orchestrates verification and searches remaining
  operations with failed-state memoization.
- `buildup_order_dag.c` validates representative operation order as a priority
  hint permutation.
- `line_clear_dependency.c` rejects placements invalidated by current board and
  line-clear dependency.
- `y_adjustment.c` applies conservative line-clear y adjustment using
  `ClearraLineClearState.deleted_row_mask`; each operation cell moves down by
  the count of deleted original rows below that cell, rather than by total
  cleared line count.
- `grounded_filter.c` rejects floating placements before reachability.
- `reachability_bridge.c` compiles `problem->rule` through
  `clearra_reachability_kick_table_from_rule` and passes the resulting
  `ClearraReachabilityKickTable` to the C reachability checker. It must not
  pass a null kick table for SRS, SRS+180, or imported verified kick profiles.
- `hold_queue_verifier.c` verifies queue order, hold decision, and bag pattern.
- `build_variant_buffer.c` stores only accepted BuildUp results.

The public C contract exposes `clr_buildup_worker_verify`,
`clr_buildup_worker_verify_into_buffer`, `clr_buildup_verification`, and
`clr_build_variant_buffer`. Rejection statuses include queue order impossible,
hold disabled impossible, bag pattern impossible, line-clear y adjustment
impossible, SRS reachability impossible, piece window impossible, and goal not
satisfied.

M13 fixtures live in `tests/fixtures/buildup` and are mirrored by
`core-c/tests/buildup_tests.c`: packing possible but queue order impossible,
packing possible but hold disabled impossible, packing possible but line clear y
adjustment impossible, packing possible but SRS reachability impossible,
NoKick/SRS/SRS+180/imported verified reachability bridge kick tables,
representative order hint is priority not single path, and valid packing plus
valid BuildUp. The positive fixture keeps the marker valid packing plus valid
BuildUp.
BuildUp을 통과한 결과만 BuildVariant가 된다.

## M14 Coverage Row Bridge

M14 bridges accepted `BuildVariant` rows into Rust coverage rows without moving
probability ownership into C. The C side exposes `clr_coverage_row_view`,
`clr_pattern_bitset_c`, `clr_coverage_row_from_build_variant`,
`clr_coverage_union_rows`, and `clr_coverage_overlap_count`. These functions
only build dense row views, check pattern universes, OR bitsets, and report
overlap.

`clearra-core-ffi/src/buildup/coverage_row_view.rs` mirrors the C layout as
`CCoverageRowView` and `CPatternBitSet`. `clearra-coverage` reads the row through
raw words with `coverage_row_from_raw_words_with_identity`, rebuilds a typed
coverage row with `CoverageRowKind`, `pattern_universe_id`, and
`pattern_weight_model_id`, and keeps the universe mismatch and probability guard
invariants. Product probability and objective reducers consume
`TypedCoverageMatrix`; the untyped matrix is confined to internal algorithm
tests and is not a production probability path.

The bridge rules are:

- C coverage row view can be read from Rust.
- PatternBitSet universe checked.
- coverage row candidate id stable.
- OR union works.
- probability never exceeds 1.0.

Final probability and objective selection remain in Rust `clearra-coverage` and
`clearra-objectives`; C coverage files must not compute probability.

## M15 Rust Coverage / Objective Reducer

M15 turns C core row output into Clearra probability and objective results in
Rust. `clearra-coverage` owns `CoverageProbabilityReducer`, which reduces a
`TypedCoverageMatrix` with `PatternBitSet` OR union before calling
`union_probability`. The typed matrix carries row kind, pattern universe id, and
weight model id, so setup/build/spin/score-cell rows cannot be unioned merely
because their pattern counts match. Variant coverage is not summed, family probability uses OR union, and duplicate variant rows cannot push probability above 1.0.

`clearra-objectives` owns `ObjectiveReducer`. It consumes coverage candidates
with stable canonical keys and returns:

- all candidate ids through `AllCollector`
- unique candidate ids through `UniqueCollector` keyed by stable canonical key
- minimum cover through the typed coverage matrix solver
- total solution count and retained trace count as separate fields

M15 keeps the handoff's probability rule intact: C rows are input evidence,
while final probability, objective selection, and count/trace separation remain
Rust-owned.

## M16 Replay and Output Bridge

M16 converts a BuildUp-verified build variant plus representative order into a
canonical `clearra-replay` trace. `BuildVariantReplayInput` carries the variant
id, initial board, operation list, representative order, and representative/sample
trace marker. `ReplayEngine::build_variant_to_trace` produces `ReplayTrace`
through `SolutionTraceBuilder`, preserving placement steps, colored cell ownership,
and line clear events.

Output remains an envelope and dispatch boundary. `clearra-output` consumes
`ReplayTrace` and renders it as text or typed JSON directly; fumen-like pages are
delegated to `clearra-fumen` through its typed replay adapter:

- `TextWriter::replay_trace`
- `JsonContract::from_replay_trace`
- `JsonWriter::write_replay_trace`
- `clearra-fumen::FumenLikeWriter::write_replay_trace`
- `RenderFormatDispatcher::render_replay_trace`

`clearra-fumen` owns the fumen-like reader, writer, trace codec, transforms, and
replay adapters. `clearra-render` owns `RenderScene`, skin manifests, decoded
PNG atlas pixels, replay frame rasterization, PNG/GIF encoding, and render
capability reports. PNG/GIF report `supported=true` and `render_exact=true`.
The exact claim is guarded by board, replay lock-frame ownership, and timeline
byte-hash goldens. The default skin is the importer-generated PNG atlas plus
manifest/provenance under `assets/skins/default`. Runtime raw SVG rendering is
forbidden; SVG exists only in the feature-gated build-time asset tool.
`clearra-output` must not grow raw fumen codec,
fumen transform, skin, atlas, or bitmap rendering implementation files.

`ReplayTrace` carries `representative=true` and `sample=true` when the trace is
a representative retained sample rather than an exhaustive trace set. Colored
cell ownership is exported as structured replay evidence, and line clear event
payloads are preserved in both event and step views.

## Build Script Policy

The handoff lists build.rs as an optional top-level build integration point.
The current repository policy intentionally does not use it for the root or
standard Rust workspace.

C core build integration belongs to:

- CMakeLists.txt
- core-c/CMakeLists.txt
- scripts/lib/core-c-tests.ps1
- scripts/clearra.ps1

Cargo build scripts are forbidden in the standard workspace because they add
short-lived executable launch surfaces to ordinary verification. The
only exception is the isolated, root-workspace-excluded Tauri package at
`apps/clearra-desktop/src-tauri`; Tauri requires `tauri-build`, and the explicit
DesktopHost gate uses the same external canonical Cargo target as all other
tasks. Its build script generates only Tauri application context; CMake and the
native link path remain runner-owned. The gate records Windows Device Guard
before creating a generated process surface. Enforced UMCI makes source work
compile-only and returns
`E_WINDOWS_GENERATED_EXECUTION_REQUIRES_APPROVED_PACKAGE`; an approved release
artifact is signature-checked before its single launch attempt. An actual
Windows error 4551 caused by a later policy verdict becomes
`E_WINDOWS_LOCAL_SOURCE_BUILD_BLOCKED`.
Default product gates never invoke WSL or replace native evidence with a WASM
process. Release-built Windows packages and the browser WASM artifact are
independent product surfaces.

Source runners never inject `/MANIFESTUAC` or `/MANIFEST:EMBED` into transient
Cargo/C artifacts. The release package is the sole owner of the reviewed PE
execution-level manifest and its signature; unpackaged source tools use the
ordinary compiler process surface and never manufacture package trust.

An explicit WSL execution request is a separate native Linux surface. The host
runner streams source into a persistent ext4 workspace; WSL Cargo, C compilation,
runtime, and cache paths remain below the Linux filesystem and reject `/mnt/*`
source trees. Host-preserving `auto` selection never turns Windows failure into
WSL success or WSL failure into Windows success. WASM preparation may also be
explicitly assigned to this Linux build surface. The completed `.wasm` is then
streamed as bytes to deployment tooling; the browser runtime itself does not
depend on WSL.

The runtime is resolved before application-control authorization. A Windows
generated-executable check is authorized only for the `windows` runtime;
explicit `wsl` and `wasm` execution retain their own process, artifact, cache,
and failure contracts and cannot be reclassified as Windows success.

## M17 Core Executor

M17 makes `clearra-core-executor` the Rust orchestration facade over the exact
search backends. `CoreExecutor` remains the compiled-`SearchProblem` router:
PC-like presets go to `PcService`, and build coverage presets go to
`CoverService`. Setup finder queries have a different fixed-input contract and
enter through `SetupAppCommand -> WasmSetupSearchBackend`; a legacy
`SearchProblemPreset::Setup` is rejected rather than routed to another setup
implementation. CLI code must not call clearra-core-ffi, `core-c`, packing
runners, or backend internals directly.

The executor flow is:

`SearchProblem -> C PackingProblem -> C PackingResult -> C BuildUpResult -> CoverageRows -> Rust ObjectiveResult -> Rust OutputModel`

The concrete Rust split is:

- `service/pc_service.rs` owns PC execution orchestration and summary fields.
- `service/cover_service.rs` and `service/percent_service.rs` own the remaining
  non-PC compiled-problem entry surfaces.
- `backend/wasm_setup_search_backend.rs` owns cooperative setup session
  execution; `backend/wasm_cpu/setup_finder.rs` owns the exact setup algorithm.
- `packing/packing_runner.rs` owns `CPackingProblemBuilder` conversion and the
  C packing result boundary.
- `buildup/buildup_runner.rs` owns `CBuildUpProblemBuilder`, C BuildUp result
  rows, C coverage row views, Rust coverage rows, and `ObjectiveReducer::reduce`.
- `backend/backend_selector.rs` remains the only backend selection owner, while
  `backend/backend_kind.rs` and `backend/backend_fallback.rs` expose report
  vocabulary.

`clearra-core-ffi` owns the native C ABI boundary. Its `raw/bindings.rs` module
declares the raw `unsafe extern "C"` bindings for `clearra_core_abi_version`,
`clearra_geometry_catalog_compile`,
`clearra_geometry_exact_cover_search_graph`,
`clearra_geometry_solution_graph_stream_buildable_task`, and
`clr_buildup_worker_verify_into_buffer`; the `native` module then exposes safe
wrappers through `CoreCNative`. The raw binding declares
`#[link(name = "clearra_core", kind = "static")]`; callers that enable
`native-c-core` must pass the built C library directory with target-owned rustc
native link flags. Windows uses
`CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="-L native=..."`; it must not put
that path in global `RUSTFLAGS`, because host proc-macro/build tools do not link
the C core. WSL-native builds use their Linux-target environment and ext4 cache.
`clearra-core-ffi` intentionally has no Cargo `build.rs`. ManagedLocal builds the
C library with `BUILD_TESTING=OFF` and performs no Rust workspace compilation;
Trusted execution is selected before Cargo can build or launch any generated
helper, test, or product artifact. The runner never reacts to a blocked process
by retrying, signing, unblocking, or substituting weaker evidence.

The canonical external Cargo target is reused across native runs. A C library
content change invalidates only the debug and release artifacts owned by
`clearra-core-ffi`; the C library hash must not be embedded in global Rust
`-C metadata`, because that would duplicate every dependency artifact rather
than refresh the native ABI owner. The UTF-8 native-link state marker lives
under `<CARGO_TARGET_DIR>/.clearra-state`, outside the repository.

Product execution consumes native C geometry and BuildUp status through
`CoreCNative::compile_geometry_catalog_with_cancellation`,
`CoreCNative::search_geometry_solution_graph`, and the BuildUp-filtered catalog
or graph stream. A non-native build has no solver
fallback: `NativeCoreError::Unavailable` becomes `E_NATIVE_CORE_UNAVAILABLE`,
`backend_selected=none`, `fallback_used=false`, and an unexecuted incomplete
resource report. Fixture witnesses and sample trace keys are test support only
and cannot be reached from AppRequest.

The executor summary fields come from service and runner boundaries. A
`PackingCandidate` is not a solution before BuildUp, final probability belongs
to Rust coverage/objectives, and the output model is
`CoreExecutionResult` / `SearchExecutionReport`.

## M18 CLI Product Path

The user-facing CLI route uses the typed application boundary. Product commands
follow this flow:

`args -> assembler -> clearra-app -> validation -> clearra-problem -> clearra-core-executor -> output`

`clearra-app` is the typed application facade shared by CLI, GUI, and downstream
programs. It owns typed command input, validation orchestration, problem
compilation, executor calls, render model construction, diagnostic aggregation,
language preference, file input policy, and effect/report models. It must not
own argv parsing, stdout/stderr output, text/json/fumen-like formatting, raw C
ABI calls, or GUI widget schema.

The command files stay thin. `pc`, `pc-scenario`, `path`, `percent`,
`failed-queue`, `setup`, `cover`, and `continue` assemble canonical typed inputs
and call `clearra-app`.
`clearra-cli` renders the returned `AppResponse` to stdout/stderr through
`clearra-output`; it must not call `CoreExecutor`, `ProblemCompiler`, or query
validators directly from production command handlers.

`cover` still assembles the canonical `BuildCoverageQuery`, but non-export
execution and native template JSON export are app commands. cover lowers
BuildCoverageQuery into BuildQuery for executor-backed coverage runs inside
`clearra-app`.
cover lowers BuildCoverageQuery into BuildQuery.

percent uses PercentQueryAssembler to interpret queue arguments into a
scenario-shaped `PcScenarioQuery`. The app command validates the queue contract,
compiles a `coverage-summary` SearchProblem, and calls the same exact executor
boundary as PC search. Observed queue expansion and failed-pattern calculation
belong to the typed app/executor path, not to the CLI command file. The browser
WASM command parser exposes the same percent arguments and does not route through
a native-only service.

The default one-line percent query separates geometry from supply observation:
it places exactly one piece into the four-cell gap while `--min-len` controls the
materialized source window. Native `ScenarioPc + coverage-summary` execution
routes to `PercentService`; opening-PC coverage summaries remain on `PcService`
until the percent service explicitly supports that preset. Conflating the source
window with the exact placement count makes otherwise valid multi-piece percent
queues fail at the native packing boundary.

`failed-queue` is a reverse-search output policy over that same exact coverage
result. It returns the complement of the covered `PatternBitSet`, never runs a
separate forward solver, and fails closed when probability coverage is
incomplete. A materialization limit may bound displayed queue strings, but must
not change the exact failed count or complement probability.

Process-level E2E owns the product-route contract for `clearra pc --lines 2`,
`clearra pc-scenario --fixture tests/fixtures/pc/example.json`, `clearra pc-replay`,
`clearra percent`, `clearra failed-queue`, `clearra setup-finder`, and
`clearra build-coverage`.
`path`, `setup`, and `cover` remain input-only compatibility aliases and do not
adopt the semantics of identically named Sfinder commands.

## M19 Backend Policy and Fallback

M19 gives users an explicit compute backend policy without letting CLI own
solver selection. User-facing backend choices are `auto|cpu|gpu|hybrid`.
`auto` is the default. `cpu` is the confirmed baseline. `gpu` and `hybrid` may
fall back only when fallback is explicitly allowed or implied by `auto`; any
fallback must leave a reason in diagnostics and output.

The CLI only parses and assembles execution policy fields:
`--backend auto|cpu|gpu|hybrid`, `--gpu-device auto|N`,
`--allow-backend-fallback`, `--no-backend-fallback`, `--max-candidates N`,
`--max-patterns N`, and `--max-memory-mib N`. Validation checks whether the
requested backend and budgets are supported before execution. Backend selection
and fallback reporting belong to `clearra-core-executor`.

Every PC executor result that can reach output must include the M19 backend
contract fields: `backend_requested`, `backend_selected`,
`backend_fallback_reason`, `gpu_confirmed`, `cpu_confirmed`,
`candidate_backend`, and `buildup_backend`. `backend_selected` reports the
actual compute surface. CPU uses serial or partitioned Bitset Algorithm X over
the immutable geometry catalog. GPU uses the layered executor for the same
canonical exact-cover state graph. A connected WebGPU execution is reported as
`gpu`; `hybrid` means GPU-only when an explicitly prepared GPU session exists
and CPU-only otherwise. It never means running two complete searches and
merging their results. When the
adapter, kernel, or requested device is unavailable, an explicitly authorized
CPU fallback reports the exact failure reason; without fallback permission the
request fails.

`candidate_backend` and `buildup_backend` are separated because packing
candidate generation and BuildUp verification may run on different compute
paths. GPU confirmation must therefore be explicit instead of inferred from a
requested backend string. A non-native local build returns
`E_NATIVE_CORE_UNAVAILABLE`, `backend_selected=none`, and no solver result. A
native build that actually executes C CPU hot paths reports `candidate_backend=cpu-packing`,
`buildup_backend=cpu-buildup`, and `native_c_core_executed=true`.

## M20 Setup Finder Product Path

Setup finder searches common playable prefixes of the empty 10x4 PC solution
family. It does not generate arbitrary intermediate boards and does not cut
prefixes from a materialized list of complete solutions.

The product flow is:

`remaining-piece multiset + selected initial hold -> admissible 10-lock signatures -> inverse lock-clear geometry family -> FamilyQuotient Partial BuildUp -> lazy hold/pattern product -> forward/backward coverage -> setup-shape union`

The remaining-piece count determines the PC cycle. Product UI requests use one
empty initial-hold condition. The CLI may explicitly select one occupied
initial hold; that piece must be included in the remaining inventory and is
removed from the queue remainder. The engine never expands a unique residue
into one search per possible held piece. Cycle-reset borrowing is a provenance
policy and is available only for cycle seven.

`FamilyQuotient Partial BuildUp` consumes canonical placement rows from the
compressed `Append / Union / Product` geometry DAG. Each partial node owns its
current board, inverse lock-clear state, and residual completion family. Nodes
with equivalent exact continuation state union their residual families before
the next layer is expanded. The root is only the search source. Every live
non-root prefix from one through ten placements is represented; depth is not a
pruning proof. The request's `max_setup_pieces` field selects result depths from
1 through 10 after exact coverage. Its product default is 9 so complete PCs do
not dominate probability ordering, while callers may select 10 to expose
terminal PC solutions.

Queue-based setup mode keeps the normal unordered cycle residue and conditions
the next bag on a distinct observed piece group. The group's letter order does
not fix draw order. Observed pieces are available to partial BuildUp but are not
mandatory setup locks.

An independent optional terminal supply target may be used in either shape
oracle or queue-based mode. The target is the multiset union of the terminal
hold and the unconsumed standard-bag suffix. Its required cardinality is
derived from the current cycle, and one duplicated piece kind is permitted
only as hold carryover. The compiler preserves the broad source universe,
derives compatible patterns from the terminal constraint, and retains the
original global pattern IDs and weights. Filtered probability is never
renormalized. Observed QB conditioning and terminal supply filtering may be
combined without changing either contract.

Coverage is evaluated on the exact product state:

`partial node x hold state x queue cursor/extra draw x pattern word`

The forward value is the set of patterns that can build the state. The backward
value is the set that can still reach a complete PC. The required equation is:

`JointCoverage(state) = ForwardCoverage(state) AND BackwardPcLiveness(state)`

The intersection is accumulated within one exact fixed-tiling partial state.
States with the same visible board but different canonical placement rows or
deleted-row state are not coverage-merged. After evaluation and ranking, one
exact state is selected for each visible board. Intersecting `OR(forward)` with
`OR(backward)` after board grouping is forbidden because it can combine
incompatible temporal states and different tilings.

Each visible candidate reports build, joint, and conditional probability plus
an exact representative placement/hold path. Its placement-count range is
derived only from PC-live exact states that satisfy the active supply contract.
Output limits are applied only after all exact-state coverage has been accumulated
and ranked. Queue knowledge is an explicit product policy:

- `FullQueueOracle` allows each complete pattern to choose its own legal path.
- `VisibleSeven` groups queues by the current hold and visible seven-piece
  prefix. Every queue in one observation class must choose the same action,
  and the policy may branch only after another piece becomes visible.

The visible-seven evaluator runs over the exact placement language and the
hold/bag automaton. It is not a truncated pattern bitset or a representative
queue approximation. Oracle remains the default for compatibility.

The only product entry is:

`CLI or web command -> SetupAppCommand -> validation -> WasmSetupSearchBackend -> SetupFinderReport`

The old `SetupSearchService`, deterministic shelf packing, and per-candidate
post-PC continuation path do not exist in the product source.

## X3 Setup Finder Result Contract

The selected supply condition reports its identity, optional CLI-selected
initial hold, materialized pattern expression, pattern count, total candidate
count, truncation state, and ranked candidates. Each candidate reports an
opaque exact-state ID, setup board mask, minimum and maximum PC-live placement
count, build and joint covered pattern counts, build/joint/conditional
probabilities, and one legal representative path.

The representative path proves that the setup itself is buildable. On-demand
`solution_paths` have a different contract: every returned path starts with the
same fixed tiling identified by the card's board, deleted rows, and canonical
placement set, then contains only the remaining placements that complete a
perfect clear. The product UI renders the setup mask in the existing-field
color and the completion placements by piece.

The top-level report records the inferred cycle, canonical residue, cycle-reset
borrow policy, geometry family count, partial graph node count, completeness,
`queue_knowledge`, `visible_piece_count`, and one of
`coverage_semantics=full-future-oracle` or
`coverage_semantics=visible-seven-policy`. A resource or allocation failure is
an error; it must not return a complete-looking partial result.

## M11 / M21 Build Coverage Product Path

M11/M21 build template coverage uses the C BuildUp coverage-row boundary.
The CLI still assembles and validates the canonical `BuildCoverageQuery`, but
execution now keeps the full template/domain/constraint contract until the
executor reducer consumes C-produced coverage rows.

The build coverage flow is:

`BuildTemplate -> SlotDomain -> SlotAssignment -> BuildUpProblem -> C BuildUp -> CoverageRow -> CoverageMatrix -> UnionProbability`

Native JSON template import remains owned by `clearra-build-coverage` and
produces `BuildTemplate`; raw fumen-like text must still be converted by
`clearra-fumen` reader/adapter code before build coverage sees it. Slot domains are solved
through `AssignmentExactCoverBridge`, with `AssignmentCsp` kept as the local
assignment contract/fallback, and `CoverService::execute_build_coverage` runs
`PackingRunner::run` plus `BuildUpRunner::run` before reducing
`C BuildUp coverage row` values.

Each assignment must have a C BuildUp coverage row. Build coverage must not
reuse one C row across multiple assignments, treat missing rows as successful
zero coverage, or derive probability from assignment count. C coverage row
identity validation rejects row kind, pattern universe, weight model, and
pattern count mismatches before rows enter `BuildCoverageMatrix`.

`BuildCoverageResult uses union probability`: assignment or build variant
probabilities must never be summed directly. C coverage rows are converted to a
`CoverageMatrix`, OR-unioned by `BuildUnionCoverage`, and weighted through
`UnionProbability`.

## M22 Rules / Kicks Runtime

M22 connects Rust rule/kick ownership to the C compact runtime rule surface.
`clearra-rules` remains the owner of profile definitions, builtin rules, kick
import/export, and kick verification. C runtime code in `core-c/src/rules`,
`core-c/src/reachability`, and `core-c/src/candidate` consumes only compact
descriptors; it must not parse raw kick files or decide whether an imported
profile is verified.

The rules/kicks flow is:

`RuleProfile + optional VerifiedKickTableProfile -> RuleDescriptorCompiler -> clr_rule_profile_descriptor -> clearra_rule_profile_from_descriptor -> ClearraCompactRuleProfile`

SRS, SRS+, Jstris 180, and NoKick compile to built-in C descriptors using
`CLR_KICK_SRS_90`, `CLR_KICK_SRS_PLUS_180`, `CLR_KICK_JSTRIS_180`, and
`CLR_KICK_NO_KICK`.
Imported kick tables compile only through `VerifiedKickTableProfile`; the
verified transitions are copied into the descriptor as compact piece/rotation
offset sequences. Unverified extension profiles such as SRS-X are rejected by
`RuleDescriptorCompiler` before `CPackingProblemBuilder` can produce a C
problem, so C execution never receives an unverified profile.

The C runtime accepts a descriptor with `has_verified_kick_profile` only when
the descriptor includes a bounded verified transition table. It then constructs
the same `ClearraCompactKickTable` surface used by reachability and candidate
generation. This keeps Rust import/verification policy separate from C hot-path
reachability while preserving kick first-success ordering.

## X6 MVP3 Board128 / Board256 / Wide Board

X6/G3 prepares board backends beyond the Board64 fast path without pretending the
generalized runtime is complete. `Board64` remains unchanged for board
descriptors with 64 cells or fewer. `Board128` covers 65 to 128 cells,
`Board256` covers 129 to 256 cells, and both expose fixed-word basic board
operations. `Wide` is the dynamic-word validation and metadata path above 256
cells until its search runtime is fully connected.

For the standard 10-wide PC product, 1..6L continues to use the existing
Board64 request and executor contract. A separate extended request contract
accepts 7..24L and owns canonical little-endian board words, selecting Board128
for 7..12L and Board256 for 13..24L. Both contracts name the same inverse
lock-clear skeleton exact-cover algorithm. Extended packing, BuildUp,
reachability, and replay must all consume the extended layout identity before
the product can report connected exact execution; partial connection remains an
explicit unsupported capability and never falls back to Board64.
The extended state vocabulary uses a checked `u32` deleted-row mask and a
checked `u64` operation bitset for at most 60 tetromino placements. Bits outside
the compiled query layout are invalid rather than ignored.

The C ABI owns the backend dispatch surface through `clr_board_descriptor`,
`clr_standard_pc_extended_board_descriptor`, `clr_board_backend_capability`,
`clr_board128_descriptor`, `clr_board256_descriptor`,
`clr_wide_board_descriptor`, `clr_generic_board_mask`,
`clr_board_dispatch_row_mask`, and `clr_board_operation_mask_from_cells`.
`core-c/src/board/board_backend_dispatch.c` chooses the backend and reports
capability; `board128.c` and `board256.c` own fixed-word basic ops; and
`wide_board.c` owns Wide descriptor validation. The generic row mask path
(`generic row mask`) can be dispatched for Board64, Board128, Board256, and
Wide descriptors. Generic operation masks are supported for the fixed-word
backends, while Wide operation masks
return `CLR_BOARD_UNSUPPORTED_BACKEND` so unsupported board width silent
fallback forbidden remains an executable contract. This keeps the marker
`unsupported board width silent fallback forbidden` tied to code and tests.

Backend capability is explicit. Board64 reports `runtime_connected=true` and
`packing_supported=true`. Board128 and Board256 report `descriptor_supported=true`,
`basic_ops_supported=true`, `operation_mask_supported=true`, and
`packing_supported=false` with `board_backend_not_connected`. Wide reports
`descriptor_supported=true`, `operation_mask_supported=false`, and
`packing_supported=false` with `wide_board_runtime_not_connected`. Invalid or
out-of-scope board dimensions report `board_width_out_of_scope` instead of
falling back to Board64.

The Rust geometry metadata bridge lowers compact `SearchProblem` board metadata through
`backend_kind_for_size` into `CBoardDescriptor.backend_kind` and
`CBoardDescriptor.cell_count`. The distinct
`CStandardPcExtendedBoardDescriptor` bridge owns all four canonical initial-board
words for 7..24L, so the legacy two-word descriptor cannot become an accidental
lowering path. This bridge is intentionally metadata-only:
runtime validation still rejects Board128/Board256/Wide search paths until placement,
packing, BuildUp, and replay can operate on those backends end to end.

G3 contract markers: Board128 descriptor; Board256 descriptor; WideBoard descriptor; generic row
mask; generic operation mask; C board backend dispatch; Rust geometry metadata
bridge; Board64 fast path unchanged; Board128 basic row mask collision place
tests pass; Wide board runtime not connected reports reason; unsupported board
width silent fallback forbidden.

## G4 MVP3 Generic Operation / Candidate / Reachability

G4 keeps the stable C/FFI operation ABI limited to the connected standard
tetromino runtime. `ClearraOperationTable` contains the existing 28-operation
I/O/T/S/Z/J/L table, and `standard_operation_table_unchanged` means the Board64
path still uses the same four-rotation, area-4 operation ids and masks.

Custom and generalized operation inputs use `CustomPieceOperationTable` and
`GenericOperationTableDescriptor`. The descriptor carries board-independent
operation metadata: `piece_definition_id_fingerprint`,
`piece_area_multiset_fingerprint`, `rotation_state_count`, `operation_count`,
`operation_table_version`, and `operation_mask_word_count`. The Rust registry
bridge can build this descriptor from custom piece schema, and
`custom_operation_table_schema_validates` is a schema claim only. It remains in
the Rust extension/validation layer and is not mirrored into C ABI. Unsupported
custom operations stop before candidate or reachability execution.

Cache identity remains part of the contract. `operation_table_version`,
`piece_definition_id_fingerprint`, and `piece_area_multiset_fingerprint` must be
included so custom tables do not share standard cache entries. G4 forbids
distorting custom pieces into the standard four-rotation assumption, forcing a
Board128 operation mask into one `u64`, or replacing piece-specific kick
profiles with a piece-independent fallback.

G4 contract markers: ClearraOperationTable; CustomPieceOperationTable;
GenericOperationTableDescriptor; custom_piece_runtime_not_connected;
cache_key_includes_operation_table_version.

## G5 MVP3 Area Multiset Feasibility / Area Decomposition

G5 makes area pruning explicit as a necessary condition before expensive search.
The standard tetromino fast path is separated from generic custom/mixed area
logic: `StandardTetrominoAreaRule` owns the area-4 rule and
`standard_area4_fast_path_unchanged`, while `AreaMultisetFeasibility` owns the
active piece area multiset and a bounded subset-sum check. Generic feasibility
uses `active_piece_area_multiset`; it must not decide custom or mixed-piece
feasibility from `missing_cells % 4`.

Area scopes are part of the contract. `AreaScopeDescriptor` names
`TargetRows`, `InterpretedTargetCells`, and `WholeBoardTarget`; whole-board
scope is allowed only when the whole board is truly the target region. The
problem compiler owns `CompileAreaPruner`, which accepts explicit scope,
component areas, and an `AreaMultisetFeasibility` value. It returns
`RejectAreaInfeasible` or `SearchMayContinue`; it never returns solution-found.

Exact-cover and validation consume the same boundary. `AreaMultisetExactCoverBridge`
can reject area-infeasible components before DLX/ExactCover, and
`AreaFeasibilityValidator` reports `EAreaInfeasible` or
`IAreaNecessaryConditionPassed` with evidence
`area_feasible_is_solution_found=false`. Area feasible means only that search may
continue, not that a packing, BuildUp, or coverage row exists.

G5 contract markers: StandardTetrominoAreaRule; AreaMultisetFeasibility;
active_piece_area_multiset; bounded_area_subset_sum; AreaScopeDescriptor;
scenario_area_pruner_requires_explicit_area_scope;
area_decomposition_is_necessary_condition_not_solver;
area_multiset_feasibility_uses_piece_area_multiset.

## X7 MVP3 Exact-Cover / DLX Generalization

X7 strengthens exact-cover as a reusable tiling layer for setup and custom-piece
work without making it a replacement for queue/hold/reachability search. The
solver row remains `ExactCoverCandidate`, while interpreted tiling inputs use
`GenericExactCoverCandidate` to carry candidate id, stable piece id, piece area,
source cells, and compact DLX columns.

G6 generalizes the problem schema around the same solver. `ExactCoverProblemSchema`
is the typed source for exact-cover rows: it carries `cell_universe`,
`PieceUsageConstraint`, `SlotConstraintColumn`, `AreaConstraintColumn`,
required columns through `ExactCoverColumnKind::Required`, optional conflict
columns through `ExactCoverColumnKind::Optional`, and `ExactCoverCandidateRow`
records. `ExactCoverProblem::with_optional_columns` lowers the schema to the
DLX row format while preserving `required_column_count` and
`optional_column_count`; optional columns are conflict columns, not completion
requirements.

`CellUniverseBuilder` owns sparse cell universe construction. It maps absolute
board cells into compact DLX columns so setup shapes with sparse masks and
custom-piece cell sets do not leak absolute bit indexes into the solver.
`PieceAreaConstraint` rejects area-infeasible shapes before expensive DLX search
by checking whether the available piece-area multiset can compose the target
cell count.

The C/Rust bridge stays explicit: `DlxBuildUpBridge` maps a DLX
`ExactCoverSolution` plus interpreted operation candidates into a
`CPackingCandidate`, then delegates to `CBuildUpProblemBuilder`. A DLX result is
therefore only a packing/tiling witness; BuildUp still decides whether it is a
valid build variant.

The handoff shape is fixed as `DlxSolution -> operation candidates -> BuildUpProblem -> C BuildUp`.
DLX solution is not a BuildVariant. Line clear, hold, queue, and reachability
remain BuildUp responsibilities; exact-cover must not assume they are finished.
`DlxSearchLimits` exposes `max_solutions` and `max_nodes`, and DLX reports
`complete`, `searched_nodes`, and `truncation_reason`. Truncated DLX output must
never be marked complete.

X7 completion markers:
- standard setup tiling still works
- custom piece tiling can be represented
- area infeasible shape rejected before expensive search
- DLX result maps to BuildUpProblem

G6 completion markers: `generic_exact_cover_candidate_schema_validates`,
`dlx_solver_returns_complete_flag`,
`area_infeasible_shape_rejected_before_search`,
`dlx_result_maps_to_buildup_problem`, and
`standard_setup_tiling_still_works`.

## G7 BuildUp Runtime Scope

The connected exact BuildUp runtime is Board64 with at most
`CLR_BUILDUP_MAX_OPERATIONS` operations. Stable C and Rust ABI surfaces do not
reuse that fixed array for Board128/Board256/Wide BuildUp. The standard 7..24L
contract requires a separate query-sized operation bitset and pointer-stable
operation/trace storage before its executor can report connected exact support.

`clearra_buildup_runtime_status_for_board` and
`clearra_buildup_operation_set_runtime_status` return
`CLR_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE` outside that implemented scope.
operation_count > 15 is guarded, not truncated. Board128/Wide BuildUp is
unsupported, Board256 follows the same guard, and none falls back to Board64.

G7 completion markers: `operation_count_above_runtime_limit_is_unsupported`,
`board128_buildup_guard_reports_unsupported`, and
`unsupported_buildup_scope_does_not_claim_solution`.

## G8 MVP3 Custom Rule Editor

G8 adds the custom rule editor contract without opening an unchecked runtime
path. The domain schema is `CustomRuleEditorSchema`, and it names the editable
surface explicitly: `rotation_states`, `spawn_rules`, `kick_transitions`,
`first_success_order`, `supports_180`, `piece_specific_overrides`,
`line_clear_policy`, and `lock_reachability_mode`.

Editor output is raw schema material until validation produces
`VerifiedCustomRuleProfile`. `CustomRuleVerificationReport` preserves the
failure reasons as typed counters: `missing_transition`,
`duplicate_transition`, `invalid_rotation`, `unsupported_piece`,
`unsupported_board_backend`, and `unsupported_runtime_feature`. Missing
first-success order is a validation error because kick order affects
reachability and spin evidence.

The FFI boundary is verified-only. `CustomRuleDescriptorCompiler` can compile a
`VerifiedCustomRuleProfile` into the compact C rule descriptor when the schema
is supported by the Board64/standard tetromino descriptor path, but raw editor
schema returns `unverified_custom_rule_rejected_before_execution`. Unsupported
custom rule surfaces report reasons; they are not mapped to SRS.

G8 completion markers: `custom_rule_editor_schema_validates`,
`custom_rule_verify_reports_missing_transition`,
`custom_rule_verify_reports_duplicate_transition`,
`verified_custom_rule_can_compile_to_descriptor_when_supported`, and
`unverified_custom_rule_rejected_before_execution`.

## G9 MVP3 Generic GPU

Generic/custom GPU execution is `Unsupported` in the default product. The
default C ABI, Rust FFI, and WebGPU crate do not expose a generic descriptor or
a GPU BuildUp subset. `GenericGpuDescriptor` remains only a capability id with
the reason `generic_gpu_descriptor_not_connected`.

Default-off experimental backend packages are outside the stable product ABI.
The product rejects generic GPU requests instead of altering
`ClearraGpuPackingBatchDescriptor`, truncating wider board masks, or reporting
an unavailable backend as connected.

## G10 MVP3 Custom Skin / Theme Editor

G10 adds the custom skin and theme editor contract without connecting raw user
assets to the runtime renderer. `CustomSkinThemeSchema` owns `skin_id`,
`palette_id`, `piece_mapping`, `grid_style`, `background`,
`line_clear_highlight`, `ownership_color_mode`, `export_limits`, and
`provenance`.

User-imported assets live only in a user config directory or user cache
directory. They are not repository assets and must not be displayed as built-in
assets. Every import requires a manifest and provenance/import report before
the GUI may show it as an editable theme.

Theme preview is PNG-atlas only. The editor and UI schema expose
`runtime_raw_svg_allowed=false`, `custom_theme_preview_uses_png_atlas`, and
`raw_svg_not_passed_to_runtime_renderer`; raw SVG preview is rejected instead of
being passed to the renderer.

G10 completion markers: `custom_skin_schema_validates`,
`custom_skin_import_requires_provenance`,
`custom_theme_preview_uses_png_atlas`, and
`raw_svg_not_passed_to_runtime_renderer`.

## G11 MVP3 Acceptance Gate

G11 defines the MVP3 acceptance gate that keeps generalization work from
changing the standard MVP1/MVP2 product path. The gate runs MVP1 ProductE2E
first, then MVP2 Acceptance, then MVP3 schema and guard gates: custom piece
schema tests, mixed bag schema tests, Board128/Wide descriptor tests, generic
operation guard tests, area multiset feasibility tests, DLX tests, unsupported
runtime guard tests, custom rule editor validation tests, generic GPU descriptor
tests, custom skin/theme editor tests, and Architecture validation.

The required acceptance summary is
`standard_fast_path_unchanged_under_mvp3=true`,
`custom_features_guarded_until_runtime_connected=true`,
`no_silent_fallback_to_standard_path=true`, and
`generic_cache_keys_include_piece_board_rule_supply_identity=true`.

MVP3 features are allowed to validate schema and report guarded capability, but
they must not execute as standard tetromino fallback paths. Custom unsupported
surfaces must return explicit disabled or unsupported diagnostics; custom
unsupported를 empty success로 처리 is forbidden. Standard and generic cache
identity must include piece, board, rule, and supply identity so standard와
generic cache key 충돌 cannot occur.

G11 marker summary: standard_fast_path_unchanged_under_mvp3.
G11 marker summary: custom_features_guarded_until_runtime_connected.
G11 marker summary: no_silent_fallback_to_standard_path.
G11 marker summary: generic_cache_keys_include_piece_board_rule_supply_identity.

## T7 MVP3 Acceptance Tests

T7 pins the MVP3 acceptance tests that keep generic expansion from polluting
the standard tetromino path. These tests are guard and identity tests, not a
claim that custom-piece or wide-board search runtime is connected.

The custom piece surface must keep
`custom_piece_schema_validates_but_runtime_guarded`. Custom piece schemas may
validate and expose stable metadata, but they remain guarded with explicit
runtime-disabled reasons until generic search is connected. The cache surface
must keep `generic_cache_key_includes_piece_definition_id` so custom and
standard piece identities cannot collide.

The generic area surface must keep `mixed_piece_area_multiset_feasibility` and
`missing_cells_mod_4_not_used_for_generic_feasibility`. Generic feasibility is
based on the active piece area multiset, not the standard tetromino
`missing_cells % 4` shortcut.

The board and supply surfaces must keep `board128_descriptor_tests`,
`wide_board_runtime_not_connected`, and
`custom_bag_not_silent_standard_fallback`. Board128 descriptors can validate
while search remains guarded; Wide boards must report runtime-not-connected;
custom bags must never silently become standard 7-bag execution.

T7 marker summary: custom_piece_schema_validates_but_runtime_guarded.
T7 marker summary: mixed_piece_area_multiset_feasibility.
T7 marker summary: missing_cells_mod_4_not_used_for_generic_feasibility.
T7 marker summary: board128_descriptor_tests.
T7 marker summary: wide_board_runtime_not_connected.
T7 marker summary: custom_bag_not_silent_standard_fallback.
T7 marker summary: generic_cache_key_includes_piece_definition_id.

## T Guarded Expansion Surface

Custom board, custom piece, custom bag, custom rule, exact scoring,
advanced render, WebGPU, and generic MVP2/MVP3 surfaces use one visible
`CapabilityState` vocabulary in UI schemas:
`Unsupported`, `ConnectedApproximate`, and `ConnectedExact`.
Only connected states may execute runtime work, and only `ConnectedExact` may
claim exactness.

Custom piece schemas can validate, but `custom_piece_schema_validates_but_runtime_guarded`
remains the runtime contract until generic execution is connected. Generic
cache identity keeps `piece_definition_id_fingerprint`,
`piece_area_multiset_fingerprint`, and `piece_set_profile` so custom piece
state cannot collide with the standard tetromino fast path.

Custom bag schema can validate, but `CustomBagRuntimeGuard` owns
`custom_bag_not_silent_standard_fallback` and reports
`custom_bag_runtime_not_connected`; custom bags must never fallback to standard
7-bag execution. Board128, Board256, and Wide descriptors stay separate:
Board128 covers 65..128 cells, Board256 covers 129..256 cells, and Wide covers
more than 256 cells; none is truncated into Board64.

Built-in SRS+ uses the pinned symmetric I-piece and transition-specific 180
kick table and reports `supports_exact_180=true`. Imported kick profiles may
claim exact 180 only after verification. `CustomKickExactnessGuard` rejects unverified custom
kick profiles with `unverified_custom_kick_rejected_before_c_execution` before
C execution.

## T8 Release Blocking Rules

T8 defines the conditions that must block release before an artifact is treated
as releasable. `ReleaseAcceptance` runs `NoProductDebt`, adversarial
correctness, C sanitizer, Rust exact tests, Product E2E, WASM build/test,
DesktopHost, and renderer goldens in that order. `NoProductDebt` executes the
current architecture validation set first. `GpuWorkerRelease` and
`WorkerRelease` include the same release path plus stricter GPU/worker gates.
This is the `release_blocking_rules_gate_release_acceptance` contract.

The release block list is intentionally explicit. Release is blocked when any
of these conditions regresses: `C memory double-release regression fail`,
`FFI pointer/count bound regression fail`, `coverage probability invariant fail`,
`silent GPU fallback detected`, `exact probability from unconfirmed GPU detected`,
`condition_summary field reintroduced`,
`renderer pixel/provenance golden regression`,
`custom piece silent fallback detected`, `GUI subprocess detected`, or
`raw SVG runtime rendering detected`.

Each block condition is pinned to a lower-level contract. T3 owns coverage
probability invariants. T5 owns C memory, FFI pointer/count, GPU unconfirmed
probability, GUI subprocess, and raw SVG regressions. T6 owns
`condition_summary` absence and renderer pixel/provenance exactness.
T7 owns custom piece/custom bag silent-fallback guards. U1/U3/U4/U5 keep GPU,
GUI, renderer, and asset-import boundaries visible and non-silent.

T8 marker summary: release_blocking_rules_gate_release_acceptance.
T8 marker summary: C memory double-release regression fail.
T8 marker summary: FFI pointer/count bound regression fail.
T8 marker summary: coverage probability invariant fail.
T8 marker summary: silent GPU fallback detected.
T8 marker summary: exact probability from unconfirmed GPU detected.
T8 marker summary: condition_summary field reintroduced.
T8 marker summary: renderer pixel/provenance golden regression.
T8 marker summary: custom piece silent fallback detected.
T8 marker summary: GUI subprocess detected.
T8 marker summary: raw SVG runtime rendering detected.

## X1 MVP2 Rule / Kick Expansion

X1 strengthens the MVP2 rule/kick surface without moving raw parsing into the C
core. `srs-plus` is a built-in exact profile and reports
`supports_exact_180=true`. Imported exact 180 behavior remains verification-gated;
`rules import` may report `supports_exact_180=true` and
`c_compact_descriptor_ready=true` only after `VerifiedKickTableProfile`
verification succeeds and the source rule is supported by the compact runtime.

Rule capability schema, rules CLI output, and rule editor schema expose the same
readiness vocabulary: `supports_exact_180`, `c_compact_descriptor_ready`, and
`unsupported_backend_reason`. `rules verify` reports missing transition,
duplicate transition, and unsupported annotation counts without treating an
unverified input as imported. `rules import` rejects unverified profiles, and
profiles outside the direct compact surface, such as SRS-X, ASC, and ARS, must disclose
disabled or unsupported backend reasons before execution.

## M23 Supply Runtime

M23 connects Rust supply ownership to the C compact PieceSource and multiset
window view.
`clearra-supply` remains the owner of raw queue parsing, normalization,
observed window analysis, ambiguity reports, and bag probability. C runtime code
consumes only compact descriptors; C core does not parse raw supply input.

The supply lowering flow is:

`PcQueueInput + HoldSlot + PieceWindow -> SupplyDescriptorCompiler -> clr_piece_source_descriptor + clr_piece_multiset_window + clr_hold_automaton_state + clr_piece_window_descriptor`

Fixed sequences compile to `C_PIECE_SOURCE_FIXED_QUEUE` and a bounded
`piece_multiset_window`. Bag-aligned patterns compile to
`C_PIECE_SOURCE_BAG_UNIVERSE`. Observed expansion remains Rust-owned: C receives
only the compact observed source descriptor and visible piece window.

The C supply surface is `core-c/src/supply/queue_view.c`,
`supply_state.c` and `piece_window.c`, declared by
`clr_supply.h`, `clr_piece.h`, `clr_piece_source.h`, `clr_hold_automaton.h`,
and `clr_problem.h`. Executor summaries must preserve the source identity
through `compact_supply_provenance_id`, `compact_piece_source_kind`, and
`compact_piece_multiset_count` so different supply sources cannot silently share
the same result identity.

The supply provenance and piece source pattern identity are part of the cache key:
`supply_provenance`, `queue_pattern_id`, `piece_window_start`, and
`piece_window_len` are mixed into the C cache identity before packing,
candidate, reachability, or BuildUp memo caches may be reused.

## M24 GPU Packing Backend

The stable Windows search GPU is the reviewed WebGPU geometry exact-cover
executor. It consumes the same immutable `GeometryCatalog` masks, piece kinds,
and support CSR as CPU Algorithm X. Its hot state contains only occupied cells,
packed used-piece counts, exact desired family counts, and depth. Queue, hold,
replay, spin, score, and Fumen state are forbidden.

Every dispatch is exact-reduced on the host with full state comparison after a
hash bucket hit. Deterministic first/middle/last parent samples are independently
expanded by the CPU reference at every dispatch boundary. Missing, duplicate,
or malformed children produce `RejectedTrustMismatch`; an unconfirmed graph is
never sent to BuildUp or probability. The reference samples validate trust and
do not duplicate the complete search.

The C native GPU ABI remains unavailable unless an independently implemented
native kernel is connected. No CUDA/OpenCL placeholder or CPU-as-GPU adapter is
registered. Fallback runs the CPU exact-cover executor as a fresh result and
preserves the original failure class and reason.

Explicit GPU warmup retains adapter, device, queue, and the reviewed compute
pipeline. When a concrete search accompanies the request, it also uploads the
immutable geometry catalog before the first dispatch. Dynamic frontier and
readback scratch are sized from the dispatch instead of being pessimistically
preallocated. Payload buffers that are completely overwritten before any read
are not host-zero-filled; read-modify-write counters are reset from a retained
zero template, and readback copies only the counter-confirmed retained prefix.
Reports distinguish warmup requested/performed from reuse of an earlier GPU
session.

## X2 MVP2 GPU Strengthening

X2 strengthens exact-cover execution without adding a second geometry
algorithm. GPU frontier buffers are segmented and their allocations transfer
directly into the immutable predecessor graph; reducer completion does not copy
the complete frontier or trace edge set. `gpu_result_deterministic` and the
runtime CPU sample confirmation represented by `gpu_result_cpu_confirmed` must
both hold before the result is trusted. `gpu_cpu_reference_match` records the
deterministic sampled transition match. Fallback and
`gpu_packing_unavailable_reason` remain visible.

X2 contract markers: GPU result deterministic; GPU result CPU-confirmed; CPU reference and GPU result match; fallback reason visible.

## M25 Hybrid Scheduler

M25 separates device preparation from execution. Explicit prewarm may retain a
device, queue, reviewed pipeline, and immutable catalog upload. Without prewarm,
catalog compilation and GPU connection may overlap. Once execution starts, one
geometry executor owns the request: prepared GPU for hybrid, otherwise CPU.
The same complete search is never run concurrently on both devices.

After geometry, immutable row-id paths are partitioned among CPU BuildUp
workers. Each worker owns only BuildUp scratch; all workers borrow the catalog,
PieceSource, and family graph. Backpressure counts graph traversal, BuildUp
workspaces, reducer memory, GPU readback, and result retention without copying a
candidate batch between stages.

Hybrid scratch buffers use `ClearraHybridScratch` allocated from a caller-owned
`ClrScope` via `clearra_hybrid_scratch_create`. Scheduler hot paths must not
use raw `malloc`, `calloc`, `realloc`, or `free`; CPU reference tables
and BuildUp variant buffers are scope-owned and released through the memory
epoch model.

M25 verification requires CPU-only result == hybrid result, GPU-only packing +
CPU BuildUp result == CPU reference, backend metrics reported, fallback reason
reported, and memory leak report clean. Executor output must expose
`hybrid_scheduler`, `hybrid_gpu_large_packing_batch`,
`hybrid_cpu_small_irregular_buildup`,
`hybrid_gpu_readback_cpu_buildup_overlap`, `hybrid_batch_buffer_reuse`,
`hybrid_memory_epoch_managed`, `hybrid_backend_metrics_reported`,
`hybrid_fallback_reason`, and `hybrid_memory_leak_report_clean`.

U2 Hybrid Scheduler Contract names the report surface explicitly:
`candidate_queue_len`, `candidate_queue_capacity`, `cpu_worker_backlog`,
`gpu_readback_backlog`, `gpu_batch_in_flight`, `backpressure_active`,
`deferred_batch_count`, `truncated_batch_count`, `memory_pressure_level`, and
`throttle_reason`. GPU owns only packing batch, readback, dominance prefilter,
and candidate hash work. CPU owns host reducer, exact confirm, BuildUp, coverage
row creation, and diagnostics. Memory pressure reduces batch size and emits
visible partial-result evidence; backpressure must not silently drop candidates.

M25 contract markers: CPU receives small/irregular BuildUp; batch buffer reuse;
GPU-only packing + CPU BuildUp result == CPU reference; fallback reason reported.

## M26 Percent / Path Product Slice

M26 connects Sfinder-style percent/path workflows to the Clearra output
contract without letting CLI own solver logic.

Percent follows:

`queue pattern universe -> multiset-grouped C Packing -> pattern-specific C BuildUp coverage rows -> PatternBitSet union -> weighted probability`

The percent command must report `total_pattern_count`, `covered_pattern_count`,
`probability`, `weighted_probability`, `coverage_probability`, and
`c_buildup_coverage_row_count`. Coverage probability is measured from the OR
union of covered pattern ids, not by summing variant rows. Percent uses the
`coverage-summary` output policy: normalized solution identities, solution-set
hashes, candidate digests, and replay traces are neither retained nor reported
as calculated. Failed queue examples are materialized only up to the explicit
output limit; the exact failed count is derived from the complete PatternBitSet.
The count is exact for the materialized universe; `failed_pattern_count_complete`
is true only when the universe itself is complete.

Path follows:

`SearchProblem -> C Packing / BuildUp -> representative replay -> retained trace -> output`

The path command must report a retained representative trace and keep
`total_solution_count`, `unique_solution_count`, `retained_trace_count`,
`solution_trace_count`, `count_complete`, `trace_retention_truncated`, and
`trace_retention_reason` as separate output fields. `retained_trace_count` is
never a substitute for total solution count.

M26 contract markers: percent reports total pattern count; percent reports
covered pattern count; percent reports probability; path reports retained
representative trace; path distinguishes retained trace from total count.

M26 exact markers: percent reports covered pattern count; path reports retained representative trace.

## X6 Path / Percent / Cover CLI

X6 strengthens the user-facing Sfinder-style CLI workflow across percent, path,
and cover without moving solver logic into CLI command handlers.

X6 marker summary: percent reports probability complete.
X6 marker summary: path reports representative trace.
X6 marker summary: cover reports union probability.
X6 marker summary: cover reports C coverage row count.

Percent follows the queue-pattern-universe path and must report total pattern
count, covered pattern count, and probability complete. If an observed universe
is truncated, the observed truncated universe is not renormalized:
`renormalized=false`, `probability_complete=false`, and a truncation reason are
visible.

Path follows the SearchProblem to C Packing/BuildUp route and then emits a
representative replay from a retained trace. The path command must report
representative trace availability, total solution count, unique solution count,
retained trace count, solution trace count, and trace retention state. Path
distinguishes retained trace from total count; retained samples are never shown
as all paths.

Cover follows `BuildTemplate -> SlotDomain -> SlotAssignment -> BuildUpProblem
-> C BuildUp -> CoverageRow -> CoverageMatrix -> UnionProbability`. X6 markers:
cover reports union probability; cover reports C coverage row count. The cover
command must report union probability, C coverage row count, and coverage row
identity validation. Slot assignment count is not success probability.

## X7 Fumen Transform / PNG / GIF Renderer

X7 promotes fumen-like transform and bitmap rendering as product surfaces while
keeping solver ownership boundaries intact. The fumen parser stays out of search
core: raw fumen text is decoded by `clearra-fumen`, normalized into typed page
models, and only then adapted to replay or build-template drafts. Search,
packing, BuildUp, coverage, and objectives must not parse raw fumen strings.

The fumen transform surface covers page decode/encode roundtrip, combine, split,
mirror, field mirror, grayout, remove comments, preserve comments, page shift,
ReplayTrace -> FumenLike output, FumenLike -> Replay adapter reading, and
fumen-to-build-template validates input. Transform identity is based on typed
pages and normalized solution keys, not raw fumen string equality.

PNG/GIF rendering is owned by `clearra-render`. The exact bitmap renderer takes
typed `RenderBoard` input and produces deterministic PNG board render golden,
PNG lock-frame render golden, after-clear/minos-crop frames, and GIF timeline
render golden bytes. It must not mutate BuildUp result, PackingCandidate,
CoverageRow, or any solver object. Runtime raw SVG rendering remains forbidden;
runtime assets remain PNG atlas + manifest + provenance.

The renderer reports export limits through `RenderExportLimits` and output
reports through `BitmapExportLimitReport`: renderer reports export limits,
maximum frame size, maximum GIF frame count, maximum delay, and timeline pixel
budget. The current runtime capability is `renderer_connected_exact`: PNG and
GIF are supported, exact, and carry no unsupported reason because both paths
produce validated bytes and pass pixel goldens.

`RenderCapabilityReport::current()` is the renderer capability source of truth.
`clearra-app` maps it into `AppResponse.capability_report.render_capability`, the
GUI host derives its display model from that report, and `RenderStatusPanel`
receives the typed capability as a prop. This establishes
`renderer_capability_matches_runtime_report` and
`render_status_ui_uses_product_capability`; frontend-local renderer status is
forbidden.

X7 marker summary: fumen parser stays out of search core.
X7 marker summary: PNG board render golden.
X7 marker summary: PNG lock-frame render golden.
X7 marker summary: GIF timeline render golden.
X7 marker summary: renderer reports export limits.
X7 marker summary: runtime raw SVG rendering remains forbidden.

## X8 GUI / Editor Schema v2

X8 gives the GUI a schema-first view of product state instead of letting UI
components infer solver meaning from labels. `GuiEditorSchemaV2` aggregates
`BackendOptionsSchema`, `ProblemPresetOptionsSchema`, `ScenarioEditorSchema`,
`SetupExplorerSchema`, `BuildEditorSchema`, `RuleEditorSchema`,
`ScoreEditorSchema`, `RenderOptionsSchema`, and `DiagnosticPanelSchema`.

The schema exposes backend auto/cpu/gpu/hybrid, fallback reason, gpu trust
state, packing candidate count, build variant count, total solution count,
retained trace count, coverage probability, raw setup metrics, raw metrics
export, score basis, score accuracy level, unsupported reason, renderer
capability, skin manifest validity, and atlas provenance validity. Unsupported
features must carry a reason in diagnostic/render/backend panels; disabling a
button is not enough.

GUI label and JSON key separation is mandatory. Contract keys such as
`gpu_trust_state`, `score_accuracy_level`, `backend_fallback_reason`, and
`renderer_capability` stay stable and unlocalized. User-facing labels use
`LocalizedLabelSchema` translation keys like `ui.gui.v2.field.*`.

X8 marker summary: backend auto/cpu/gpu/hybrid.
X8 marker summary: fallback reason.
X8 marker summary: gpu trust state.
X8 marker summary: raw setup metrics.
X8 marker summary: score accuracy level.
X8 marker summary: renderer capability.
X8 marker summary: skin manifest validity.
X8 marker summary: unsupported reason.
X8 marker summary: GUI label and JSON key separation.

## X9 GPU Packing Strengthening

X9 strengthens GPU packing as a performance backend while preserving the same
correctness contract used by CPU packing and hybrid scheduling. It introduces a
larger batch planner, dominance prefilter, GPU candidate hash, readback
compression, CPU exact confirm optimization, coverage bitset OR helper,
backend autotune, and memory pressure handling as explicit backend
components.

The larger batch planner may increase GPU batch size only when readback, CPU
confirm backlog, coverage buffer pressure, and memory pressure are below the
configured thresholds. Backend autotune and memory pressure handling may
throttle or shrink the batch, but they must never silently drop coverage rows
or present a partial result as complete.

The dominance prefilter may deduplicate exact optional candidate duplicates,
but it must preserve every required candidate. GPU candidate hash is an
acceleration index only. CPU exact confirm remains mandatory: a candidate hash
match must be followed by exact identity comparison of shape, tiling, operation
set, final board, and coverage bits before a GPU result can become
CPU-confirmed.

Readback compression is allowed only when decompression preserves the candidate
set exactly. The coverage bitset OR helper consumes only CPU-confirmed GPU
candidate evidence or deterministic reference evidence. Unconfirmed GPU
coverage never sources probability, exact score, exact spin, or BuildVariant
acceptance.

Fallback reason visible is required whenever GPU strengthening cannot run or
the backend falls back. The report exposes `gpu_result_deterministic`,
`gpu_result_cpu_confirmed`, `cpu_reference_and_gpu_result_match`,
`readback_compression_preserves_candidates`,
`dominance_prefilter_does_not_drop_required_candidate`, and
`fallback_reason` so product output can distinguish performance acceleration
from trusted exactness.

X9 marker summary: larger batch planner.
X9 marker summary: dominance prefilter.
X9 marker summary: GPU candidate hash.
X9 marker summary: readback compression.
X9 marker summary: CPU exact confirm optimization.
X9 marker summary: coverage bitset OR helper.
X9 marker summary: backend autotune.
X9 marker summary: memory pressure handling.
X9 marker summary: fallback reason visible.
X9 marker summary: CPU exact confirm remains mandatory.
X9 marker summary: unconfirmed GPU coverage never sources probability.

## X10 MVP2 Acceptance Gate

X10 defines the MVP2 acceptance gate that keeps MVP2 expansion from breaking
MVP1 product health. The gate runs MVP1 ProductE2E first, then the MVP2
Rule/Kick tests, MVP2 Scoring tests, SpinTarget coverage tests, Setup raw
metrics tests, Render/Fumen transform tests, GPU portable/reference tests, GUI
schema tests, and Architecture validation.

The gate is intentionally ordered: `mvp2_acceptance_runs_mvp1_product_e2e_first`
means MVP1 pc/path/percent product health is checked before optional MVP2
surfaces. MVP2 feature failure must not break MVP1 pc/path/percent, and MVP2
checks must not relabel partial, preview, skeleton, or basic-approximation
features as exact.

Exact claims remain guarded by the existing capability registry and each
feature contract. `mvp2_exact_claims_guarded` means an exact label requires the
capability to be Exact and backed by fixtures. `mvp2_scoring_basic_approximation_disclosed`
keeps built-in scoring profiles visibly approximate until profile-specific
exactness is proven. `mvp2_renderer_exact_only_when_supported` now resolves to
the connected exact PNG/GIF runtime report and its pixel/provenance fixtures.
`mvp2_gpu_fallback_reason_visible` requires GPU fallback and unavailable
states to remain visible in backend reports and diagnostics.

The ManagedLocal gate provides a process-free static product contract, while
Trusted owns executed evidence. The product acceptance order remains
the same: ProductE2E first, MVP2 feature gates second, architecture validation
last.

X10 marker summary: mvp2_acceptance_runs_mvp1_product_e2e_first.
X10 marker summary: mvp2_exact_claims_guarded.
X10 marker summary: mvp2_scoring_basic_approximation_disclosed.
X10 marker summary: mvp2_renderer_exact_only_when_supported.
X10 marker summary: mvp2_gpu_fallback_reason_visible.
X10 marker summary: MVP2 feature failure must not break MVP1 pc/path/percent.

## T6 MVP2 Acceptance Tests

T6 pins the exactness and approximation checks that the MVP2 acceptance gate
must keep visible. These tests are not broad feature-completeness claims; they
prove that MVP2 surfaces stay honest about approximate scoring, missing spin
evidence, connected exact renderer capability, and score-aware objective
probability behavior.

The scoring surface must keep `score_profile_reports_accuracy_level` and
`tetrio_not_profile_specific_exact_until_exact_supported`. A built-in
profile such as TETR.IO may report `basic-approximation`, but it cannot
claim profile-specific exactness until the exact evaluator and fixtures are
connected.

The spin target surface must keep `spin_target_requires_classifier` and
`missing_kick_evidence_is_incomplete_not_exact`. A spin target request without
a classifier is invalid, and traces missing kick evidence remain incomplete
instead of becoming exact spin evidence.

The objective, setup, and render surfaces must keep
`max_score_cover_does_not_double_count_probability`,
`setup_raw_metrics_no_condition_summary`, and
`renderer_connected_exact`. Score-aware cover must not double count
probability, setup output must expose raw metrics without condition summary
interpretation, and renderer capability must match the connected exact runtime
report through `renderer_capability_matches_runtime_report` and
`render_status_ui_uses_product_capability`.

T6 marker summary: score_profile_reports_accuracy_level.
T6 marker summary: tetrio_not_profile_specific_exact_until_exact_supported.
T6 marker summary: spin_target_requires_classifier.
T6 marker summary: missing_kick_evidence_is_incomplete_not_exact.
T6 marker summary: max_score_cover_does_not_double_count_probability.
T6 marker summary: setup_raw_metrics_no_condition_summary.
T6 marker summary: renderer_connected_exact.
T6 marker summary: renderer_capability_matches_runtime_report.
T6 marker summary: render_status_ui_uses_product_capability.

## G0 MVP3 Scope Gate

G0 keeps MVP3 generalization behind a separate capability registry so custom
piece, mixed piece set, custom bag, custom board, Board128/Wide, generic
operation table, generic exact-cover, DLX, area multiset feasibility, custom
rule editor, generic GPU descriptor, GPU BuildUp expansion, and custom skin
editor work cannot contaminate MVP1/MVP2 standard tetromino paths.

MVP3 capability identity is separate from MVP2 capability identity, but both
use the same three stable states. Schema without a connected runtime is
`Unsupported`; connected non-exact algorithms are `ConnectedApproximate`;
exact runtime support is `ConnectedExact`. Unsupported entries require a
disabled reason.

The standard fast path must remain unchanged: `standard_fast_path_unchanged`
must hold for every MVP3 capability. Generic cache identity must include custom
piece, board, bag, rule, operation-table, area, and generic GPU descriptor
identity; MVP3 cache key가 standard enum만 사용 is forbidden. Generic schema
must never be displayed as runtime-connected support, and custom feature를
standard fast path로 조용히 fallback is forbidden.

G0 marker summary: mvp3_capability_report_lists_all_generalization_features.
G0 marker summary: schema_only_features_do_not_execute_runtime.
G0 marker summary: unsupported_features_emit_disabled_reason.
G0 marker summary: standard_fast_path_unchanged.
G0 marker summary: custom feature를 standard fast path로 조용히 fallback.
G0 marker summary: generic schema 추가 후 runtime이 연결된 것처럼 표시.
G0 marker summary: MVP3 cache key가 standard enum만 사용.

## G1 Custom Piece Domain Model

G1 introduces the custom/mixed piece domain model without connecting custom
pieces to runtime search. Standard tetromino pieces remain a separate
`StandardTetrominoPiece` type over the standard `PieceKind` enum. Custom
pieces are represented by `CustomPieceDefinition` and stable
`PieceDefinitionId`; custom pieces must not be inserted into `PieceKind` as an
empty, unknown, or fallback enum variant.

The custom `PieceDefinition` schema carries `piece_definition_id`,
`display_name`, `area`, `rotation_states`, `cells_by_rotation`,
`bounds_by_rotation`, `spawn_offsets`, `color_hint`, `symmetry_class`, and
`source_provenance` source/provenance metadata. Area is per piece definition
and may be non-4. Generic feasibility uses the area multiset and keeps
`missing_cells_mod_4_not_used_for_generic_feasibility`.

`PieceSetDefinition` owns `piece_set_id`, `pieces`,
`standard_fast_path_compatible`, and `mixed_area_multiset`. Custom/mixed piece
sets are schema-valid but runtime-guarded with
`custom_piece_runtime_not_connected` and `mixed_piece_runtime_not_connected`
until generic placement/search runtime exists.

Cache identity must include `piece_definition_id_fingerprint`,
`piece_area_multiset_fingerprint`, and `piece_set_profile_id`. These values are
lowered directly into the connected packing/cache descriptor only after
validation; no schema-only piece identity ABI is exported.

G1 marker summary: standard_tetromino_fast_path_unchanged.
G1 marker summary: custom_piece_schema_validates.
G1 marker summary: custom_piece_runtime_not_connected_until_runtime_exists.
G1 marker summary: missing_cells_mod_4_not_used_for_generic_feasibility.
G1 marker summary: piece_definition_id_included_in_cache_keys.

## G2 Mixed Piece Set / Custom Bag / Supply Generalization

G2 keeps implemented supply kinds separate from extensions. Stable kinds are
`Standard7Bag`, `FixedSequence`, `ObservedWindow`, and
`MaterializedPatternUniverse`. A schema whose runtime is unavailable is
represented as `UnsupportedExtension(ExtensionId)` and is rejected before C
execution; it does not receive a dedicated stable enum value.

Supply provenance is explicit. Every generalized supply profile carries
`supply_provenance_id`, `bag_profile_id`, `piece_set_id`, optional
`observed_window_id`, `bag_boundary_evidence`, `duplicate_witness`, and
`ambiguity_report`. C cache identity still includes supply provenance through
`supply_provenance`, and the C/FFI surface exposes
`clr_supply_identity_descriptor` / `CSupplyIdentityDescriptor` for the same
cache-key material.

Observed windows are not fixed into exact queues. `observed_window_ambiguity_reported`
must remain true when multiple bag-boundary candidates exist, and duplicate
witnesses must stay visible. Fixed sequence, observed standard 7-bag, and
bag-aligned pattern inputs remain distinct provenance routes.

Mixed/custom bag schema can validate through `mixed_bag_schema_validates` and
`custom_bag_schema_valid`, but custom bag runtime starts guarded:
`custom_bag_runtime_not_connected_until_runtime_exists` and
`custom_bag_runtime_not_connected` are required until piece-definition-id
supply and generalized placement runtime are connected. Custom bags must never
silently fall back to standard 7-bag behavior.

G2 marker summary: standard_7_bag_path_unchanged.
G2 marker summary: mixed_bag_schema_validates.
G2 marker summary: custom_bag_schema_valid.
G2 marker summary: custom_bag_runtime_not_connected_until_runtime_exists.
G2 marker summary: supply_provenance_in_cache_key.
G2 marker summary: observed_window_ambiguity_reported.

## M27 Scoring Post-Processing

M27 keeps scoring out of the core search hot path. Core search hot path stays
scoring-free: the C side exposes event basis data and Rust evaluates score
profiles from replay traces after BuildUp, coverage, and objective reduction.

The C scoring event basis lives under `core-c/src/scoring_events` and provides:

- placement event available
- clear event available
- drop event basis available
- spin event basis available

The Rust replay layer converts BuildUp representatives into replay events, and
`clearra-scoring` evaluates those replay events as post-processing. The score
profile evaluates replay via `ScoreModelEvaluator::evaluate_replay_trace`, not
by changing Packing, BuildUp, coverage rows, or PatternBitSet union. Score does
not change probability union: output must expose probability before/after
scoring and `score_does_not_change_probability_union=true` when they match.

Score output states accuracy level. MVP2 built-ins report
`accuracy_level=basic-approximation`; only a connected profile-specific exact
evaluator may report exact scoring. Output must also expose the score evaluation
basis and whether the evaluated traces cover the counted solution set.

M27 contract markers: M27 Scoring Post-Processing; Core search hot path stays
scoring-free; score profile evaluates replay; placement event available; clear
event available; drop event basis available; spin event basis available; score
does not change probability union; score output states accuracy level;
core-c/src/scoring_events.

M27 exact markers: Core search hot path stays scoring-free; score does not change probability union.

## X4 MVP2 Scoring Summary

X4 keeps score logic in post-processing. Score profiles are evaluated from
replay events that come from C BuildUp evidence plus `clearra-replay`; output must disclose
`score_event_basis=c-replay`, `score_accuracy_level`,
`score_profile_accuracy_mode`, `score_evaluation_basis`,
`score_evaluation_scope`, and `score_evaluation_complete`.

Core search exports every buildable `(candidate_id, pattern_id, trace_identity)`
replay seed without assigning a score. `clearra-postprocess` evaluates all legal
executions, keeps the highest legal score for each concrete supply pattern, and
applies the materialized pattern weights. Patterns without a PC execution
contribute zero. The result reports the full-universe field average and a
covered-pattern conditional average. It does not expose per-solution score rows
or score-aware minimum sets. The ordinary exact minimum-cover objective remains
independent of scoring.

When the execution batch, trace basis, or weights cannot materialize a complete
matrix, score summary reports `objective_complete=false` and an explicit
incomplete reason. A partially materialized execution batch uses
`score_matrix_incomplete`. Zero-valued placeholder cells are forbidden; the
only defined zero is the contribution of a concrete pattern proven to have no
PC execution.

X4 contract markers: score profile exact/basic accuracy 표시; score event basis
from C/replay; failed PC patterns contribute zero; score does not modify
coverage probability; score evaluation basis visible; sample vs full
evaluation distinguished.

## Spin And Score Coverage Contract

Spin and score objectives are first-class typed coverage contracts.
`SpinTargetRequest` is the canonical query/goal contract owned by
`clearra-problem`; `clearra-scoring` owns `SpinClassifier` and
`SpinTargetPredicate` evaluation. Spin-target queries compile into ordinary
`SearchProblem` goals, but the satisfaction check is applied only after BuildUp
has produced a `BuildVariant` and replay has produced spin evidence. The
reducer flow is:

`BuildVariant -> ReplayTrace -> SpinClassifier -> SpinTargetPredicate -> CoverageRowKind::SpinTarget -> PatternBitSet OR`.

Kick-sensitive scoring depends on explicit `KickEvidence`; hidden C
reachability state is not enough for exact special-spin claims. Special cases
such as Fin, ISO, and NEO live in `SpecialSpinCaseRegistry`, and exact variants
require `VerifiedSpecialSpinProfile` evidence.

Score aggregation stays separate from coverage probability. `CandidateScoreStats`
and `PatternScoreContribution` may rank or summarize candidates, but they do not
change the pattern-union probability. `ScoreProfileObjectValidator` guards score
model ids, spin classifier ids, all-spin policy, drop-score trace completeness,
and exact/estimated capability before product execution.

Coverage-producing BuildUp must use `BuildUpExecutionMode::EnumerateVariants`.
`VerifyFirst` may provide a visual witness, but it is not a coverage source.

Static architecture authority is defined in `docs/architecture-validation.md`.
Implementation marker presence is advisory only and cannot establish runtime
correctness. Release correctness comes from the executed
`AdversarialCorrectness` cases; static release blockers are limited to
dependency boundaries, forbidden APIs, public ABI fields, unsafe isolation,
and unsupported capability disclosure.

## X5 MVP3 Custom Piece Foundation

X5 establishes custom/mixed piece schema without connecting custom pieces to the
runtime search hot path. `PieceDefinitionId` is the stable identity, custom
piece_area and rotation states live on `CustomPieceDefinition`, and
`CustomOperationTableSchema` lowers interpreted definitions into schema-versioned
operation records. `PieceRegistryBridge` preserves the standard fast path for
standard-only piece sets and exposes `custom_piece_runtime_not_connected` for
mixed/custom registries.

Generic feasibility uses area multiset feasibility instead of tetromino-only
arithmetic. `AreaMultisetFeasibility` reads `MixedPieceSet` or
`MixedBagProfile` areas with multiplicity and must not use `missing_cells % 4`
to reject custom/mixed PC scenarios. C hot-path cache identity includes both
piece definition id fingerprint and piece area multiset fingerprint so registry
order or same-name profile drift cannot reuse stale cache entries.

X5 contract markers: PieceDefinitionId; piece_area; rotation states; custom
operation table schema; piece registry bridge; area multiset feasibility;
validation guard; standard fast path unaffected; custom piece unsupported
reason visible; missing_cells % 4 not used for generic feasibility; piece
definition id included in cache keys.

## M28 GUI Schema

The GUI schema represents the backend/result contract without
owning solver truth. The UI schema crate remains presentation schema only:
backend ids come from canonical execution policy values, problem preset ids
come from `clearra-problem::SearchProblemPreset`, rule/profile ids come from
their canonical registries, and result columns mirror executor/output contract
fields.

The M28 GUI surface includes:

- `language_selector_schema`
- `localized_label_schema`
- `backend_options`
- `problem_preset_options`
- `scenario_editor_schema`
- `setup_explorer_schema`
- `build_editor_schema`
- `rule_editor_schema`
- `score_editor_schema`

The schema must express backend auto/cpu/gpu/hybrid, fallback reason, packing
candidate count, BuildVariant count, total_solution_count, retained_trace_count,
coverage_probability, raw metrics export, score basis, and unsupported reason.
GUI labels use `clearra-i18n` translation keys plus English fallback labels;
the schema does not make translated strings the canonical contract. The language
selector exposes English and Korean options, resolves explicit user preference
before detected OS locale, maps `ko`/`ko-KR` style locale values to Korean, and
defaults to English. JSON/output contract keys such as `total_solution_count`
must never be translated.

The one desktop product lives under `apps/clearra-desktop`. Its SvelteKit UI
calls the Tauri command surface, Tauri delegates only to `clearra-gui-host`, and
the host builds a typed request and calls `clearra-app`:

`SvelteKit -> Tauri -> clearra-gui-host -> clearra-app -> validation -> clearra-problem -> exact WASM CPU / WebGPU backend`.

There is no CMake GUI product, shell-preview executable, CLI subprocess bridge,
or fixture final response. The desktop host preserves the language preference
storage contract, diagnostic localization keys such as
`ui.diagnostic.backend_fallback_used`, and backend option schema binding from
`clearra-ui-schema/setup_explorer/BackendOptionsSchema`.

Desktop work is asynchronous end to end. The UI calls `start_job`, polls the
batched `get_job_events` command, exposes progress/backend/memory/resource
status, and sends a real cancellation request. A terminal `Completed`, `Failed`,
or `Cancelled` event joins the worker, releases the active queue slot, and allows
the next job to start. The desktop gate compiles the Svelte/TypeScript sources
in memory on every host. WASM CPU GUI-host lifecycle tests and Tauri compilation
run only when the host permits generated executable evidence; an unavailable
host capability remains a release blocker rather than a static-pass substitute.

M28 contract markers: M28 GUI Schema; language selector; localized label schema; backend auto/cpu/gpu/hybrid;
fallback reason; packing candidate count; BuildVariant count;
total_solution_count; retained_trace_count; coverage_probability; raw metrics
export; score basis; unsupported reason; Tauri desktop host boundary; I18N
resource ownership; language preference storage; diagnostic localization;
backend option schema binding; GUI direct C core calls are forbidden.

## M29 Diagnostics and Security Gate

M29 connects core-c, GPU/backend selection, FFI memory scope, file/input bridge
failures, and executor fallback evidence to the Rust diagnostic system. C core
status is never treated as a plain string or ignored log line: C status maps to
Rust diagnostic, ABI mismatch maps to `E_CORE_ABI_VERSION_MISMATCH`, packing
descriptor failures map to `E_CORE_PACKING_FAILED`, and BuildUp/result-buffer
failures map to `E_CORE_BUILDUP_FAILED`.

The security gate also covers runtime safety evidence. GPU unavailable maps to
diagnostic with `E_BACKEND_GPU_UNAVAILABLE`, explicit backend fallback maps to
`W_BACKEND_FALLBACK_USED`, and GPU packing results that still need CPU exact
confirmation map to `W_GPU_RESULT_CPU_CONFIRM_REQUIRED`. Memory scope failures
map to `E_CORE_MEMORY_SCOPE_INVALID`, double-release maps to
`E_CORE_MEMORY_CONTEXT_DOUBLE_RELEASE`, and memory leak report maps to diagnostic
with `E_CORE_MEMORY_LEAK_DETECTED`.

Invalid C result buffers are rejected before coverage/objective reduction.
Coverage row views must check word counts, input length, tail bits outside the
pattern universe, and candidate id range. PackingCandidate as solution attempt
rejected is a hard diagnostic: packing candidates are only candidate evidence
until BuildUp verification promotes them to build variants/solutions.

S6 Security Diagnostic Gate keeps the security/correctness failures visible to
users. Unbounded native pointer/count views map to `E_CORE_FFI_BUFFER_BOUNDS` or
`E_CORE_INVALID_NATIVE_VIEW`, missing GPU worker memory tickets map to
`E_GPU_WORKER_MISSING_MEMORY_TICKET`, missing fence epochs map to
`E_GPU_FENCE_EPOCH_MISSING`, unconfirmed GPU probability use maps to
`E_GPU_UNCONFIRMED_PROBABILITY_SOURCE`, runtime raw SVG rendering maps to
`E_RENDER_RUNTIME_SVG_FORBIDDEN`, missing render provenance maps to
`E_RENDER_ASSET_PROVENANCE_MISSING`, GUI subprocess execution maps to
`E_GUI_SUBPROCESS_FORBIDDEN`, and frontend/AppRequest bypass maps to
`E_FRONTEND_TYPED_REQUEST_REQUIRED`.

S6 output rule: JSON diagnostics include diagnostic evidence and
`suggested_next_step` under `contract.diagnostics.items`, while text diagnostics
show a concise message plus optional `location`, `evidence`, and `next` lines.
Security errors are not downgraded to warnings, and fallback is never silent.

M29 contract markers: M29 Diagnostics and Security Gate; C status maps to Rust
diagnostic; GPU unavailable maps to diagnostic; memory leak report maps to
diagnostic; invalid C result buffer rejected; PackingCandidate as solution
attempt rejected.

M29 exact markers: C status maps to Rust diagnostic; GPU unavailable maps to diagnostic; memory leak report maps to diagnostic; invalid C result buffer rejected; PackingCandidate as solution attempt rejected.

S6 contract markers: S6 Security Diagnostic Gate; JSON diagnostics include diagnostic evidence; text diagnostics include suggested_next_step; C status is not collapsed to unknown error; backend fallback diagnostic is visible; security error is not downgraded to warning.

## M Proof-carrying Pruning

Pruning is clear-state aware, proof-carrying, and budget-aware. Local target-frame
domain facts are not global candidate removal facts. Candidate removal is owned
by connected native engine producers. The connected Packing producers cover
collision, target-mask overflow, complete multiset-family piece-count overflow,
and exhaustive line-clear-order impossibility. Rust exports no
`AuthorizedPrune`, proof seal, or global proof constructor. Ledger fields are
reporting metadata and cannot themselves grant removal authority.

Reachability, all-state domain, and independent language proof engines are not
connected. Their observations remain conditional evidence and cannot create a
global candidate drop. Witnessed pattern coverage cannot create language proof
authority.

`LocalOnly` and `ClearStateConditional` evidence keeps the candidate and falls
back to BuildUp or a less aggressive path. A fact observed under one clear-state
can become global only after all reachable clear-states have been proven. A
globally forced family is still only a constraint until a candidate-specific
violation proof is constructed. Candidate drop without ledger is forbidden,
budget overflow reports `ResourceBudgetExceeded`, and resource cap results are
incomplete rather than silent drops.
Architecture validation marker: candidate drop without ledger is forbidden.

CPU pruning context is derived from the concrete `clr_packing_problem` cache
identity; GPU pruning context is derived from the concrete batch id and its
operation-table/rule/kick identity. GPU operation-table filtering and frontier
collision filtering use the same ledger-aware static pruner as CPU packing.
Missing context or ledger fails before removal. Ledger retention capacity is
diagnostic-only and cannot turn a GlobalSafe prune into search truncation.
`PruningEvidencePolicy::BestEffort` permits summarized evidence truncation.
`PruningEvidencePolicy::CompleteRequired` never authorizes a drop without a
retained entry: capacity pressure keeps the candidate and routes it to BuildUp
or fails the verification audit explicitly.
The C public ABI does not accept a raw caller-filled GlobalSafe entry as a drop
authorization. Only connected producer paths may remove a candidate: static
collision and target-mask overflow, complete multiset-family prefix rejection,
and complete operation-subset line-clear-order rejection. Incomplete supply,
cancelled search, resource pressure, or strict evidence-capacity pressure keeps
the candidate.
At the Rust FFI boundary, `CNativePruningProofLedger` is validated and copied
into an owned `NativePruningLedger`; no borrowed C ledger entry escapes the
native call.

Forbidden prune names remain invalid: `LooksBad`, `RareShape`,
`ProbablyImpossible`, `MctsLowScore`, `NoImmediatePlacement`,
`ThisCellLooksLikeLOnly`, `FloatingInTargetFrame`, `ScoreTooLow`, and
`SpinUnknown`.

## L BuildOrders / HoldReachableOrders / Language Intersection Coverage

The invariant is: `Packing P covers Pattern Q` only when `BuildOrders(P) intersects HoldReachableOrders(Q)`. A single representative order is never enough to prove that invariant.

The current product bridge does not claim an independently generated symbolic language proof. It runs pattern-specific BuildUp for each concrete pattern Q. An accepted `PatternVerifiedBuildVariant` is recorded by `WitnessedPatternCoverageAccumulator`, which inserts Q directly into the candidate `PatternBitSet`. Duplicate accepted variants for Q still set one bit through OR semantics.

The explicit-order `BuildOrderLanguage`, `HoldReachableLanguage`, and
`LanguageIntersection` implementation is a test helper, not product coverage or
pruning authority. Product code must not derive the same synthetic token from
an already accepted variant, insert it into both languages, and call the
resulting tautology an intersection proof. Independent symbolic language
execution is unsupported in the product. Product coverage uses pattern-specific
BuildUp generated from the operation dependency state, PieceSource, and
HoldAutomaton.

`verify_first` witnesses and raw BuildVariant counts do not source coverage
probability. Pattern-specific BuildUp enumeration is the product coverage
authority.

## Part S Security Surface Inventory

`docs/security-fix-map.md` is the release-blocking security inventory for C
memory, FFI pointer/count views, GPU fallback/trust, coverage truncation,
render asset import, GUI execution, and Web/WASM boundaries. The
inventory starts with `SEC-C-MEM-001`, `SEC-FFI-001`, `SEC-FFI-002`,
`SEC-GPU-001`, `SEC-GPU-002`, `SEC-COV-001`, `SEC-REN-001`, `SEC-SVG-001`,
`SEC-GUI-001`, and `SEC-WASM-001`.

Architecture validation must keep these guards active:
`architecture_validation_rejects_silent_gpu_fallback`,
`architecture_validation_rejects_runtime_raw_svg`,
`architecture_validation_rejects_gui_subprocess`, and
`architecture_validation_rejects_unbounded_ffi_pointer_count`. MVP-outside
features must expose disabled/capability diagnostics instead of appearing to
work, and capacity exceeded must not truncate without diagnostic evidence.

## U Test / Acceptance / Release Gate

Release gates must block solution loss, false probability, silent fallback,
unsafe FFI, and renderer/GUI boundary regressions. The U gate is a named
contract layer over existing validation and tests: architecture tests pin
dependency and forbidden algorithm boundaries; data-structure tests pin
`PieceSource`, `PackingBfsState`, `BuildUpMemoKey`, and
ShapeFamily/TilingVariant/BuildVariant separation; algorithm tests pin layered
BFS, GPU reference parity, BuildUp, and language intersection coverage;
pruning tests pin proof-carrying clear-state logic; probability tests pin
PatternBitSet union and incomplete resource-cap output; spin/score tests pin
special spin cases and score/attack separation; PostProcess GPU tests pin trust
state and search/post backend separation; field/Fumen/replay tests pin adapter
boundaries and lock-frame replay; security tests pin C memory, FFI pointer/count,
GPU ticket/fence, raw SVG, GUI subprocess, and WebGPU shader guards.

U release blockers include `MITM PC backend marker found`,
`PackingBfsState owns queue/hold/full trace`,
`BuildUp memo key missing deleted_line_state`,
`BuildUp memo key missing hold_automaton_state`,
`representative order used as coverage proof`,
`verify_first used as coverage proof`, `Unknown spin treated as False`,
`Fin modeled as kick table`, `postprocess changes PC coverage probability`,
`resource cap output marked complete`, `GUI subprocess shortcut`,
`WASM subprocess/process shortcut`, and
`unconfirmed GPU result sources exact probability`.

