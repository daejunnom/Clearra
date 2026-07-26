# This file is dot-sourced by Invoke-WorkspaceSurfaceArchitectureValidation.

$uiSchemaCargo = Read-Text "crates/clearra-ui-schema/Cargo.toml"
$uiSchemaLib = Read-Text "crates/clearra-ui-schema/src/lib.rs"
$uiSchemaSnapshot = Read-Text "crates/clearra-ui-schema/src/schema_snapshot.rs"
foreach ($crateName in @("clearra-build-coverage", "clearra-core-domain", "clearra-profiles", "clearra-rules", "clearra-scoring", "clearra-setup-search", "clearra-validation")) {
    if (-not (Test-DependencyLine $uiSchemaCargo $crateName)) {
        Add-ArchitectureError "clearra-ui-schema must depend on $crateName instead of duplicating canonical ids or diagnostic codes"
    }
}
$uiForbiddenCanonicalIdLiterals = @(
    '"standard-10"',
    '"standard-tetrominoes"',
    '"standard-7-bag"',
    '"srs"',
    '"srs-90"',
    '"srs-plus"',
    '"srs-x"',
    '"jstris-180"',
    '"asc"',
    '"ars"',
    '"no-kick"',
    '"custom"',
    '"jstris-ultra"',
    '"tetrio"',
    '"ppt"'
)
foreach ($file in Get-RustFiles "crates/clearra-ui-schema/src") {
    $contents = Get-Content -LiteralPath $file.FullName -Raw
    foreach ($literal in $uiForbiddenCanonicalIdLiterals) {
        if ($contents.Contains($literal)) {
            Add-ArchitectureError "$($file.FullName) must not hard-code canonical profile/rule id literal $literal"
        }
    }
    if ($contents.Contains(".disabled()")) {
        Add-ArchitectureError "$($file.FullName) must attach a diagnostic-backed disabled reason instead of plain disabled()"
    }
}
$dropdownOption = Read-Text "crates/clearra-ui-schema/src/dropdown/dropdown_option.rs"
if ($dropdownOption -like "*disabled: bool*") {
    Add-ArchitectureError "DropdownOption must expose diagnostic-backed disabled_reason instead of a bare disabled bool"
}
foreach ($requiredMarker in @("pub mod schema_snapshot", "UiSchemaSnapshot", "UI_SCHEMA_SNAPSHOT_VERSION")) {
    if ($uiSchemaLib -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-ui-schema lib surface must export schema snapshot marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "UI_SCHEMA_SNAPSHOT_VERSION",
    "UiSchemaSnapshot",
    "RuleEditorSchema::mvp2",
    "ScoreProfileEditorSchema::mvp2",
    "SetupExplorerSchema::mvp2",
    "BuildEditorSchema::mvp_template_slots",
    "ui_schema_snapshot_pins_mvp2_surface_counts"
)) {
    if ($uiSchemaSnapshot -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-ui-schema must expose MVP2 schema snapshot marker '$requiredMarker'"
    }
}
$kickEditor = Read-Text "crates/clearra-ui-schema/src/rule_editor/kick_table_editor_schema.rs"
foreach ($requiredMarker in @("KickProfileRegistry::builtin_profiles", "KickTablePreviewSchema", "KickTableImportExportSchema", "KickTableVerificationSchema", "KickImport", "KickProfileVerificationReport", "supports_exact_180", "c_compact_descriptor_ready", "unsupported_backend_reason", "unsupported_kick_profiles_carry_diagnostic_disabled_reason", "kick_table_editor_schema_exposes_registry_preview_and_import_export")) {
    if ($kickEditor -notlike "*$requiredMarker*") {
        Add-ArchitectureError "KickTableEditorSchema must expose MVP2 kick preview/import/export/verification marker '$requiredMarker'"
    }
}
$ruleEditor = Read-Text "crates/clearra-ui-schema/src/rule_editor/rule_editor_schema.rs"
$customRuleEditorSchema = Read-Text "crates/clearra-ui-schema/src/rule_editor/custom_rule_editor_schema.rs"
foreach ($requiredMarker in @("KickProfileRegistry::builtin_profiles", "custom_rule().id().as_str()", "CustomRuleEditorSchema::mvp3_guarded", "custom_rule_editor", "disabled_rule_editor_features_expose_diagnostic_codes_for_unsupported_profiles", "rule_presets_use_canonical_rule_ids")) {
    if ($ruleEditor -notlike "*$requiredMarker*") {
        Add-ArchitectureError "RuleEditorSchema must use canonical rule/kick registry source marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("CustomRuleEditorSchema", "CustomRuleEditorSectionSchema", "raw_editor_schema_type", "validation_adapter", "verified_profile_type", "search_capability_report_type", "search_input_allowed", "CustomRuleEditorDraft", "CustomRuleValidator::validate_editor_draft", "VerifiedCustomRuleProfile", "CustomRuleSearchCapabilityReport", "kick-table", "spawn-profile", "rotation-system", "lock-reachability", "line-clear-policy", "custom_rule_editor_schema_exposes_raw_validate_verify_capability_pipeline")) {
    if ($customRuleEditorSchema -notlike "*$requiredMarker*") {
        Add-ArchitectureError "CustomRuleEditorSchema must expose MVP3 raw->validation->verified->capability editor pipeline marker '$requiredMarker'"
    }
}
$scoreEditor = @(
    Read-Text "crates/clearra-ui-schema/src/score_editor/score_profile_editor_schema.rs"
    Read-Text "crates/clearra-ui-schema/src/score_editor/score_profile_editor_fields.rs"
    Read-Text "crates/clearra-ui-schema/src/score_editor/score_profile_import_export_schema.rs"
    Read-Text "crates/clearra-ui-schema/src/score_editor/score_profile_result_contract_fields.rs"
    Read-Text "crates/clearra-ui-schema/src/score_editor/score_profile_editor_schema_tests.rs"
) -join "`n"
foreach ($requiredMarker in @("ScoreProfileRegistry::builtins", "ScoreProfileImportExportSchema", "ScoreProfileImport", "ScoreProfileExport", "accuracy_level", "profile_specific_exact", "accuracy_reason", "score_fields", "attack_fields", "spin_fields", "t-spins", "combo_fields", "b2b_fields", "score_profile_editor_uses_canonical_registry_profiles", "score_profile_editor_exposes_profile_attack_spin_combo_b2b_fields")) {
    if ($scoreEditor -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ScoreProfileEditorSchema must expose MVP2 scoring profile/editor marker '$requiredMarker'"
    }
}
$setupExplorerSchema = @(
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_explorer_schema.rs"
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_result_column_schema.rs"
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_result_columns.rs"
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_probability_columns.rs"
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_backend_columns.rs"
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_score_columns.rs"
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_diagnostic_columns.rs"
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_continuation_columns.rs"
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/scenario_result_columns.rs"
    Read-Text "crates/clearra-ui-schema/src/setup_explorer/setup_explorer_schema_tests.rs"
) -join "`n"
foreach ($requiredMarker in @("SetupResultColumnSchema", "SetupResultColumnSource", "SetupResultColumnType", "total_solution_count", "count_complete", "solution_trace_mode", "backend_selection_reason", "state_count", "multiplicity_count", "score_expectation", "attack_expectation", "score_evaluation_trace_count", "score_evaluation_complete", "score_evaluation_basis", "build_variant_metrics_required_hold", "diagnostic_evidence_rule_profile", "continuation_available", "continuation_available_complete", "setup_explorer_schema_exposes_mvp2_result_columns")) {
    if ($setupExplorerSchema -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SetupExplorerSchema must expose MVP2 setup explorer result column marker '$requiredMarker'"
    }
}
$buildEditorSchema = Read-Text "crates/clearra-ui-schema/src/build_editor/build_editor_schema.rs"
$buildPreviewSchema = Read-Text "crates/clearra-ui-schema/src/build_editor/build_preview_board_schema.rs"
$buildValidationSchema = Read-Text "crates/clearra-ui-schema/src/build_editor/build_validation_schema.rs"
$buildCoverageSummarySchema = Read-Text "crates/clearra-ui-schema/src/build_editor/build_coverage_summary_schema.rs"
foreach ($requiredMarker in @("BuildPreviewBoardSchema", "BuildValidationDiagnosticSchema", "BuildCoverageSummarySchema", "with_validation_report", "with_coverage_summary", "build_editor_schema_accepts_validation_diagnostics_and_coverage_summary")) {
    if ($buildEditorSchema -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildEditorSchema must expose MVP2 preview/diagnostic/coverage marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("BuildTemplate", "occupied_cells", "from_template")) {
    if ($buildPreviewSchema -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildPreviewBoardSchema must derive preview board from BuildTemplate marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("DiagnosticReport", "DiagnosticSeverity", "diagnostic.code().as_str()", "from_report")) {
    if ($buildValidationSchema -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildValidationDiagnosticSchema must expose validation diagnostic display contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("BuildCoverageResult", "covered_pattern_count", "probability", "from_result")) {
    if ($buildCoverageSummarySchema -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildCoverageSummarySchema must expose build coverage summary marker '$requiredMarker'"
    }
}

$desktopRequiredFiles = @(
    "apps/clearra-desktop/package.json",
    "apps/clearra-desktop/src-tauri/Cargo.toml",
    "apps/clearra-desktop/src-tauri/src/main.rs",
    "apps/clearra-desktop/src-tauri/src/commands.rs",
    "apps/clearra-desktop/src/routes/+page.svelte",
    "packages/clearra-ui/src/lib/components/DesktopHostShell.svelte",
    "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts",
    "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs",
    "scripts/desktop-host-check.ps1",
    "scripts/desktop-ui-compile-check.mjs",
    "scripts/lib/clearra-application-control.ps1"
)
foreach ($relativePath in $desktopRequiredFiles) {
    if (-not (Test-Path -LiteralPath (Join-Path $Root $relativePath))) {
        Add-ArchitectureError "Tauri desktop product file is missing: $relativePath"
    }
}

foreach ($removedSurface in @(
    "gui/clearra-gui",
    "scripts/gui-smoke.ps1",
    "scripts/gui-host-smoke.ps1",
    "scripts/gui-host-e2e.ps1",
    "crates/clearra-gui-host/src/main.rs"
)) {
    if (Test-Path -LiteralPath (Join-Path $Root $removedSurface)) {
        Add-ArchitectureError "removed desktop product surface still exists: $removedSurface"
    }
}

$rootCmake = Read-PhysicalText "CMakeLists.txt"
$guiHostCargo = Read-PhysicalText "crates/clearra-gui-host/Cargo.toml"
$tauriCommands = Read-PhysicalText "apps/clearra-desktop/src-tauri/src/commands.rs"
$desktopBridge = Read-PhysicalText "crates/clearra-gui-host/src/desktop_host/desktop_request_bridge.rs"
$desktopClient = Read-PhysicalText "packages/clearra-ui/src/lib/host/clearraDesktopHost.ts"

foreach ($forbiddenMarker in @(
    "CLEARRA_BUILD_GUI",
    "add_subdirectory(gui/clearra-gui)",
    "clearra_gui",
    "Clearra GUI shell scaffold"
)) {
    if ($rootCmake -like "*$forbiddenMarker*") {
        Add-ArchitectureError "root CMake retains removed GUI marker '$forbiddenMarker'"
    }
}
if ($guiHostCargo -like "*[[bin]]*") {
    Add-ArchitectureError "clearra-gui-host must be a library-only Tauri host"
}

foreach ($requiredMarker in @(
    "DesktopTauriCommandBridge",
    "run_request",
    "validate_request",
    "start_job",
    "cancel_job",
    "get_job_events"
)) {
    if ($tauriCommands -notlike "*$requiredMarker*") {
        Add-ArchitectureError "Tauri command route is missing '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "self.app_context.run(request)",
    "response.to_host_response()",
    "serde_json::to_string",
    "GuiToAppRequest::build"
)) {
    if ($desktopBridge -notlike "*$requiredMarker*") {
        Add-ArchitectureError "desktop bridge does not execute the typed AppRequest path '$requiredMarker'"
    }
}
foreach ($requiredMarker in @("invoke<string>('run_request'", "JSON.parse(response)")) {
    if ($desktopClient -notlike "*$requiredMarker*") {
        Add-ArchitectureError "desktop client is missing real AppResponse route '$requiredMarker'"
    }
}

$desktopProductSurface = $tauriCommands + "`n" + $desktopBridge + "`n" + $desktopClient
foreach ($forbiddenMarker in @(
    "std::process::Command",
    "clearra.exe",
    "CliParser",
    "run_with_args",
    "clearra_packing_",
    "clr_buildup_",
    "final_response_matches_app_response_contract",
    "tauri_command_calls_clearra_gui_host_only",
    "desktop_form_builds_app_request: true"
)) {
    if ($desktopProductSurface -like "*$forbiddenMarker*") {
        Add-ArchitectureError "desktop product surface contains forbidden marker '$forbiddenMarker'"
    }
}
