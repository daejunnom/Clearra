# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-GuiEditorSchemaV2ContractValidation() {
foreach ($requiredPath in @(
            "crates/clearra-ui-schema/src/gui_editor_schema_v2/mod.rs",
            "crates/clearra-ui-schema/src/gui_editor_schema_v2/gui_editor_schema_v2.rs",
            "crates/clearra-ui-schema/src/gui_editor_schema_v2/gui_contract_field_schema.rs",
            "crates/clearra-ui-schema/src/gui_editor_schema_v2/render_options_schema.rs",
            "crates/clearra-ui-schema/src/gui_editor_schema_v2/diagnostic_panel_schema.rs",
            "scripts/gui-editor-schema-v2-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "X8 GUI / Editor Schema v2 required file is missing: $requiredPath"
        }
    }
$guiSchema = @(
        Read-Text "crates/clearra-ui-schema/src/gui_editor_schema_v2/gui_editor_schema_v2.rs"
        Read-Text "crates/clearra-ui-schema/src/gui_editor_schema_v2/gui_contract_field_schema.rs"
        Read-Text "crates/clearra-ui-schema/src/gui_editor_schema_v2/render_options_schema.rs"
        Read-Text "crates/clearra-ui-schema/src/gui_editor_schema_v2/diagnostic_panel_schema.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "GuiEditorSchemaV2",
            "BackendOptionsSchema",
            "ProblemPresetOptionsSchema",
            "ScenarioEditorSchema",
            "SetupExplorerSchema",
            "BuildEditorSchema",
            "RuleEditorSchema",
            "ScoreProfileEditorSchema",
            "RenderOptionsSchema",
            "DiagnosticPanelSchema",
            "backend_requested",
            "backend_selected",
            "backend_fallback_reason",
            "gpu_trust_state",
            "packing_candidate_count",
            "build_variant_count",
            "total_solution_count",
            "retained_trace_count",
            "coverage_probability",
            "raw_coverage_export_path",
            "score_basis",
            "score_accuracy_level",
            "unsupported_reason",
            "renderer_capability",
            "skin_manifest_valid",
            "atlas_provenance_valid",
            "json_contract_keys_localized: false",
            "gui_schema_exposes_backend_trust_state",
            "gui_schema_exposes_raw_setup_metrics",
            "gui_schema_exposes_score_accuracy_level",
            "gui_schema_exposes_exact_renderer_asset_status",
            "gui_schema_does_not_localize_json_keys"
        )) {
        if ($guiSchema -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X8 GUI editor schema v2 must expose marker '$requiredMarker'"
        }
    }
$uiSchemaLib = Read-Text "crates/clearra-ui-schema/src/lib.rs"
foreach ($requiredMarker in @(
            "pub mod gui_editor_schema_v2",
            "GuiEditorSchemaV2",
            "RenderOptionsSchema",
            "DiagnosticPanelSchema",
            "ScoreEditorSchema"
        )) {
        if ($uiSchemaLib -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-ui-schema public API must export X8 marker '$requiredMarker'"
        }
    }
$buildEditor = Read-Text "crates/clearra-ui-schema/src/build_editor/build_editor_schema.rs"
foreach ($requiredMarker in @(
            "packing_candidate_count",
            "build_variant_count",
            "total_solution_count",
            "retained_trace_count",
            "coverage_probability",
            "raw_coverage_export_path",
            "backend_fallback_reason"
        )) {
        if ($buildEditor -notlike "*$requiredMarker*") {
            Add-ArchitectureError "BuildEditorSchema must expose X8 result marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "X8 GUI / Editor Schema v2",
            "GUI label and JSON key separation",
            "backend auto/cpu/gpu/hybrid",
            "fallback reason",
            "gpu trust state",
            "raw setup metrics",
            "score accuracy level",
            "renderer capability",
            "skin manifest validity",
            "unsupported reason"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document X8 marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "localized_json_key",
            "hide_backend_fallback",
            "disabled_without_reason"
        )) {
        if ($guiSchema -like "*$forbiddenMarker*") {
            Add-ArchitectureError "X8 GUI schema must not contain forbidden marker '$forbiddenMarker'"
        }
    }
}
