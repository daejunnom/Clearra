# This file is dot-sourced by Invoke-WorkspaceSurfaceArchitectureValidation.
# It intentionally contains ordered validation statements, not a standalone entrypoint.

$cargoConfig = Read-Text ".cargo/config.toml"
$activeCargoConfig = ($cargoConfig -split "`r?`n" |
    Where-Object { $_ -notmatch '^\s*#' }) -join "`n"
if ($activeCargoConfig -match '(?m)^\s*target-dir\s*=') {
    Add-ArchitectureError ".cargo/config.toml must not force a repository-local Cargo target-dir; scripts set CARGO_TARGET_DIR when needed"
}
if ($cargoConfig -notlike "*CARGO_TARGET_DIR*") {
    Add-ArchitectureError ".cargo/config.toml must document that Clearra runners set CARGO_TARGET_DIR explicitly"
}
if ($cargoConfig -like "*MANIFESTINPUT*" -or $cargoConfig -like "*windows-as-invoker.manifest*") {
    Add-ArchitectureError ".cargo/config.toml must not hard-code Windows manifest paths; scripts/verify.ps1 injects an absolute linker flag"
}

$pathHelpers = Read-Text "scripts/lib/clearra-path-helpers.ps1"
foreach ($requiredCargoTargetPolicy in @(
    "function Get-ClearraCargoTargetDir",
    "function Assert-ClearraCanonicalCargoTargetDir",
    "function Remove-ClearraRepositoryLocalBuildArtifacts",
    "clearra-artifact-cache.ps1"
)) {
    if ($pathHelpers -notlike "*$requiredCargoTargetPolicy*") {
        Add-ArchitectureError "Cargo execution-surface policy is missing '$requiredCargoTargetPolicy'"
    }
}
$artifactCache = Read-Text "scripts/lib/clearra-artifact-cache.ps1"
foreach ($requiredCachePolicy in @(
    "function Initialize-ClearraBuildArtifactCache",
    "function Ensure-ClearraBuildArtifactCache",
    "function Enter-ClearraArtifactCacheUsageLock",
    "function Exit-ClearraBuildArtifactCacheUsage",
    "function Invoke-ClearraBuildArtifactCacheRetention",
    "function Test-ClearraInheritedArtifactCacheOwner",
    "CLEARRA_BUILD_CACHE_OWNER_PID",
    "CLEARRA_MAX_BUILD_CACHE_GIB",
    "CARGO_INCREMENTAL = '0'",
    ".clearra-cache-state.json",
    "input-change-reuse",
    "workspace-or-schema-reset",
    "budget-reset",
    "post-run-budget-reset"
)) {
    if ($artifactCache -notlike "*$requiredCachePolicy*") {
        Add-ArchitectureError "bounded artifact-cache policy is missing '$requiredCachePolicy'"
    }
}
if ($artifactCache -like '*input-change-reset*') {
    Add-ArchitectureError "ordinary source changes must preserve the incremental CMake/Cargo cache"
}
$clearraRunner = Read-Text "scripts/clearra.ps1"
foreach ($requiredRunnerCachePolicy in @(
    "Ensure-ClearraBuildArtifactCache",
    "previousCargoIncremental",
    "previousBuildCacheSessionKey"
)) {
    if ($clearraRunner -notlike "*$requiredRunnerCachePolicy*") {
        Add-ArchitectureError "Clearra runner does not preserve the artifact-cache contract '$requiredRunnerCachePolicy'"
    }
}
foreach ($assetImporter in @(
    "tools/asset-import/svg_sanitize.ps1",
    "tools/asset-import/svg_rasterize.ps1",
    "tools/asset-import/import_skin.ps1"
)) {
    $assetImporterText = Read-Text $assetImporter
    if ($assetImporterText -notlike "*Get-ClearraCargoTargetDir*") {
        Add-ArchitectureError "$assetImporter must use the canonical Cargo target directory"
    }
    if ($assetImporterText -like '*Resolve-ClearraArtifactPath "asset-import"*') {
        Add-ArchitectureError "$assetImporter must not create a duplicate asset-import Cargo target"
    }
}
$activeRunnerScripts = @(Get-ChildItem -LiteralPath (Join-Path $Root "scripts") `
        -Recurse -File -Filter "*.ps1" | Where-Object {
            $_.FullName -notlike "*\scripts\architecture\*" -and
            $_.FullName -ne (Join-Path $Root "scripts/lib/clearra-path-helpers.ps1")
        })
foreach ($runnerScript in $activeRunnerScripts) {
    $runnerText = Get-Content -LiteralPath $runnerScript.FullName -Raw
    foreach ($forbiddenCargoSurface in @(
        "Get-ClearraReleaseCargoTargetDir",
        "Get-DesktopCargoTargetDir",
        "Get-ProductE2ECargoTargetDir",
        "Get-WorkerE2ECargoTargetDir",
        "Get-ClearraUxCargoTargetDir",
        "cargo-target-native",
        "tauri-target"
    )) {
        if ($runnerText -like "*$forbiddenCargoSurface*") {
            Add-ArchitectureError "$($runnerScript.FullName) creates a non-canonical Cargo surface '$forbiddenCargoSurface'"
        }
    }
    if ($runnerText -like '*Resolve-ClearraArtifactPath "cargo-target"*' -or
        $runnerText -like '*Get-StartTestsPersistentBuildDir "cargo-target"*') {
        Add-ArchitectureError "$($runnerScript.FullName) must obtain CARGO_TARGET_DIR only through Get-ClearraCargoTargetDir"
    }
}

$gitignore = Read-Text ".gitignore"
if ($gitignore -notlike "*/.cargo/target*/*") {
    Add-ArchitectureError ".gitignore must ignore .cargo/target*/ test artifacts"
}

$cargoToml = Read-Text "Cargo.toml"
if ($cargoToml -like '*"xtask"*') {
    Add-ArchitectureError "Cargo workspace must not depend on Rust xtask for the standard test gate"
}
$workspaceDependencyGraph = Get-WorkspaceDependencyGraph

$invariantCargo = Read-Text "crates/clearra-invariant-tests/Cargo.toml"
if ($invariantCargo -notlike "*[lib]*" -or $invariantCargo -notlike "*test = false*") {
    Add-ArchitectureError "clearra-invariant-tests must disable its empty lib test harness; executable contracts live in integration tests"
}
$invariantBuildContracts = Read-Text "crates/clearra-invariant-tests/tests/build_coverage_contract_tests.rs"
$invariantAreaDecompositionContracts = Read-Text "crates/clearra-invariant-tests/tests/area_decomposition_contract_tests.rs"
$invariantBoardBackendContracts = Read-Text "crates/clearra-invariant-tests/tests/board_backend_contract_tests.rs"
$invariantCustomPieceContracts = Read-Text "crates/clearra-invariant-tests/tests/custom_piece_contract_tests.rs"
$invariantCustomRuleEditorContracts = Read-Text "crates/clearra-invariant-tests/tests/custom_rule_editor_contract_tests.rs"
$invariantExactCoverDlxContracts = Read-Text "crates/clearra-invariant-tests/tests/exact_cover_dlx_contract_tests.rs"
$invariantProfileContracts = Read-Text "crates/clearra-invariant-tests/tests/profile_contract_tests.rs"
$workspaceInvariantContracts = Read-Text "crates/clearra-invariant-tests/tests/workspace_invariant_tests.rs"
$scenarioFixtureContracts = Read-Text "crates/clearra-invariant-tests/tests/scenario_fixture_contract_tests.rs"
foreach ($requiredMarker in @("AssignmentCsp", "BuildCoverageMatrix", "BuildCoverageResult", "BuildCoverageLimits", "build_coverage_probability_uses_union_not_assignment_sum", "build_template_native_json_import_export_roundtrips_editor_contract", "build_template_native_json_import_rejects_raw_external_text")) {
    if ($invariantBuildContracts -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-invariant-tests must carry build coverage contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("board_backend_kind_selects_board64_board128_and_wide_by_area", "Board128State", "WideBoardState", "BoardStateBackend", "collision_place_clear_row_mask_and_occupied_count", "ECustomBoardUnsupportedMvp")) {
    if ($invariantBoardBackendContracts -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-invariant-tests must carry MVP3 board backend contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("area_decomposition_runs_through_non_board64_backend_families", "AreaDecomposer", "AreaScope", "ScenarioAreaPruner", "AreaTileabilityRules", "Board128State", "WideBoardState", "scenario_pruner_requires_an_explicit_area_scope", "tileability_uses_piece_area_rules_without_assuming_tetromino_only_runtime")) {
    if ($invariantAreaDecompositionContracts -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-invariant-tests must carry MVP3 area decomposition contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("dlx_solver_enumerates_exact_cover_solutions_with_completeness_contract", "dlx_solver_reports_truncation_without_claiming_complete_enumeration", "setup_tiling_bridge_uses_dlx_after_sparse_shape_column_remap", "custom_piece_bridge_uses_dlx_for_tiling_enumeration_without_pc_runtime_search", "build_slot_assignment_can_use_exact_cover_without_moving_csp_into_cli_or_search")) {
    if ($invariantExactCoverDlxContracts -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-invariant-tests must carry MVP3 exact-cover/DLX contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("standard_10_board_profile", "standard_7_bag_profile", "standard_tetromino_piece_set_profile", "SearchDefaults::MVP1", "scenario_retained_trace_limit", "profile_ids_expose_stable_canonical_strings", "multiplicity_for", "total_weight")) {
    if ($invariantProfileContracts -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-invariant-tests must carry profile contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("invariant_setup_union_probability_is_invariant_to_variant_order", "invariant_checkpoint_schedule_and_continuation_hint_are_label_contracts", "invariant_opening_and_scenario_are_chain_labels_not_solver_paths", "invariant_core_executor_uses_checkpoint_schedule_metadata_without_cache_fields", "invariant_observed_opening_uses_same_schedule_metadata_without_cache_counters", "invariant_scenario_service_keeps_full_queue_for_min_remaining_queue", "invariant_observed_supply_ambiguity_is_warning_not_error", "coverage_crate_does_not_depend_on_scoring_crate", "problem_crate_does_not_depend_on_scoring_implementation_crate")) {
    if ($workspaceInvariantContracts -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-invariant-tests must carry workspace invariant marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("scenario_fixtures_drive_clear_to_empty_search_and_count_contracts", "tests/fixtures/pc", "expected_total_solution_count", "requires_180", "scenario_requires_180_unsupported", "exact_pieces", "min_remaining_queue", "allow_hold", "count_policy", "retained_trace_limit", "ProblemCompiler::compile_scenario_pc", "CoreExecutor::execute")) {
    if ($scenarioFixtureContracts -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-invariant-tests must carry scenario fixture contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("mvp3_custom_piece_fixture_defines_stable_mixed_piece_set_but_runtime_is_guarded", "stable_piece_definition_ids_are_not_registry_order_indices", "tests/fixtures/pieces/mixed_custom_piece_set.json", "PieceDefinitionId", "MixedPieceSet", "MixedBagProfile", "validate_mixed_piece_set_mvp3_guard", "validate_mixed_bag_profile_mvp3_guard", "ECustomPieceUnsupportedMvp", "ECustomBagUnsupportedMvp")) {
    if ($invariantCustomPieceContracts -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-invariant-tests must carry MVP3 custom piece contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("custom_rule_editor_pipeline_requires_validation_before_verified_profile_and_capability", "raw_custom_rule_editor_draft_is_not_a_search_input_contract", "rule_editor_schema_exposes_full_custom_rule_sections_but_keeps_mvp3_guard", "CustomRuleEditorDraft", "VerifiedCustomRuleProfile", "CustomRuleSearchCapabilityReport", "validate_custom_rule_editor_draft", "search_backend_supported", "custom_rule_search_backend_not_connected")) {
    if ($invariantCustomRuleEditorContracts -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-invariant-tests must carry MVP3 custom rule editor contract marker '$requiredMarker'"
    }
}
$coveragePatternBitSet = Read-Text "crates/clearra-coverage/src/pattern/pattern_bitset.rs"
$coverageUnionProbability = Read-Text "crates/clearra-coverage/src/probability/union_probability.rs"
foreach ($requiredMarker in @("union_uses_or_semantics", "union_rejects_different_pattern_universes", "union_with_rejects_different_pattern_universes_without_mutating", "pub fn is_superset", "Result<bool, PatternBitSetError>", "is_superset_rejects_different_pattern_universes")) {
    if ($coveragePatternBitSet -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-coverage must own PatternBitSet invariant marker '$requiredMarker'"
    }
}
$coverageMatrix = Read-Text "crates/clearra-coverage/src/matrix/coverage_matrix.rs"
foreach ($requiredMarker in @("RowIndexOutOfRange", "pub fn union_rows", "Result<PatternBitSet, CoverageMatrixError>", "union_rows_rejects_out_of_range_row_index", "typed_coverage_matrix_rejects_zero_identity_when_universe_required")) {
    if ($coverageMatrix -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-coverage must reject invalid CoverageMatrix::union_rows row indices with marker '$requiredMarker'"
    }
}
if ($coverageUnionProbability -notlike "*overlapping_patterns_are_measured_once_after_union*") {
    Add-ArchitectureError "clearra-coverage must own union probability invariant marker 'overlapping_patterns_are_measured_once_after_union'"
}
$cliSrcContractFiles = @(Get-ChildItem -LiteralPath (Join-Path $Root "crates/clearra-cli/src") -Recurse -File -Filter "*contract_tests.rs" -ErrorAction SilentlyContinue)
foreach ($file in $cliSrcContractFiles) {
    Add-ArchitectureError "clearra-cli/src must not own cross-crate contract test file '$($file.Name)'; use clearra-invariant-tests or crate-local tests"
}
foreach ($file in Get-RustFiles "crates/clearra-cli/src") {
    $relativePath = Resolve-Path -LiteralPath $file.FullName -Relative
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    foreach ($contractMarker in @(
        "build_coverage_probability_uses_union_not_assignment_sum",
        "build_coverage_limits_come_from_profile_defaults",
        "profile_ids_expose_stable_canonical_strings",
        "mvp1_defaults_expose_runtime_budget_values",
        "invariant_setup_union_probability_is_invariant_to_variant_order",
        "invariant_checkpoint_schedule_and_continuation_hint_are_label_contracts"
    )) {
        if ($contents -like "*$contractMarker*") {
            Add-ArchitectureError "$relativePath must not carry build/profile/search contract test marker '$contractMarker'; use clearra-invariant-tests"
        }
    }
}

foreach ($file in Get-ProductionRustFiles) {
    $relativePath = Resolve-Path -LiteralPath $file.FullName -Relative
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    foreach ($fixtureName in @("tsar", "evil_cannon", "lorax")) {
        if ($contents -match "(?i)\b$([regex]::Escape($fixtureName))\b") {
            Add-ArchitectureError "$relativePath must not contain external fixture name '$fixtureName' in production code"
        }
    }
    if ($contents -match '(?i)"[^"\r\n]*(harddrop_fixture|harddrop_pc|hard drop)[^"\r\n]*"') {
        Add-ArchitectureError "$relativePath must not contain Hard Drop fixture names in production string/data literals"
    }
}
