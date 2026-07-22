# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-AssetImportSecurityContractValidation() {
    foreach ($requiredFile in @(
            "crates/clearra-render/src/asset_import/asset_import_limits.rs",
            "crates/clearra-render/src/asset_import/asset_import_report.rs",
            "crates/clearra-render/src/asset_import/svg_security_scanner.rs",
            "crates/clearra-render/src/asset_import/svg_sanitizer.rs",
            "crates/clearra-render/src/asset_import/asset_import_pipeline.rs",
            "crates/clearra-render/src/asset_import/runtime_asset_gate.rs",
            "crates/clearra-render/tests/asset_import_security.rs",
            "tools/asset-import/import_skin.ps1",
            "tools/asset-import/svg_sanitize.ps1",
            "tools/asset-import/svg_rasterize.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "asset import security file missing: $requiredFile"
        }
    }

    $surface = @(
        Read-Text "crates/clearra-render/src/asset_import/asset_import_limits.rs"
        Read-Text "crates/clearra-render/src/asset_import/svg_security_scanner.rs"
        Read-Text "crates/clearra-render/src/asset_import/svg_sanitizer.rs"
        Read-Text "crates/clearra-render/src/asset_import/asset_import_pipeline.rs"
        Read-Text "crates/clearra-render/src/asset_import/runtime_asset_gate.rs"
        Read-Text "crates/clearra-render/src/bin/clearra_asset_import.rs"
    ) -join "`n"
    foreach ($publicAbiMarker in @(
            "max_svg_bytes", "max_decompressed_bytes", "max_elements", "max_group_depth",
            "max_path_commands", "max_path_segments_per_path", "max_gradients",
            "max_filters", "max_external_references", "max_css_rules",
            "max_viewbox_width", "max_viewbox_height", "max_raster_pixels",
            "max_import_time_ms", "max_memory_mib", "AssetImportPipeline",
            "sanitize_svg", "rasterize_sanitized_svg", "verify_hashes"
        )) {
        if ($surface -notlike "*$publicAbiMarker*") {
            Add-ArchitectureError "asset import public/security contract missing '$publicAbiMarker'"
        }
    }
    foreach ($securityMarker in @(
            "forbidden_svg_script", "svg_external_resource_forbidden",
            "svg_size_limit_exceeded", "svg_path_complexity_limit_exceeded",
            "forbidden_svg_filter", "compressed_svg_input_forbidden",
            "svg_memory_limit_exceeded", "svg_import_time_limit_exceeded"
        )) {
        if ($surface -notlike "*$securityMarker*") {
            Add-ArchitectureError "asset import rejection code missing '$securityMarker'"
        }
    }
    foreach ($hardLimitMarker in @(
        'Command::new', 'try_wait', 'child.kill', 'tempdir_in',
        'asset_import_atomic_commit_failed', 'svg_import_time_limit_exceeded'
    )) {
        if ($surface -notlike "*$hardLimitMarker*") {
            Add-ArchitectureError "asset import hard resource boundary missing '$hardLimitMarker'"
        }
    }

    $toolSurface = @(
        Read-Text "tools/asset-import/import_skin.ps1"
        Read-Text "tools/asset-import/svg_sanitize.ps1"
        Read-Text "tools/asset-import/svg_rasterize.ps1"
    ) -join "`n"
    foreach ($forbiddenMarker in @("renderer_not_connected", "placeholder", "limit disable", "raw SVG runtime")) {
        if ($toolSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "asset import tool contains forbidden product marker '$forbiddenMarker'"
        }
    }
    if ($toolSurface -notlike "*clearra-asset-import*") {
        Add-ArchitectureError "asset import scripts must use the reviewed Rust importer"
    }

    $runtimeAssetFiles = @(Get-ChildItem -LiteralPath (Join-Path $Root "assets") -Recurse -File -ErrorAction SilentlyContinue)
    foreach ($asset in $runtimeAssetFiles) {
        if ($asset.Extension -eq ".svg") {
            Add-ArchitectureError "runtime assets must consume PNG atlas only: $($asset.FullName)"
        }
    }
}
