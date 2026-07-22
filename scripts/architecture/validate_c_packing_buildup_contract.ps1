# This file is dot-sourced by an architecture validation wrapper.
# Keep the grouped validation functions side-effect free at load time.
function Invoke-CGeometryPackingValidation() {
foreach ($requiredPath in @(
        "core-c/src/packing/packing_problem.h",
        "core-c/include/clr_resource_budget.h",
        "core-c/src/resource/resource_budget.c",
        "core-c/src/resource/resource_report.c",
        "core-c/src/packing/placement_candidate.c",
        "core-c/src/packing/geometry_catalog.c",
        "core-c/src/packing/geometry_catalog_internal.h",
        "core-c/src/packing/geometry_exact_cover.c",
        "core-c/src/packing/geometry_residual_memo.c",
        "core-c/src/packing/geometry_solution_graph.c",
        "core-c/src/packing/geometry_buildable_stream.c",
        "core-c/src/packing/packing_candidate_materializer.c",
        "core-c/src/packing/packing_candidate_buffer.c",
        "core-c/src/packing/packing_pruner.c",
        "core-c/src/packing/tiling_key.c",
        "core-c/src/packing/tiling_key.c",
        "core-c/src/packing/tiling_key.c",
        "core-c/src/packing/packing_deduper.c",
        "core-c/src/packing/packing_deduper.c",
        "core-c/src/packing/packing_deduper.c",
        "core-c/tests/packing_tests.c",
        "core-c/tests/geometry_exact_cover_tests.c",
        "tests/fixtures/packing/harddrop_candidates.json",
        "tests/fixtures/packing/locked_candidates.json",
        "tests/fixtures/packing/locked180_candidates.json",
        "tests/fixtures/packing/kick_first_success_candidates.json",
        "tests/fixtures/packing/unreachable_but_collision_free.json"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M10 geometry packing required file is missing: $requiredPath"
        }
    }
$packingHeader = Read-Text "core-c/src/packing/packing_problem.h"
foreach ($requiredMarker in @(
        "ClearraPackingCandidateView",
        "clearra_placement_candidates_generate",
        "clearra_geometry_catalog_compile",
        "clearra_geometry_exact_cover_search_graph",
        "clearra_geometry_solution_graph_stream_buildable_task",
        "clearra_geometry_catalog_rows_buildable_to_sink",
        "clearra_packing_materialize_catalog_row_ids",
        "clearra_packing_shape_key",
        "clearra_packing_tiling_key",
        "clearra_packing_operation_set_key",
        "clearra_packing_hash_confirm_exact",
        "clearra_packing_deduper_push_unique"
    )) {
        if ($packingHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "packing_problem.h must expose M10 geometry/buildability marker '$requiredMarker'"
        }
    }
    $buildableStream = Read-Text "core-c/src/packing/geometry_buildable_stream.c"
    foreach ($requiredMarker in @(
            "clearra_geometry_solution_graph_stream_buildable_task",
            "clearra_geometry_catalog_rows_buildable_to_sink",
            "clearra_buildup_exists_catalog_rows_with_constraints_and_workspace",
            "clearra_packing_materialize_catalog_row_ids",
            "required_predecessors",
            "candidate_buildable"
        )) {
        if ($buildableStream -notlike "*$requiredMarker*") {
            Add-ArchitectureError "geometry_buildable_stream.c must enforce BuildUp-before-materialization marker '$requiredMarker'"
        }
    }
    $exactCover = (Read-Text "core-c/src/packing/geometry_catalog.c") +
        (Read-Text "core-c/src/packing/target_frame_geometry_domain.c") +
        (Read-Text "core-c/src/packing/geometry_exact_cover.c") +
        (Read-Text "core-c/src/packing/geometry_residual_memo.c") +
        (Read-Text "core-c/src/packing/geometry_solution_graph.c") +
        (Read-Text "core-c/src/packing/geometry_full_placement_domain.c")
foreach ($requiredMarker in @(
            "clearra_geometry_catalog_compile",
            "clearra_geometry_exact_cover_search_to_sink",
            "clearra_geometry_exact_cover_search_graph",
            "clr_resource_report_mark_truncated",
            "clearra_placement_candidates_generate",
            "pivot_cell",
            "cell_support_offsets",
            "clearra_geometry_residual_memo_lookup",
            "clearra_geometry_residual_memo_insert",
            "selected_rows",
            "used_piece_counts"
        )) {
        if ($exactCover -notlike "*$requiredMarker*") {
            Add-ArchitectureError "geometry exact-cover engine must implement marker '$requiredMarker'"
        }
    }
    $catalogSource = Read-Text "core-c/src/packing/geometry_catalog.c"
    foreach ($requiredMarker in @("problem->board.search_height", "search_height == 0u", "problem->board.visible_height")) {
        if ($catalogSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "geometry_catalog.c layout must prefer search_height with visible_height fallback marker '$requiredMarker'"
        }
    }
    if ($catalogSource -like "*height = problem->board.visible_height;*") {
        Add-ArchitectureError "geometry_catalog.c must not use visible_height without a zero search_height compatibility guard"
    }
$packingProblem = Read-Text "core-c/src/problem/packing_problem.c"
foreach ($requiredMarker in @("problem->board.search_height", "search_mask", "goal_region_mask & ~search_mask", "required_fill_mask & ~problem->goal_region_mask", "initial_mask & ~search_mask")) {
        if ($packingProblem -notlike "*$requiredMarker*") {
            Add-ArchitectureError "packing_problem.c must validate packing masks against search_height universe marker '$requiredMarker'"
        }
    }
$shapeKey = Read-Text "core-c/src/packing/tiling_key.c"
if ($shapeKey -notlike "*clearra_packing_shape_key*") {
        Add-ArchitectureError "tiling_key.c must implement M10 shape key stable marker"
    }
$tilingKey = Read-Text "core-c/src/packing/tiling_key.c"
foreach ($requiredMarker in @("clearra_packing_tiling_key", "sort_masks")) {
        if ($tilingKey -notlike "*$requiredMarker*") {
            Add-ArchitectureError "tiling_key.c must implement M10 tiling key stable marker '$requiredMarker'"
        }
    }
$hashConfirm = Read-Text "core-c/src/packing/packing_deduper.c"
if ($hashConfirm -notlike "*clearra_packing_hash_confirm_exact*") {
        Add-ArchitectureError "packing_deduper.c must implement M10 hash collision exact confirm marker"
    }
$frontier = Read-Text "core-c/src/packing/geometry_exact_cover.c"
foreach ($requiredMarker in @(
        "remaining_cells",
        "packed_counts",
        "propagation.pivot_cell",
        "cell_support_row_ids",
        "partition_owns_prefix",
        "clearra_geometry_residual_memo_lookup"
    )) {
        if ($frontier -notlike "*$requiredMarker*") {
            Add-ArchitectureError "geometry exact-cover must keep exact continuation marker '$requiredMarker'"
        }
    }
$deduper = Read-Text "core-c/src/packing/packing_deduper.c"
foreach ($requiredMarker in @("clearra_packing_hash_bucket", "clearra_packing_hash_confirm_exact", "clearra_packing_candidate_buffer_push")) {
        if ($deduper -notlike "*$requiredMarker*") {
            Add-ArchitectureError "packing_deduper.c must implement M10 dedupe marker '$requiredMarker'"
        }
    }
$packingTests = Get-PackingTestsValidationSurface
foreach ($requiredMarker in @(
        "two_line_empty_board_packing_candidates_generated",
        "placement_candidate_preserves_same_mask_different_rotation",
        "same_mask_different_operation_id_not_dropped_before_buildup",
        "placement_geometry_class_retains_operation_variants",
        "build_up_tries_next_operation_variant_when_first_variant_unreachable",
        "kick_sensitive_replay_not_lost_by_mask_dedupe",
        "custom_piece_same_mask_different_definition_not_deduped",
        "candidate_buffer_is_soa",
        "shape_key_stable",
        "tiling_key_stable",
        "shape_key_does_not_drop_tiling_variant",
        "tiling_key_does_not_drop_build_variant",
        "same_mask_different_piece_definition_not_same_tiling",
        "operation_set_key_stable",
        "hash_collision_exact_confirm_works",
        "geometry_catalog_collapses_skeletons_without_losing_realizations",
        "geometry_catalog_view_is_pointer_stable_across_search",
        "geometry_catalog_identity_is_independent_of_piece_multiset",
        "exact_cover_partition_union_matches_serial",
        "residual_memo_requires_exact_piece_counts",
        "residual_memo_saturation_keeps_search_authority",
        "packing_deduper_preserves_distinct_operation_sets",
        "geometry_catalog_collapses_skeletons_without_losing_realizations",
        "geometry_catalog_view_is_pointer_stable_across_search",
        "candidate_identity_includes_final_board_and_cleared_lines",
        "packing_candidate_is_not_solution_before_buildup",
        "problem_descriptor_uses_search_height_for_layout_when_visible_height_differs"
    )) {
        if ($packingTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "packing_tests.c must verify M10 marker '$requiredMarker'"
        }
    }
$problemDescriptorTests = Read-Text "core-c/tests/problem_descriptor_tests.c"
if ($problemDescriptorTests -notlike "*packing_problem_masks_are_validated_against_search_height*") {
        Add-ArchitectureError "problem_descriptor_tests.c must verify masks above visible height but inside search height remain valid"
    }
$cmake = Read-Text "core-c/cmake/source_manifest.cmake"
    foreach ($requiredMarker in @("resource_budget.c", "resource_report.c", "placement_candidate.c", "geometry_catalog.c", "geometry_exact_cover.c", "geometry_residual_memo.c", "geometry_solution_graph.c", "geometry_buildable_stream.c", "packing_candidate_materializer.c", "packing_candidate_buffer.c", "packing_pruner.c", "tiling_key.c", "packing_deduper.c")) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c source manifest must build exact geometry source '$requiredMarker'"
        }
    }
    $testingBoundary = $cmake.IndexOf("if(BUILD_TESTING)")
    foreach ($checkpointSource in @(
            "src/gpu/gpu_backend.c",
            "src/gpu/gpu_readback_reduce.c",
            "src/gpu/gpu_host_confirm.c",
            "src/packing/cpu_packing_reference.c",
            "src/packing/packing_enumerator_cpu.c"
        )) {
        $sourceIndex = $cmake.IndexOf($checkpointSource)
        if ($testingBoundary -lt 0 -or $sourceIndex -lt $testingBoundary) {
            Add-ArchitectureError "checkpoint adapter must be isolated under BUILD_TESTING: $checkpointSource"
        }
    }
$placementCandidate = Read-Text "core-c/src/packing/placement_candidate.c"
foreach ($requiredMarker in @(
        "existing->rotation == candidate.rotation",
        "existing->operation_id == candidate.operation_id",
        "existing->x == candidate.x",
        "existing->y == candidate.y"
    )) {
        if ($placementCandidate -notlike "*$requiredMarker*") {
            Add-ArchitectureError "placement candidate dedupe must preserve operation variants marker '$requiredMarker'"
        }
    }
$packingProblemHeader = Read-Text "core-c/src/packing/packing_problem.h"
$tilingKeySource = Read-Text "core-c/src/packing/tiling_key.c"
foreach ($forbiddenMarker in @("clearra_packing_tiling_key(")) {
        if ($packingProblemHeader -like "*$forbiddenMarker*" -or
            $tilingKeySource -like "*$forbiddenMarker*") {
            Add-ArchitectureError "masks-only tiling identity must be named CellPartitionKey; forbidden marker '$forbiddenMarker'"
        }
    }
foreach ($requiredMarker in @("clearra_packing_cell_partition_key", "clearra_packing_tiling_key_with_piece_identity")) {
        if ($packingProblemHeader -notlike "*$requiredMarker*" -or
            $tilingKeySource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "packing key surface must preserve P3 marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
    foreach ($requiredMarker in @("M10 Geometry Skeleton Exact Cover", "immutable placement catalog", "pointer-stable solution-family", "pattern-specific BuildUp", "accepted-row-only materialization", "BUILD_TESTING", "board.search_height", "search-height mask universe")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M10 packing marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
    foreach ($requiredMarker in @("C geometry packing compiles an immutable catalog", "clearra_geometry_exact_cover_search_graph", "clearra_geometry_solution_graph_stream_buildable_task", "Geometry paths are not solutions", "BuildUp-accepted catalog rows", "search-height Board64 universe", "display metadata rather than a packing boundary")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M10 packing marker '$requiredMarker'"
        }
    }
}
function Invoke-CHostReducerValidation() {
foreach ($requiredPath in @(
        "crates/clearra-core-ffi/src/native/packing_candidate_sink.rs",
        "crates/clearra-core-ffi/src/packing_candidate_batch.rs",
        "core-c/src/packing/packing_deduper.c"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M11 Host Reducer required file is missing: $requiredPath"
        }
    }
    $productReducer = Read-Text "crates/clearra-core-ffi/src/native/packing_candidate_sink.rs"
    foreach ($requiredMarker in @(
        "NativeCandidateReducer",
        "bucket_heads",
        "next_indices",
        "exact_candidate_matches",
        "canonicalize_identities",
        "CandidateBudgetExceeded",
        "MemoryExceeded"
    )) {
        if ($productReducer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "NativeCandidateReducer must preserve M11 exact reducer marker '$requiredMarker'"
        }
    }
    $candidateBatch = Read-Text "crates/clearra-core-ffi/src/packing_candidate_batch.rs"
    foreach ($requiredMarker in @("exact_candidate_matches", "operation_dictionary.entry", "candidate.operations", "final_board", "canonicalize_identities")) {
        if ($candidateBatch -notlike "*$requiredMarker*") {
            Add-ArchitectureError "PackingCandidateBatch exact comparison must include '$requiredMarker'"
        }
    }
    $checkpointReducer = Read-Text "core-c/src/packing/packing_deduper.c"
    foreach ($requiredMarker in @("clearra_packing_hash_confirm_exact", "clearra_packing_host_reduce", "raw_to_canonical_ids")) {
        if ($checkpointReducer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "BUILD_TESTING reducer checkpoint must preserve '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
    foreach ($requiredMarker in @("M11 Host Reducer", "NativeCandidateReducer", "BuildUp-accepted catalog row sets", "exact_candidate_matches", "Canonical IDs", "BUILD_TESTING")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M11 host reducer marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
    foreach ($requiredMarker in @("product reducer consumes only BuildUp-accepted catalog rows", "hash bucket", "exact piece", "raw CPU/GPU", "equivalence")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M11 host reducer marker '$requiredMarker'"
        }
    }
}
function Invoke-CBuildUpProblemBuilderValidation() {
foreach ($requiredPath in @(
        "core-c/include/clr_problem.h",
        "core-c/src/problem/buildup_problem.c",
        "core-c/src/buildup/buildup_memo.c",
        "core-c/src/buildup/buildup_operation_source.c",
        "core-c/src/buildup/buildup_workspace.c",
        "core-c/tests/buildup_tests.c",
        "crates/clearra-core-ffi/src/problem/buildup_problem_builder.rs",
        "crates/clearra-core-ffi/src/packing_problem.rs"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M12 BuildUp Problem Builder required file is missing: $requiredPath"
        }
    }
$problemHeader = Read-Text "core-c/include/clr_problem.h"
foreach ($requiredMarker in @(
        "clr_buildup_operation",
        "clr_buildup_operation_set",
        "representative_order_hint",
        "clr_bag_window",
        "initial_board",
        "operation_set",
        "queue",
        "hold",
        "bag_window",
        "rule",
        "line_clear_policy",
        "piece_window",
        "goal",
        "coverage_pattern_id",
        "clr_buildup_problem_is_valid"
    )) {
        if ($problemHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clr_problem.h must expose M12 BuildUpProblem field marker '$requiredMarker'"
        }
    }
$buildupProblem = Read-Text "core-c/src/problem/buildup_problem.c"
foreach ($requiredMarker in @(
        "clearra_buildup_problem_from_packing_candidate",
        "copy_packing_fields",
        "coverage_pattern_id",
        "representative_order_hint",
        "CLR_LINE_CLEAR_POLICY_STANDARD",
        "clr_buildup_problem_is_valid"
    )) {
        if ($buildupProblem -notlike "*$requiredMarker*") {
            Add-ArchitectureError "buildup_problem.c must implement M12 conversion marker '$requiredMarker'"
        }
    }
$buildupState = Read-Text "core-c/src/buildup/buildup_memo.c"
foreach ($requiredMarker in @("clearra_buildup_state_initial", "initial_board.initial_mask", "hold_automaton_state = problem->initial_hold_automaton")) {
        if ($buildupState -notlike "*$requiredMarker*") {
            Add-ArchitectureError "buildup_state.c must implement M12 initial state marker '$requiredMarker'"
        }
    }
$ffiCandidate = Read-Text "crates/clearra-core-ffi/src/packing_problem.rs"
foreach ($requiredMarker in @("CPackingOperation", "CPackingCandidate", "operation_count", "operations", "ffi_packing_candidate_uses_c_layout_without_solution_count")) {
        if ($ffiCandidate -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-core-ffi packing_problem.rs must expose M12 candidate operation-set marker '$requiredMarker'"
        }
    }
    $ffiBuilder = Read-Text "crates/clearra-core-ffi/src/problem/buildup_problem_builder.rs"
    foreach ($requiredMarker in @("CBuildUpProblemTemplate", "compile_for_standard_bag_automaton", "new_scratch", "new_standard_bag_automaton_scratch", "configure_piece_source_pattern", "attach_geometry_catalog", "from_packing_candidate", "C_LINE_CLEAR_POLICY_STANDARD")) {
        if ($ffiBuilder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "CBuildUpProblemBuilder must implement M12 marker '$requiredMarker'"
        }
    }
    $operationSource = Read-Text "core-c/src/buildup/buildup_operation_source.c"
    foreach ($requiredMarker in @("clearra_buildup_operation_source_from_catalog_rows", "catalog_row_ids", "required_predecessors", "clearra_buildup_operation_source_operation_at")) {
        if ($operationSource -notlike "*$requiredMarker*") {
            Add-ArchitectureError "catalog-row BuildUp operation source must preserve '$requiredMarker'"
        }
    }
$buildupTests = Get-BuildUpTestsValidationSurface
foreach ($requiredMarker in @("packing_candidate_converts_to_buildup_problem", "clearra_buildup_problem_from_packing_candidate", "buildup_state_starts_from_problem_initial_board_hold_and_cursor")) {
        if ($buildupTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/tests/buildup_tests.c must verify M12 marker '$requiredMarker'"
        }
    }
$cmake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
        "src/buildup/buildup_search.c",
        "src/buildup/buildup_memo.c",
        "src/buildup/buildup_memo.c",
        "src/problem/buildup_problem.c"
    )) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must build M12 source '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
    foreach ($requiredMarker in @('M12 BuildUp Problem Builder', 'CBuildUpProblemTemplate', 'Geometry workers attach', 'borrowed catalog row IDs', 'exact predecessor constraints', 'does not copy a complete candidate', 'cannot authorize initial candidate materialization')) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M12 marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
    foreach ($requiredMarker in @('CBuildUpProblemTemplate', 'NativeGeometryCatalog', 'borrowed row IDs plus predecessor constraints', 'clearra_buildup_exists_catalog_rows_with_constraints_and_workspace', 'Candidate materialization follows this exact gate', 'downstream interpretation path')) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M12 marker '$requiredMarker'"
        }
    }
}
function Invoke-CBuildUpVerifierValidation() {
foreach ($requiredPath in @(
        "core-c/src/buildup/buildup_worker.c",
        "core-c/src/buildup/buildup_search.c",
        "core-c/src/buildup/buildup_search.h",
        "core-c/src/buildup/buildup_bfs_state.h",
        "core-c/src/buildup/buildup_search.c",
        "core-c/src/buildup/buildup_memo.c",
        "core-c/src/buildup/buildup_memo.c",
        "core-c/src/buildup/buildup_memo.c",
        "core-c/src/buildup/buildup_order_dag.c",
        "core-c/src/buildup/buildup_order_dag.c",
        "core-c/src/buildup/y_adjustment.c",
        "core-c/src/buildup/grounded_filter.c",
        "core-c/src/buildup/reachability_bridge.c",
        "core-c/src/buildup/hold_queue_verifier.c",
        "core-c/src/buildup/build_variant_buffer.c",
        "core-c/tests/buildup_tests.c",
        "tests/fixtures/buildup/queue_order_impossible.json",
        "tests/fixtures/buildup/queue_order_mismatch.json",
        "tests/fixtures/buildup/hold_disabled_impossible.json",
        "tests/fixtures/buildup/hold_branch_required.json",
        "tests/fixtures/buildup/line_clear_y_adjustment_impossible.json",
        "tests/fixtures/buildup/srs_reachability_impossible.json",
        "tests/fixtures/buildup/valid_packing_valid_buildup.json"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M13 CPU BuildUp Verifier required file is missing: $requiredPath"
        }
    }
$problemHeader = Read-Text "core-c/include/clr_problem.h"
foreach ($requiredMarker in @(
        "clr_buildup_status",
        "CLR_BUILDUP_QUEUE_ORDER_IMPOSSIBLE",
        "CLR_BUILDUP_HOLD_DISABLED_IMPOSSIBLE",
        "CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE",
        "CLR_BUILDUP_REACHABILITY_IMPOSSIBLE",
        "clr_buildup_verification",
        "clr_build_variant_buffer",
        "clr_buildup_worker_verify",
        "clr_buildup_worker_verify_into_buffer"
    )) {
        if ($problemHeader -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clr_problem.h must expose M13 BuildUp verifier marker '$requiredMarker'"
        }
    }
$worker = Read-Text "core-c/src/buildup/buildup_worker.c"
$search = (Read-Text "core-c/src/buildup/buildup_search.c") + "`n" +
        (Read-Text "core-c/src/buildup/buildup_search.h")
$buildupVerifierSurface = $worker + "`n" + $search
foreach ($requiredMarker in @(
        "clearra_buildup_order_from_problem",
        "clearra_buildup_queue_hold_consume",
        "clearra_buildup_queue_hold_enumerate_branches",
        "clearra_buildup_adjust_operation_for_line_clears",
        "clearra_buildup_check_line_clear_dependency",
        "clearra_buildup_grounded_filter_accepts",
        "clearra_buildup_reachability_bridge_accepts",
        "verify_goal",
        "clearra_build_variant_from_state"
    )) {
        if ($buildupVerifierSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "buildup worker/search surface must orchestrate M13 verifier marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
        "stop_after_first_success",
        "preserve_hold_branches",
        "enumerated_variant_count",
        "search_record_success",
        "out_variants",
        "out_report->retained_variant_count = 0u",
        "CLR_BUILDUP_ENUMERATION_TRUNCATED"
    )) {
        if ($buildupVerifierSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "buildup worker/search surface must implement full BuildUp enumerate/count traversal marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
        "clr_buildup_worker_verify_into_buffer(problem, out_variants",
        "clr_buildup_enumerate_variants(problem, &enumeration_limits"
    )) {
        if ($worker -like "*$forbiddenMarker*") {
            Add-ArchitectureError "buildup_worker.c must not route enumerate/count modes through first-witness helper '$forbiddenMarker'"
        }
    }
foreach ($requiredMarker in @("problem->initial_board.search_height", "height == 0", "problem->initial_board.visible_height")) {
        if ($search -notlike "*$requiredMarker*") {
            Add-ArchitectureError "buildup_search.c layout_from_problem must prefer search_height with visible_height fallback marker '$requiredMarker'"
        }
    }
if ($search -like "*uint16_t height = problem->initial_board.visible_height*") {
        Add-ArchitectureError "buildup_search.c layout_from_problem must not resolve BuildUp layout from visible_height before search_height"
    }
$variantBuffer = Read-Text "core-c/src/buildup/build_variant_buffer.c"
foreach ($requiredMarker in @("verification->accepted", "CLR_BUILDUP_MAX_VARIANTS", "clearra_build_variant_from_state")) {
        if ($variantBuffer -notlike "*$requiredMarker*") {
            Add-ArchitectureError "build_variant_buffer.c must only push accepted BuildUp variants and expose marker '$requiredMarker'"
        }
    }
$reachabilityBridge = Read-Text "core-c/src/buildup/reachability_bridge.c"
foreach ($requiredMarker in @(
        "clearra_reachability_kick_table_from_rule",
        "&problem->rule",
        "&kick_table",
        "clearra_reachability_check"
    )) {
        if ($reachabilityBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "reachability_bridge.c must pass compact rule kick tables to BuildUp reachability marker '$requiredMarker'"
        }
    }
if ($reachabilityBridge -like "*clearra_reachability_check*reachability_mode_for_rule(&problem->rule), 0, &report*") {
        Add-ArchitectureError "reachability_bridge.c must not pass a null kick table to clearra_reachability_check"
    }
$buildupTests = Get-BuildUpTestsValidationSurface
foreach ($requiredMarker in @(
        "packing_possible_but_queue_order_impossible_fixture",
        "packing_possible_but_hold_disabled_impossible_fixture",
        "packing_possible_but_line_clear_y_adjustment_impossible_fixture",
        "packing_possible_but_srs_reachability_impossible_fixture",
        "buildup_reachability_bridge_uses_no_kick_srs_srs_plus_and_imported_tables",
        "CLR_KICK_NO_KICK",
        "CLR_KICK_SRS_90",
        "CLR_KICK_SRS_PLUS_180",
        "CLR_KICK_IMPORTED",
        "buildup_worker_uses_search_height_when_visible_height_differs",
        "valid_packing_and_valid_buildup_becomes_build_variant_fixture",
        "buildup_enumerate_variants_returns_expected_count_for_two_operation_fixture",
        "buildup_count_variants_matches_enumerate_variants_for_small_fixture",
        "buildup_enumerate_variants_preserves_hold_branch_kind",
        "enumerate_variants_truncates_after_limit_without_losing_prefix",
        "EXPECT_U64(variants->count, 2)",
        "EXPECT_U64(report.total_variant_count, variants->count)",
        "EXPECT_U64(variants->count, 120)",
        "EXPECT_U64(count_report.total_variant_count, 120)",
        "CLR_BUILDUP_QUEUE_ORDER_IMPOSSIBLE",
        "CLR_BUILDUP_HOLD_DISABLED_IMPOSSIBLE",
        "CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE",
        "CLR_BUILDUP_REACHABILITY_IMPOSSIBLE"
    )) {
        if ($buildupTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/tests/buildup_tests.c must verify M13 fixture marker '$requiredMarker'"
        }
    }
$cmake = Read-Text "core-c/CMakeLists.txt"
foreach ($requiredMarker in @(
        "src/buildup/buildup_worker.c",
        "src/buildup/buildup_order_dag.c",
        "src/buildup/buildup_order_dag.c",
        "src/buildup/y_adjustment.c",
        "src/buildup/grounded_filter.c",
        "src/buildup/reachability_bridge.c",
        "src/buildup/hold_queue_verifier.c",
        "src/buildup/build_variant_buffer.c",
        "src/reachability/kick_first_success.c"
    )) {
        if ($cmake -notlike "*$requiredMarker*") {
            Add-ArchitectureError "core-c/CMakeLists.txt must build M13 source '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M13 CPU BuildUp Verifier", "only BuildUp-verified rows become", "BuildUp을 통과한 결과만 BuildVariant가 된다", "packing possible but queue order impossible", "valid packing plus valid BuildUp")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M13 marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
foreach ($requiredMarker in @("C CPU BuildUp verification", "clr_buildup_worker_verify", "operation order", "line clear dependency", "y adjustment", "groundedness", "reachability", "queue order", "hold decision", "bag pattern", "piece window", "goal satisfaction")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M13 marker '$requiredMarker'"
        }
    }
}
