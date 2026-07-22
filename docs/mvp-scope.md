# MVP Scope

MVP1 supports standard 10-wide boards, standard tetromino pieces, standard 7-bag supply, hold-aware 2L/4L/6L perfect-clear search, token-based `continue` follow-up search, setup search foundations, and build coverage foundations. Custom boards, custom pieces, custom bags, custom kick editors, score optimization, and GUI runtime paths must remain guarded or explicitly unsupported.

MVP2 expands rule/kick, scoring, setup, build editor schema, and typed output contracts, but the dedicated 2L table fast path remains excluded until the table runner and trace bridge are actually connected. The supported 2L contract is capability and availability reporting plus normal `SearchProblem`/C-core execution: `two_line_capable=true`, `two_line_fast_path_available=false`, and `two_line_table_unavailable`.

MVP2 scoring is available as post-processing only with `accuracy_level=basic-approximation`; built-in profile names such as `jstris-ultra` and `tetrio` must not be presented as profile-specific exact implementations until exact spin classification and profile-specific score/attack rules are implemented.

## MVP1

MVP1 remains standard PC/setup/build coverage centered. It supports standard
pieces, standard board assumptions, C Packing/BuildUp, replay/output, and
PatternBitSet union probability. SpinTarget probability, exact special-spin
classification, all-spin exact support, and score-matrix optimization are not
MVP1 product promises.

## MVP2

MVP2 may add the product skeleton for spin and score objectives:

- SpinTarget query skeleton.
- `SpinTargetPredicate` over `ReplayTrace`.
- `CoverageRowKind::SpinTarget` rows using the same PatternBitSet OR invariant.
- `SpinClassifier` object model.
- T-spin corner rule.
- KickEvidence plumbing.
- `SpecialSpinCaseRegistry`.
- `ScoreProfileObjectValidator`.
- TETR.IO source-pinned score fixture.
- T-Spins / T-Spins+ / All-Spin / All-Spin+ / All-Mini / All-Mini+ as independent spin profiles.
- `CandidateScoreStats`.
- `PatternScoreContribution`.
- `SpinProbabilityResult`.
- `BuildUpExecutionMode::EnumerateVariants` for coverage-producing BuildUp.
- C/Rust FFI owned snapshots for scope-bound coverage and KickEvidence views.
- `PatternBitSet` dynamic word allocation budget.
- `SpinCoverageMatrix` and `ScoreCellMatrix` memory budget diagnostics.
- BuildUp enumerate variant limit from `SearchProblemBudget`.
- KickEvidence buffer budget diagnostics.
- Score matrix capacity exceeded diagnostic.

MVP2 exact claims remain guarded. A score profile may be named after an external
game mode only when output still discloses its accuracy level. Exact SpinTarget
queries require classifier capability and trace evidence.

Bitmap rendering is `ConnectedExact`. PNG/GIF consume `ReplayTrace` through an
owner-aware `RenderScene`, validated default PNG atlas, and deterministic
encoders. Pixel and byte-hash goldens guard the exact claim. Runtime raw SVG
rendering remains forbidden; the optional build-time importer owns bounded
sanitize/rasterize/manifest/provenance generation.

### MVP2 Capability Registry

MVP2 capability state must be `Unsupported`, `ConnectedApproximate`, or
`ConnectedExact`. Exact claims require `ConnectedExact`; approximate runtime
algorithms must disclose their accuracy contract.

Required capability ids:

- `RuleKickExpansion`
- `ScoringPostProcessing`
- `SpinTarget`
- `SetupRawMetricsV2`
- `BuildEditorSchema`
- `RendererPngSkeleton`
- `RendererGifSkeleton`
- `GpuPackingStrengthening`
- `HybridScheduler`

Required markers:

- `mvp2_capability_report_lists_all_mvp2_features`
- `mvp2_exact_claims_require_capability_exact`
- `mvp2_unsupported_features_emit_disabled_reason`

Forbidden claims:

- Skeleton을 exact로 표시
- BasicApproximation을 profile-specific exact로 표시
- MVP2 feature failure를 MVP1 failure로 처리

## Security And Capability Honesty

`docs/security-fix-map.md` is release blocking for MVP work that touches C
memory, Rust FFI, GPU worker results, renderer assets, GUI execution, or future
Web/WASM entrypoints. MVP 밖 기능은 동작하는 것처럼 보이면 안 된다: GPU
fallback/trust state, renderer skeleton state, GUI execution mode, capacity
truncation, and memory leak status must be visible through diagnostics or
capability reports.

Required MVP security markers:

- `security_fix_map_mentions_all_known_risks`
- `mvp_out_of_scope_features_must_not_appear_supported`
- `capacity_exceeded_must_not_truncate_without_diagnostic`
- `architecture_validation_rejects_silent_gpu_fallback`
- `architecture_validation_rejects_runtime_raw_svg`
- `architecture_validation_rejects_gui_subprocess`
- `architecture_validation_rejects_unbounded_ffi_pointer_count`

## MVP3 And Later

MVP3 opens the generalization layer while preserving the standard tetromino
fast path. Custom piece, custom bag, custom board, Board128/Wide, DLX, generic
GPU, custom rule editor, custom skin/theme, and GPU BuildUp/score matrix work
must stay behind a separate MVP3 capability registry until each runtime path is
explicitly connected and proven.

### MVP3 Capability Registry

MVP3 uses the same three stable states. A schema without a connected runtime is
`Unsupported`; unsupported features emit a disabled reason. A connected
non-exact algorithm is `ConnectedApproximate`, while exact output requires
`ConnectedExact`.

Required capability ids:

- `CustomPieceSchema`
- `MixedPieceSet`
- `CustomBagProfile`
- `CustomBoardWidth`
- `Board128Runtime`
- `WideBoardRuntime`
- `GenericOperationTable`
- `GenericExactCover`
- `DlxSolver`
- `AreaMultisetFeasibility`
- `CustomRuleEditor`
- `GenericGpuDescriptor`
- `GpuBuildUpExpansion`
- `CustomSkinEditor`

Required markers:

- `mvp3_capability_report_lists_all_generalization_features`
- `schema_only_features_do_not_execute_runtime`
- `unsupported_features_emit_disabled_reason`
- `standard_fast_path_unchanged`

Forbidden claims:

- custom feature를 standard fast path로 조용히 fallback
- generic schema 추가 후 runtime이 연결된 것처럼 표시
- MVP3 cache key가 standard enum만 사용

G1 custom piece domain model:

- `StandardTetrominoPiece` keeps the standard tetromino fast path separate
  from `CustomPieceDefinition`.
- `PieceDefinitionId` is the stable custom/standard piece identity.
- `PieceDefinition` fields include `piece_definition_id`, `display_name`,
  `area`, `rotation_states`, `cells_by_rotation`, `bounds_by_rotation`,
  `spawn_offsets`, `color_hint`, `symmetry_class`, and source/provenance via
  `source_provenance`.
- `PieceSetDefinition` fields include `piece_set_id`, `pieces`,
  `standard_fast_path_compatible`, and `mixed_area_multiset`.
- Runtime execution remains guarded with `custom_piece_runtime_not_connected`
  and `mixed_piece_runtime_not_connected` until generic placement/search
  runtime exists.
- Generic area checks keep
  `missing_cells_mod_4_not_used_for_generic_feasibility`.
- Cache identity includes `piece_definition_id_fingerprint`,
  `piece_area_multiset_fingerprint`, and `piece_set_profile_id`.

G1 markers: `standard_tetromino_fast_path_unchanged`,
`custom_piece_schema_validates`,
`custom_piece_runtime_not_connected_until_runtime_exists`,
`missing_cells_mod_4_not_used_for_generic_feasibility`, and
`piece_definition_id_included_in_cache_keys`.

G2 mixed supply generalization:

- Stable supply kinds are limited to implemented paths: `Standard7Bag`,
  `FixedSequence`, `ObservedWindow`, and `MaterializedPatternUniverse`.
  Unavailable schemas use `UnsupportedExtension(ExtensionId)` and stop before
  C execution.
- Supply provenance carries `supply_provenance_id`, `bag_profile_id`,
  `piece_set_id`, `observed_window_id`, `bag_boundary_evidence`,
  `duplicate_witness`, and `ambiguity_report`.
- `mixed_bag_schema_validates` and `custom_bag_schema_valid` are schema claims,
  not runtime execution claims.
- Runtime execution remains guarded with `custom_bag_runtime_not_connected`
  until generalized supply/runtime support exists.
- `observed_window_ambiguity_reported` keeps observed windows from being
  silently fixed into exact fixed sequences.
- `supply_provenance_in_cache_key` keeps supply identity in cache keys.

G2 markers: `standard_7_bag_path_unchanged`, `mixed_bag_schema_validates`,
`custom_bag_runtime_not_connected_until_runtime_exists`,
`supply_provenance_in_cache_key`, and `observed_window_ambiguity_reported`.

G3 Board128 / Board256 / Wide board runtime:

- Board backend selection is fixed by interpreted cell count:
  `Board64` for `cell_count <= 64`, `Board128` for `65 <= cell_count <= 128`,
  `Board256` for `129 <= cell_count <= 256`, and `Wide` above 256.
- Board64 remains the connected MVP1 packing runtime:
  `runtime_connected=true` and `packing_supported=true`.
- Board128 is descriptor and basic-ops connected only:
  `descriptor_supported=true`, `basic_ops_supported=true`,
  `operation_mask_supported=true`, and `packing_supported=false` with
  `board_backend_not_connected`.
- Board256 is a four-word descriptor and basic-ops contract. Standard 10-wide
  13..24L boards use it without truncation; packing remains unsupported until
  extended packing, BuildUp, reachability, and replay share the same layout.
- `clr_standard_pc_extended_board_descriptor` and its Rust mirror own the full
  initial-board words for 7..24L. The legacy compact descriptor is not an
  extended input adapter.
- Wide is descriptor-only scaffolding:
  `descriptor_supported=true`, `operation_mask_supported=false`, and
  `packing_supported=false` with `wide_board_runtime_not_connected`.
- Unsupported widths and backends must report `board_width_out_of_scope`,
  `board_backend_not_connected`, or `wide_board_runtime_not_connected`; they
  must not truncate to Board64, use only low 64 bits, or return an empty success.

G3 markers: `board64_fast_path_unchanged`, `board128_descriptor_validates`,
`board128_basic_row_mask_collision_place_tests_pass`,
`wide_board_descriptor_validates`,
`wide_board_runtime_not_connected_reports_reason`, and
`unsupported_board_width_silent_fallback_forbidden`.

G4 generic operation / candidate / reachability:

- The standard operation table remains the connected MVP runtime:
  `ClearraOperationTable`, 28 operations, four rotations, area 4, and
  `standard_operation_table_unchanged`.
- Custom operation schema is represented by `CustomPieceOperationTable` and
  `GenericOperationTableDescriptor` in the Rust extension layer only; it is not
  a C/FFI runtime descriptor.
- Unsupported custom operation schemas are rejected before candidate and
  reachability execution.
- Cache identity must include `operation_table_version`,
  `piece_definition_id_fingerprint`, and `piece_area_multiset_fingerprint`.

G4 markers: `custom_operation_table_schema_validates`,
`custom_piece_runtime_not_connected`, and
`cache_key_includes_operation_table_version`.

G5 area multiset feasibility / area decomposition:

- Standard area rules stay explicit with `StandardTetrominoAreaRule` and
  `standard_area4_fast_path_unchanged`.
- Generic custom/mixed checks use `AreaMultisetFeasibility` over the
  `active_piece_area_multiset` with `bounded_area_subset_sum`.
- Scenario pruning must name `AreaScopeDescriptor`: `TargetRows`,
  `InterpretedTargetCells`, or `WholeBoardTarget`.
- Whole-board scope is valid only when the whole board is truly the target
  region; empty sky must not become a default completion target.
- Area decomposition is a necessary condition only. `SearchMayContinue` is not
  a solution, and validation reports `area_feasible_is_solution_found=false`.

G5 markers: `area_multiset_feasibility_uses_piece_area_multiset`,
`scenario_area_pruner_requires_explicit_area_scope`,
`area_decomposition_is_necessary_condition_not_solver`,
`EAreaInfeasible`, and `IAreaNecessaryConditionPassed`.

G6 generic exact-cover / DLX:

- `ExactCoverProblemSchema` is the typed boundary for generic exact-cover
  inputs. It carries `cell_universe`, `PieceUsageConstraint`,
  `SlotConstraintColumn`, `AreaConstraintColumn`, required columns through
  `ExactCoverColumnKind::Required`, optional conflict columns through
  `ExactCoverColumnKind::Optional`, and `ExactCoverCandidateRow`.
- `ExactCoverProblem::with_optional_columns` lowers the schema while preserving
  `required_column_count` and `optional_column_count`.
- DLX limits are explicit through `DlxSearchLimits`, `max_solutions`, and
  `max_nodes`; reports expose `complete`, `searched_nodes`, and
  `truncation_reason`.
- `BuildExactCoverProblemBridge` and `SetupTilingExactCover` are adapters.
  They do not own line-clear, hold, queue, or reachability decisions.
- The C/Rust handoff remains `DlxSolution -> operation candidates -> BuildUpProblem -> C BuildUp`.
  DLX solution is not a BuildVariant, and truncated DLX output is not complete.

G6 markers: `generic_exact_cover_candidate_schema_validates`,
`dlx_solver_returns_complete_flag`,
`area_infeasible_shape_rejected_before_search`,
`dlx_result_maps_to_buildup_problem`, and
`standard_setup_tiling_still_works`.

G7 BuildUp runtime scope:

- MVP1 remains the fixed Board64 BuildUp path with
  `mvp1_buildup_15_operation_fast_path_unchanged` and
  `C_BUILDUP_MAX_OPERATIONS = 15`.
- Stable ABI contains only the connected Board64 state used by the runtime.
- Board128/Wide BuildUp returns `CLR_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE`;
  Board256 follows the same guard instead of falling back to Board64.
- operation_count > 15 is guarded, not truncated.
- Unsupported BuildUp scope cannot claim a solution.

G7 markers: `operation_count_above_runtime_limit_is_unsupported`,
`board128_buildup_guard_reports_unsupported`, and
`unsupported_buildup_scope_does_not_claim_solution`.

G8 custom rule editor:

- `CustomRuleEditorSchema` owns the editable schema for `rotation_states`,
  `spawn_rules`, `kick_transitions`, `first_success_order`, `supports_180`,
  `piece_specific_overrides`, `line_clear_policy`, and
  `lock_reachability_mode`.
- `CustomRuleVerificationReport` exposes `missing_transition`,
  `duplicate_transition`, `invalid_rotation`, `unsupported_piece`,
  `unsupported_board_backend`, and `unsupported_runtime_feature`.
- `VerifiedCustomRuleProfile` is required before any FFI descriptor compile.
- `CustomRuleDescriptorCompiler` rejects raw editor schema with
  `unverified_custom_rule_rejected_before_execution`.
- Unsupported custom rule execution surfaces report explicit reasons and are not
  mapped to SRS.

G8 markers: `custom_rule_editor_schema_validates`,
`custom_rule_verify_reports_missing_transition`,
`custom_rule_verify_reports_duplicate_transition`,
`verified_custom_rule_can_compile_to_descriptor_when_supported`, and
`unverified_custom_rule_rejected_before_execution`.

G9 generic GPU / GPU BuildUp:

- Default capability is `Unsupported` with
  `generic_gpu_descriptor_not_connected`.
- Generic descriptor and GPU BuildUp subset types are absent from the default C
  ABI, Rust FFI, and WebGPU package.
- A future runtime must be a default-off package with its own exactness and CPU
  confirmation gate. It may not fall back to the standard descriptor or
  truncate custom board masks.

G10 custom skin / theme editor:

- `CustomSkinThemeSchema` owns `skin_id`, `palette_id`, `piece_mapping`,
  `grid_style`, `background`, `line_clear_highlight`,
  `ownership_color_mode`, `export_limits`, and `provenance`.
- User-imported skin assets are stored in `user_config_directory` or
  `user_cache_directory`; they are not repository assets.
- Manifest and provenance/import report are required before a user-imported
  asset can be shown in the editor.
- Theme preview uses a PNG atlas only. `runtime_raw_svg_allowed=false`, and raw
  SVG is not passed to the runtime renderer.

T guarded expansion surface:

- UI-facing capability schemas use `Unsupported`, `ConnectedApproximate`, and
  `ConnectedExact`.
- Runtime execution requires a connected state; exact claims require
  `ConnectedExact`.
- Custom piece schema remains guarded by
  `custom_piece_schema_validates_but_runtime_guarded`, while cache identity
  includes `piece_definition_id_fingerprint`,
  `piece_area_multiset_fingerprint`, and `piece_set_profile`.
- Custom bag schema remains guarded by `CustomBagRuntimeGuard`;
  `custom_bag_not_silent_standard_fallback` and
  `custom_bag_runtime_not_connected` prevent standard 7-bag fallback.
- Board128/Board256/Wide descriptors validate in their own ranges and never
  truncate to Board64.
- Built-in SRS+ is exact for its pinned symmetric I-piece and transition-specific
  180-degree kick table; `supports_exact_180=true`.
- User-imported assets must not be displayed as built-in assets without
  provenance.

G10 markers: `custom_skin_schema_validates`,
`custom_skin_import_requires_provenance`,
`custom_theme_preview_uses_png_atlas`, and
`raw_svg_not_passed_to_runtime_renderer`.

G11 MVP3 acceptance gate:

- MVP1 ProductE2E runs first so MVP3 schema work cannot change the standard
  product path without being noticed.
- MVP2 Acceptance runs before MVP3-specific gates to prove MVP3 expansion does
  not invalidate MVP2 guarded exactness and preview claims.
- The MVP3 gate includes custom piece schema tests, mixed bag schema tests,
  Board128/Wide descriptor tests, area multiset feasibility tests, DLX tests,
  custom rule editor validation tests, generic GPU descriptor tests, and
  unsupported runtime guard tests.
- Generic operation, generic BuildUp, and custom skin/theme checks are also
  included because they are part of the current MVP3 guard surface.
- `standard_fast_path_unchanged_under_mvp3`,
  `custom_features_guarded_until_runtime_connected`,
  `no_silent_fallback_to_standard_path`, and
  `generic_cache_keys_include_piece_board_rule_supply_identity` are release
  markers.
- MVP3 feature 추가로 MVP1 PC 결과 변경, custom unsupported를 empty success로
  처리, and standard와 generic cache key 충돌 are forbidden.

MVP3+ owns the exact special-spin and generalized spin roadmap:

- `VerifiedSpecialSpinProfile` import.
- Fin / ISO / NEO source-pinned exact fixtures.
- all-piece spin classifier exact support.
- GPU score matrix.
- generic custom-piece spin classification.
- Board128/Wide spin classification.

Validation guards:

- Fin/ISO/NEO exact profile without verified fixture -> disabled reason.
- AllSpin profile without all-piece classifier -> disabled reason.
- SpinTarget query with missing SpinClassifier capability -> validation error.
- exact score profile with trace-completeness mismatch -> validation error.
- score/spin matrix capacity exceeded without incomplete-output policy -> error.
- BuildUp enumeration truncation in strict exact query -> error.
- coverage capacity exceeded -> `E_COVERAGE_CAPACITY_EXCEEDED`.
- BuildUp variant enumeration truncation used as exact evidence ->
  `E_BUILDUP_VARIANT_ENUMERATION_TRUNCATED`.
- observed queue truncation -> `W_OBSERVED_QUEUE_PROBABILITY_INCOMPLETE` with
  `renormalized=false`.
- trace retention truncation -> `W_TRACE_RETENTION_TRUNCATED`, not
  `count_truncated_reason`.
