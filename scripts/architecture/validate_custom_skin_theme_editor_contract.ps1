# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# G10 keeps user-provided skin/theme editing provenance-backed, cache/config
# scoped, and PNG-atlas-only at runtime.

function Invoke-CustomSkinThemeEditorContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-render/src/skin/custom_skin_theme_schema.rs",
            "crates/clearra-render/src/skin/custom_skin_theme_parts.rs",
            "crates/clearra-render/tests/custom_skin_theme_editor.rs",
            "crates/clearra-ui-schema/src/render/custom_skin_theme_editor_schema.rs",
            "packages/clearra-ui/src/lib/render/customSkinThemeEditor.ts",
            "tools/asset-import/custom_skin_theme_import_contract.json",
            "docs/architecture.md",
            "docs/future-custom-pieces.md",
            "docs/mvp-scope.md",
            "scripts/custom-skin-theme-editor-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "G10 required custom skin theme editor file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-render/src/skin/custom_skin_theme_schema.rs"
        Read-Text "crates/clearra-render/src/skin/custom_skin_theme_parts.rs"
        Read-Text "crates/clearra-render/src/skin/mod.rs"
        Read-Text "crates/clearra-render/src/lib.rs"
        Read-Text "crates/clearra-render/tests/custom_skin_theme_editor.rs"
        Read-Text "crates/clearra-ui-schema/src/render/custom_skin_theme_editor_schema.rs"
        Read-Text "crates/clearra-ui-schema/src/render/mod.rs"
        Read-Text "crates/clearra-ui-schema/src/lib.rs"
        Read-Text "packages/clearra-ui/src/lib/render/customSkinThemeEditor.ts"
        Read-Text "packages/clearra-ui/src/lib/render/index.ts"
        Read-Text "tools/asset-import/custom_skin_theme_import_contract.json"
        Read-Text "docs/architecture.md"
        Read-Text "docs/future-custom-pieces.md"
        Read-Text "docs/mvp-scope.md"
        Read-Text "scripts/custom-skin-theme-editor-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "CustomSkinThemeSchema",
            "CustomSkinThemeEditorSchema",
            "skin_id",
            "palette_id",
            "piece_mapping",
            "grid_style",
            "background",
            "line_clear_highlight",
            "ownership_color_mode",
            "export_limits",
            "provenance",
            "UserImportedSkinAssetLocation",
            "user_config_directory",
            "user_cache_directory",
            "not_repository_assets",
            "manifest_and_provenance_required",
            "custom_skin_schema_validates",
            "custom_skin_import_requires_provenance",
            "custom_theme_preview_uses_png_atlas",
            "raw_svg_not_passed_to_runtime_renderer",
            "runtime_raw_svg_allowed",
            "false",
            "png-atlas",
            "compile-rust-ui-schema-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G10 custom skin theme editor contract must expose marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "user_imported_asset_builtin_without_provenance",
            "raw_svg_preview_to_renderer=true",
            "external_asset_license_unknown_committed",
            "repository_assets_for_user_import=true",
            "rawSvgRuntimeRendererAllowed: true",
            "runtimePreviewSource: 'raw-svg'"
        )) {
        if ($surface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "G10 must not introduce forbidden custom skin shortcut marker '$forbiddenMarker'"
        }
    }
}
