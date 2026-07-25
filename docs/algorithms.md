# Algorithms

This document tracks checkpoint label planning, C Packing + BuildUp execution, exact-cover fallback, coverage union probability, setup grouping, and build coverage CSP design. Algorithm notes should name the owning crate instead of duplicating implementation details across crates.

## PC Search State

The product PC route compiles both opening and scenario requests through `clearra-problem` before execution. Opening PC is a preset over the same scenario-shaped `SearchProblem` used by `pc-scenario`: 2L/4L/6L openings compile to empty standard boards, exact piece windows of 5/10/15, a `ClearToEmpty` goal, and labels `2L`/`4L`/`6L`. `PcTarget` remains label and analysis metadata; it is not the core success condition.

Product execution goes through `ProblemCompiler`, `SearchProblem`,
`CoreExecutor`, `PackingProblem`, and `BuildUpProblem`. Rust owns typed product
contracts; C owns packing and BuildUp search.

The product contracts below are the active algorithm surface:

- C packing owns occupancy, piece-multiset, operation-set, shape, and tiling
  state. C BuildUp separately owns PieceSource cursor, HoldAutomaton, board,
  deleted-line, reachability, and cache hot-path identity.
- Replay trace contracts live in `clearra-replay`, including `TraceCanonicalKey`.
- Solution counting and trace retention are separate. `total_solution_count` is the number of solutions counted, `unique_solution_count` is the canonical trace-key count, and `retained_trace_count` is only the number of traces kept in memory for output. `count_complete` and `count_truncated_reason` disclose whether performance tests can trust the count. `trace_retention_truncated` and `trace_retention_reason` disclose whether only a representative trace sample was retained. A retained trace limit must never become a count truncation reason.
- Product commands route through assembler, `clearra-app`, validation, `ProblemCompiler`, `SearchProblem`, and `CoreExecutor`.
- `OpeningPreset` owns the empty-field opening lowering. It converts opening metadata into a scenario-shaped core query and preserves line labels for output, continuation, and analysis.
- `ScenarioPreset` owns direct setup completion lowering. It wraps the interpreted scenario input without creating a second solver contract.
- `clearra-pc-graph` owns checkpoint metadata only: `CheckpointSchedule`, `LinePartition` labels, `ContinuationToken`, `ContinuationHint`, `ChainClassifier`, and `BagPhaseClassifier`.
- Product output may expose schedule metadata such as `checkpoint_schedule_source=clearra-pc-graph-labels`, `checkpoint_schedule_partitions`, and `checkpoint_schedule_checkpoint_count`; it must not expose checkpoint cache counters or frontier execution fields as the default core result.
- The product result contract is `SearchExecutionReport` with `PackingResult`, `PackingCandidateView`, `BuildUpResult`, `CoverageRowView`, `CoverageResult`, `ObjectiveResult`, `ReplayTrace`, and `BackendReport`. `PackingCandidate` counts are not solution counts, retained trace counts are not total solution counts, and checkpoint trace fragments are not full solution traces.
- C Board64 operations own the search hot path: `clearra_board64_empty`, `clearra_board64_occupied_mask`, `clearra_board64_cell_index`, `clearra_board64_cell_mask`, `clearra_board64_row_mask`, `clearra_board64_row_is_full`, `clearra_board64_collision`, `clearra_board64_place`, `clearra_board64_clear_lines`, `clearra_board64_hash`, and `clearra_board64_equal`. Rust `Board64State` remains available for replay/render contracts only.
- Board dispatch keeps the Board64 fast path unchanged and selects `Board64` for up to 64 cells, `Board128` for 65..128, `Board256` for 129..256, and `Wide` above 256. Board128 and Board256 expose fixed-word row/collision/place and operation-mask contracts. Wide descriptors expose generic row metadata but return `CLR_BOARD_UNSUPPORTED_BACKEND` for operation masks until the dynamic-word runtime is connected. Capability reasons remain explicit: `board_width_out_of_scope`, `board_backend_not_connected`, and `wide_board_runtime_not_connected`, so unsupported board width silent fallback forbidden is enforced. Standard 10-wide PC requests use the legacy Board64 contract through 6L and `clr_standard_pc_extended_board_descriptor` through 24L. Its four canonical initial-board words select Board128 for 7..12L and Board256 for 13..24L; no high bits are truncated into the compact path.
- `clr_board_backend_kind_for_cell_count` selects storage, `clr_board_backend_capability_for_cell_count` reports whether its full runtime is connected, and `clr_board_dispatch_row_mask` plus `clr_board_operation_mask_from_cells` construct masks without changing backend identity.
- C piece operations own the standard tetromino geometry table: `clearra_piece_is_standard_tetromino`, `clearra_piece_area`, `clearra_tetromino_shape`, `clearra_rotation_count_for_piece`, `clearra_operation_id`, `clearra_operation_from_shape`, `clearra_operation_mask`, `clearra_operation_table_generate`, and `clearra_operation_set_count_piece`. The table covers I/O/T/S/Z/J/L, four rotation states per piece, cell offsets, bounds, stable operation masks, area 4, and deterministic operation ids. Custom operation schemas stay in the Rust extension/validation layer and are rejected before C candidate or reachability execution; no schema-only generic operation C ABI is exported.
- C rule/kick compact profiles own the runtime view of built-in rules: `clearra_rule_profile_from_descriptor`, `clearra_srs_kick_table`, `clearra_srs_plus_kick_table`, `clearra_no_kick_table`, `clearra_kick_table_sequence_for`, `clearra_kick_table_supports_180`, `clearra_kick_table_zero_offsets_only`, and `clearra_spawn_profile_from_id`. Rust `clearra-rules` remains the source/verify/import/export owner. C supports SRS, SRS+, and NoKick descriptors, reports SRS+ 180 capability, keeps NoKick zero-offset-only, and returns unsupported status for rules outside the compact runtime surface.
- Rust clearra-rules remains the source/verify/import/export owner; C rule/kick code is a compact runtime view only.
- C candidate generation owns the placement candidate hot path: `clearra_candidate_search`, `clearra_harddrop_candidates_generate`, `clearra_locked_candidates_generate`, `clearra_locked180_candidates_generate`, `clearra_candidate_first_success_kick`, and `clearra_candidate_transition_kind`. Candidate generation consumes the C operation table and compact rule profile, then returns a deterministic possible Action/Operation list. C candidate fixtures verify harddrop candidate matches fixture, locked candidate matches fixture, locked180 candidate matches fixture, harddrop impossible but locked reachable, collision-free but unreachable placement rejection, locked180-only placement, kick first-success earliest valid offset ordering, candidate cache key includes board/rule/piece, duplicate candidate removed, unreachable placement reject, kick first-success ordering, and rotation transition correctness.
- C reachability owns runtime placement possibility checks: `clearra_reachability_check`, `clearra_reachability_policy_for_mode`, `clearra_harddrop_reachability_is_reachable`, `clearra_locked_reachability_is_reachable`, `clearra_reachability_kick_table_from_rule`, `clearra_kick_first_success`, and `clearra_reachability_cache_key`. Collision-free placement is not enough to be reachable. The stable policy enum contains only HarddropOnly, LockedReverseGraph, and Locked180ReverseGraph. Kick order uses first-success, locked reachability uses a reverse movement graph over Board64 candidate states, and M9 fixtures cover collision-free but unreachable, harddrop reachable, locked reachable via multiple movements, kick reachable only with first-success offset, first-success earliest valid offset, 180 reachable where locked90 rejects the same target, and kick order mismatch rejected.
- C geometry packing compiles an immutable catalog with
  `clearra_geometry_catalog_compile`, searches exact row families with
  `clearra_geometry_exact_cover_search_graph`, and streams borrowed catalog row
  IDs through `clearra_geometry_solution_graph_stream_buildable_task` or
  `clearra_geometry_catalog_rows_buildable_to_sink`. Product execution uses the
  shared `clr_packing_problem`, so opening and scenario presets share the same
  SearchProblem -> PackingProblem path. Geometry paths are not solutions:
  pattern-specific BuildUp checks PieceSource order, hold, reachability,
  deleted-line transitions, and operation order before
  `clearra_packing_materialize_catalog_row_ids` may create a candidate.
- Packing problem masks use the search-height Board64 universe. `visible_height` is display metadata rather than a packing boundary. `search_height` drives `problem_layout`, and `initial_board`, `goal_region_mask`, `required_fill_mask`, and `forbidden_mask` are valid above the visible rows only when the cells remain inside `search_height`.
- The product reducer consumes only BuildUp-accepted catalog rows. It groups by
  hash bucket but confirms exact piece, rotation, coordinate, operation ID, mask,
  final board, and line-state identity before dedupe. The older raw CPU/GPU
  buffer host reducer remains a `BUILD_TESTING` checkpoint for backend
  equivalence; it is not an authority that can promote an unverified geometry
  path into the product BuildUp or coverage queues.
- `CBuildUpProblemTemplate` compiles the immutable SearchProblem descriptors
  once. Geometry existence workers attach a shared `NativeGeometryCatalog` and
  pass borrowed row IDs plus predecessor constraints to
  `clearra_buildup_exists_catalog_rows_with_constraints_and_workspace`.
  Candidate materialization follows this exact gate. The same template later
  configures an accepted candidate for complete BuildVariant enumeration;
  candidate conversion is therefore a downstream interpretation path, not a
  packing proof or coverage shortcut.
- BuildUp exposes only its connected Board64 runtime scope in stable ABI. `C_BUILDUP_MAX_OPERATIONS = 15`; `clearra_buildup_runtime_status_for_board` and `clearra_buildup_operation_set_runtime_status` return `CLR_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE` outside that scope. operation_count > 15 is guarded, not truncated, and Board128/Board256/Wide never fall back to Board64.
- C CPU BuildUp verification decides whether that problem is an actual build variant through `clr_buildup_worker_verify`, `clr_buildup_worker_verify_into_buffer`, `clr_buildup_verification`, and `clr_build_variant_buffer`. The verifier searches remaining operations with the representative order hint used only as priority, applies failed-state memoization over board/remaining/queue/hold/line-clear state, and checks operation order, line clear dependency, y adjustment, groundedness, reachability, queue order, hold decision, bag pattern, piece window, and goal satisfaction. Y adjustment uses `ClearraLineClearState.deleted_row_mask`: each operation cell moves down by the count of deleted original rows below that cell, so non-bottom clears and skim histories are not reduced to `cleared_lines * width`. BuildUp reachability compiles `problem->rule` into a `ClearraReachabilityKickTable` with `clearra_reachability_kick_table_from_rule` and passes that table to `clearra_reachability_check`; it must not pass a null kick table for kick-aware modes. BuildUp을 통과한 결과만 BuildVariant가 된다. M13 fixtures cover packing possible but queue order impossible, packing possible but hold disabled impossible, packing possible but line clear y adjustment impossible, packing possible but SRS reachability impossible, NoKick/SRS/SRS+180/imported verified BuildUp reachability bridge kick tables, representative order hint is priority not single path, and valid packing and valid BuildUp.
- BuildUp execution is mode-specific. `verify_first` is a witness path, `enumerate_variants` is the coverage-producing path, and `count_variants` is the trace-light count path. `verify_first` returns a single representative witness and must not source coverage or min-cover; `enumerate_variants` must continue after the first accepted BuildVariant and traverse all reachable operation-order and queue/hold branches until the variant budget is reached; `count_variants` reports total count without retaining traces. `verify_first` is the only first-success mode. `clr_buildup_enumeration_limits.max_variants` is derived from the compiled `SearchProblemBudget`; `CLR_BUILDUP_ENUMERATION_TRUNCATED` must become an incomplete-result diagnostic, not a silent success. M6 tests include `buildup_verify_first_returns_single_witness`, `buildup_enumerate_variants_preserves_hold_branches`, `buildup_count_variants_reports_total_count_without_retaining_traces`, `verify_first_result_not_used_for_min_cover`, `build_variant_exports_hold_branch_kind`, and `build_variant_exports_kick_evidence`.
- KickEvidence export is bounded by `CLR_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT`. `CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED` means exact kick-sensitive spin classification lacks complete evidence for that BuildVariant.
- C coverage row bridging turns accepted BuildVariant rows into
  `clr_coverage_row_view` records through verified identity APIs. The bridge
  checks PieceSource, PatternBitSet universe, weight model, and OR-union
  compatibility, but probability never moves into C. Product probability and
  objective paths use `TypedCoverageMatrix`; untyped raw-word helpers are
  confined to internal algorithm tests.
- L coverage keeps `BuildOrders(P) intersects HoldReachableOrders(Q)` as the
  invariant. The product bridge runs pattern-specific BuildUp for every concrete
  Q, wraps accepted evidence in `PatternVerifiedBuildVariant`, and lets
  `WitnessedPatternCoverageAccumulator` insert Q directly into the candidate
  PatternBitSet. Explicit-order language types are test helpers, not product
  coverage or pruning authority. Independent symbolic language execution is
  unsupported. The same pattern covered by multiple variants counts once by
  PatternBitSet OR union.
- C/Rust FFI pointer escape is forbidden. Scope-bound C views such as pattern-bitset words and BuildVariant kick-evidence pointers must be copied into owned Rust snapshots before the C buffer or memory scope can be released. Tests such as `ffi_pattern_bitset_pointer_escape_is_blocked_by_owned_snapshot` and `ffi_build_variant_view_copies_kick_evidence_to_block_pointer_escape` keep this lifetime boundary visible.
- Rust `PatternBitSet` dynamic word allocation is budgeted by the caller. `PatternBitSet::new_with_word_budget` reports `WordCapacityExceeded` when a matrix would exceed the active memory scope. Spin and score matrices layer row budgets on top of that word budget.
- Rust coverage/objective reduction owns the final interpretation of C row evidence. `CoverageProbabilityReducer::family_probability` reduces rows by PatternBitSet OR union before `union_probability`, so variant coverage is not summed and family probability uses OR union. Product objective reduction applies all collector, stable-canonical-key unique collector, and exact minimum cover while keeping retained trace count separated from total count. Field-average scoring runs only after accepted executions are materialized.
- Replay/output reduction turns BuildUp-verified representative order into `ReplayTrace` through `ReplayEngine::build_variant_to_trace` and `SolutionTraceBuilder`. The replay layer preserves colored cell ownership and line clear event payloads, marks representative/sample traces explicitly, and lets `clearra-output` dispatch the same `ReplayTrace` to text, typed JSON, fumen-like comment pages, or exact bitmap output. `clearra-render` lowers replay into owner-aware `RenderScene` frames and consumes a validated PNG atlas; pixel and encoded-byte goldens guard `render_exact=true`. Runtime SVG is never part of this path.
- Core executor orchestration is the M17 product execution spine. `CoreExecutor` routes compiled PC/build problems into `PcService`, `CoverService`, or `PercentService`; the fixed empty-4L setup query enters the separate `WasmSetupSearchBackend`. PC execution runs through `PackingRunner::run`, `BuildUpRunner::run`, accepted C `BuildVariant` views, C coverage row views, Rust `ObjectiveReducer::reduce`, and `CoreExecutionResult`. Setup execution uses inverse lock-clear geometry, family quotient partial BuildUp, and exact-state forward/backward pattern coverage. CLI remains outside backend internals and calls the application facade.
- Packing and BuildUp have no fixture-backed product fallback. `NativeCoreError::Unavailable` becomes `E_NATIVE_CORE_UNAVAILABLE`; no candidate, witness, resource-complete report, or product trace key is synthesized. Fixture witnesses may exist only under test support and cannot be reached from AppRequest.
- C cache identity owns hot-path cache boundaries through `ClearraCacheIdentity`. Packing, candidate, reachability, and BuildUp memo keys all include board, piece set profile, piece definition id fingerprint, piece area multiset fingerprint, rule/kick profile, backend mode, operation table version, supply provenance or queue pattern id, piece window, and goal id. Rust-side caching that remains product-owned is coverage/objective caching over `PatternBitSet`, `CoverageMatrix`, and min-cover results.
- `PcContinuationAdvisor` owns the queue-left-for-next-PC check so continuation guidance does not leak into CLI or objective selection. Search results expose `remaining_queue_len`, `remaining_queue_preview`, `remaining_hold`, `next_pc_candidate`, `continuation_token`, and `continue_hint`; core never prompts the user. Scenario search keeps input replay separate: `scenario_replay_token`/`replay_hint` reproduce the original scenario, while `continuation_token`/`continue_hint` are emitted only from the solver output board, output hold, output cursor, and remaining queue after a successful result. `next_pc_available` means there is enough remaining queue to attempt another PC; `continuation_token_available` means Clearra could encode a non-interactive CLI follow-up token. If the next PC opportunity exists but token encoding is not supported, `continuation_token_unavailable_reason` must explain why. Scenario-state `sc2` continuation intentionally resets `exact_pieces` to `none` (`continuation_exact_pieces_policy=unset-for-next-state`) because exact consumption belongs to the completed solve, not the next one. It recalculates `min_remaining_queue` from the next remaining queue by preserving the previous minimum only up to the remaining queue length (`continuation_min_remaining_queue_policy=recalculated-from-next-state`).
- Scenario and post-PC results must report actual consumption, not the query window as a proxy. `min_consumed_pieces`, `max_consumed_pieces`, `sample_consumed_pieces`, and `best_remaining_queue_len` come from solver results. Post-PC continuation availability is based on `best_remaining_queue_len >= min_remaining_queue`; if no witnessed solution has enough remaining queue and counting was incomplete, `continuation_available_complete=false` discloses that the negative answer is not exhaustive.
- `clearra-cli continue <token>` is a concrete non-interactive follow-up route.
  It decodes the token into the next interpreted query and sends it through
  `clearra-app`, which validates it, compiles a `SearchProblem`, and executes
  through `CoreExecutor`. Version 1 `pc1`/`sc1` decode support is isolated from
  the solver hot path; current encoders emit version 2 tokens.
- The opening CLI input remains an empty-field opening contract for assembly and validation, but it is not a solver input. `PcQuery -> OpeningPreset -> SearchProblem` is the product path.
- The interpreted scenario input is intentionally target-less. It carries the initial board mask, remaining queue, hold state, piece window, clear-to-empty completion goal, count policy, exact piece constraint, minimum remaining queue, hold permission, and 180 requirement flag so setup examples are validated as scenario searches instead of fake empty-board openings. Its result reports the cleared line count after solving; the cleared line count is not the input mode.
- `TwoLineCapability` means the query satisfies the 2L fast-path conditions.
- `TwoLineFastPathAvailability` means the concrete two-line tables and runner are executable. MVP1 and MVP2 intentionally keep the 2L table fast path out of scope until the table runner and trace bridge are wired together. Capable 2L requests must report `two_line_capable=true`, `two_line_fast_path_available=false`, and `two_line_table_unavailable`; the request still compiles into the normal `SearchProblem` and runs through `CoreExecutor`/C core. It must not claim that the two-line table fast path executed or use the two-line layer to decide PC success.

This follows the useful shape of solution-finder's mature order/data-pool search architecture without copying its package-level responsibility mix into Clearra. The next step toward percent/path coverage is to replace the recursive call stack with the C packing and BuildUp frontier while preserving the Rust reducer distinction between count completeness, trace retention, coverage union, and objective selection.

## SpinTarget Probability Algorithm

Spin target probability uses the same coverage invariant as PC/setup/build
probability:

`SearchProblem -> C PackingProblem -> PackingCandidate -> BuildUpProblem -> BuildVariant enumeration -> ReplayTrace -> SpinClassifier -> SpinTargetPredicate -> CoverageRowKind::SpinTarget -> CoverageMatrix -> PatternBitSet OR -> SpinProbabilityResult`

Required invariants:

- SpinTarget probability follows `PatternBitSet` OR union.
- `SpinTargetPredicate` is not applied before BuildUp.
- `SpinTargetPredicate` never treats a raw `PackingCandidate` as satisfied.
- `SpinTargetPredicate` uses `ReplayEvent`, clear events, `SpinResult`, and
  `KickEvidence`.
- SpinTarget replay is reconstructed from accepted BuildVariant replay evidence:
  operation list, representative order, initial board, line clear events, and
  kick evidence. The runner must not synthesize a hard-coded T operation or a
  fixed 10x4 board when native BuildVariant evidence is missing.
- A profile that needs kick evidence but receives none must report
  estimated/incomplete diagnostics instead of exact results.
- SpinTarget coverage uses `SpinCoverageMatrix` row/word budgets. Score-cell
  coverage uses `ScoreCellMatrix` row/word budgets. Budget failures are result
  completeness issues, not proof that the target is impossible.

Pseudo-code:

```text
for each PackingCandidate:
  enumerate BuildVariant under queue/hold/rule
  for each BuildVariant:
    replay = build_replay_trace(variant)
    spin_result = spin_classifier.classify(replay, profile)
    if spin_target_predicate.satisfied(spin_result, replay):
      row = coverage_row_for_variant(
        variant,
        row_kind = SpinTarget(target_id))
      matrix.add(row)

probability = matrix.union_probability()
```

## Exact Cover And DLX

`clearra-exact-cover` owns generic exact-cover models and the MVP3 DLX/Algorithm X solver. DLX is not a replacement for PC queue/hold/reachability search. It is first used as a tiling and assignment enumerator:

- `GenericExactCoverCandidate` represents interpreted setup/custom-piece tiling candidates with stable piece id, piece area, source cells, and compact DLX columns before they are reduced to solver rows.
- `ExactCoverProblemSchema` generalizes exact cover inputs with `cell_universe`, `PieceUsageConstraint`, `SlotConstraintColumn`, `AreaConstraintColumn`, required columns via `ExactCoverColumnKind::Required`, optional conflict columns via `ExactCoverColumnKind::Optional`, and `ExactCoverCandidateRow`.
- `ExactCoverProblem::with_optional_columns` preserves `required_column_count` and `optional_column_count`; optional columns are conflict constraints, not cells that must be covered.
- `CellUniverseBuilder` builds a sparse cell universe and remaps absolute cells to compact columns, keeping setup tiling and custom piece tiling independent from board bit density.
- `PieceAreaConstraint` checks area feasibility before expensive search, so area infeasible shape rejected before expensive search is a first-class exact-cover guard.
- `DlxSolver::solve_all_limited` uses `DlxSearchLimits` with `max_solutions` and `max_nodes`, then returns solutions plus `complete`, `searched_nodes`, and `truncation_reason` so bounded tiling enumeration never pretends to be exhaustive.
- `SetupTilingBridge::enumerate` remaps sparse setup shape bit indexes to compact exact-cover columns before calling DLX.
- `GenericExactCoverBridge::enumerate_tilings` and `CustomPieceBridge::enumerate_tilings` accept already interpreted custom-piece placement columns and enumerate tilings without parsing raw piece/schema text or invoking PC runtime search.
- `AssignmentExactCoverBridge` in `clearra-build-coverage` models build slot assignment as exact cover over slot columns. The existing CSP remains available; the DLX bridge is an adapter for enumeration, not a CLI responsibility.
- `DlxBuildUpBridge` maps a DLX result to BuildUpProblem by constructing a `CPackingCandidate` from interpreted operation candidates and then calling `CBuildUpProblemBuilder`; the handoff is `DlxSolution -> operation candidates -> BuildUpProblem -> C BuildUp`.
- DLX solution is not a BuildVariant. Line clear, hold, queue, and reachability remain BuildUp responsibilities; exact-cover does not finish PC execution.
- PC completion receives interpreted tiling/build results through
  `clearra-problem` and `clearra-core-executor`. PieceSource order, hold, kicks,
  and reachability are compiled into the C core problem/execution boundary.
- Product test ownership follows the same boundary: SearchProblem compiler
  tests, C Board64/candidate/reachability/packing/BuildUp fixtures, Rust FFI
  safety tests, executor orchestration tests, coverage invariants, and CLI E2E
  tests are the executable solver contract.

## Area Decomposition

`clearra-core-executor::area` owns backend-generic connected-component analysis for executor-side pruning. It is deliberately built on `BoardStateBackend` plus `singleton_mask` instead of raw `u64` masks, so the same `AreaDecomposer`, `AreaScope`, and `ScenarioAreaPruner` work for `Board64`, `Board128`, and `Wide` board states.

Area decomposition is a necessary-condition optimization, not a solver. It can split empty or occupied regions into 4-neighbor components and check whether each component area can be composed from the active piece area set. Standard tetromino rules use area `4`; mixed/custom piece sets can provide other stable areas through `AreaTileabilityRules`.

`AreaMultisetFeasibility` is the MVP3 custom-piece foundation for bounded area checks. It uses the active piece area multiset from `MixedPieceSet` or `MixedBagProfile`, so generic feasibility must not assume `missing_cells % 4 == 0`. The standard tetromino fast path remains the special case with repeated area-4 pieces, while mixed/custom schema paths expose `custom_piece_runtime_not_connected` until generalized placement/search runtime exists.

Scenario pruning must always name its scope. A full 20-row board contains empty sky that is not necessarily part of the completion target, so `ScenarioAreaPruner` accepts an explicit `AreaScope` such as target rows or interpreted target cells. Search code must not prune from whole-board empty components unless the whole board is truly the target region. G5 mirrors this in `AreaScopeDescriptor` and `CompileAreaPruner`: `TargetRows`, `InterpretedTargetCells`, and `WholeBoardTarget` are explicit choices, `EAreaInfeasible` rejects impossible component areas, and `IAreaNecessaryConditionPassed` means only `SearchMayContinue`, never solution found.

## Proof-carrying Pruning

Proof-carrying pruning is the only route for candidate removal. Tier 0 filters
cover exact bounds, collision, target mask overflow, piece count overflow, area
overflow, and row capacity overflow. Tier 1 and Tier 2 domain propagation are
indexed by clear-state, so `CellDomainEmptyUnderClearState` and
`ForcedPieceFamilyUnderClearState` are conditional facts until every reachable
clear-state has been checked. A complete materialized `PieceMultisetFamily`
authorizes only count-vector prefix rejection. Indexed Pull materializes the
complete cell-to-placement incidence table and changes expansion order without
becoming a pruning proof. Tier 3 can exhaust the operation-subset graph to reject
a final candidate only when no line-clear dependency order exists; small
component exact-cover still runs only under `PropagationBudget`. Tier 4 BuildUp
remains the final legality gate for y adjustment, groundedness, reachability,
hold, queue, bag, and actual line-clear execution.

Global pruning requires `ProofLevel::GlobalSafe` and a
`PruningProofLedgerEntry`. Local target-frame floating, MCTS priority, rare-piece
heuristics, unknown spin classification, and score thresholds are not pruning
proofs.
