# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-RenderExactnessGateContractValidation() {
    foreach ($requiredFile in @(
            "crates/clearra-render/src/scene.rs",
            "crates/clearra-render/src/bitmap/bitmap_renderer.rs",
            "crates/clearra-render/src/skin/skin_atlas.rs",
            "crates/clearra-render/src/asset_import/asset_import_pipeline.rs",
            "crates/clearra-render/src/bin/clearra_asset_import.rs",
            "crates/clearra-output/src/render.rs",
            "assets/skins/default/skin.json",
            "assets/skins/default/provenance.json",
            "assets/skins/default/import-report.json",
            "assets/skins/default/atlas.png",
            "tests/golden/render/render_capability_exact.json",
            "tests/golden/render/render_exact_output_connected.json",
            "tools/asset-import/import_skin.ps1",
            "tools/asset-import/svg_sanitize.ps1",
            "tools/asset-import/svg_rasterize.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "connected renderer required file missing: $requiredFile"
        }
    }

    $renderApi = @(
        Read-Text "crates/clearra-render/src/scene.rs"
        Read-Text "crates/clearra-render/src/bitmap/bitmap_renderer.rs"
        Read-Text "crates/clearra-render/src/skin/skin_atlas.rs"
        Read-Text "crates/clearra-render/src/capability/render_capability.rs"
        Read-Text "crates/clearra-output/src/render.rs"
    ) -join "`n"
    foreach ($publicContract in @(
            "RenderScene::from_replay_trace",
            "SkinAtlas::from_manifest_and_png",
            "render_replay_png",
            "render_replay_timeline_gif",
            "RenderCapabilityReport::current",
            "render_replay_trace"
        )) {
        if ($renderApi -notlike "*$publicContract*") {
            Add-ArchitectureError "connected renderer public contract missing '$publicContract'"
        }
    }
    foreach ($forbiddenProductMarker in @(
            "renderer_not_connected",
            "current_unsupported",
            "unsupported_exact_bitmap_output",
            "contract placeholder",
            "placeholder_preview"
        )) {
        if ($renderApi -like "*$forbiddenProductMarker*") {
            Add-ArchitectureError "connected renderer product surface contains forbidden marker '$forbiddenProductMarker'"
        }
    }

    $manifestPath = Join-Path $Root "assets/skins/default/skin.json"
    $provenancePath = Join-Path $Root "assets/skins/default/provenance.json"
    $reportPath = Join-Path $Root "assets/skins/default/import-report.json"
    $atlasPath = Join-Path $Root "assets/skins/default/atlas.png"
    try {
        $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
        $provenance = Get-Content -LiteralPath $provenancePath -Raw | ConvertFrom-Json
        $report = Get-Content -LiteralPath $reportPath -Raw | ConvertFrom-Json
        if (-not $manifest.capability.supported -or -not $manifest.capability.render_exact) {
            Add-ArchitectureError "default skin must be ConnectedExact"
        }
        if ($null -ne $manifest.capability.unsupported_reason) {
            Add-ArchitectureError "connected default skin must not carry unsupported_reason"
        }
        if ($manifest.runtime_raw_svg_allowed -ne $false) {
            Add-ArchitectureError "runtime_raw_svg_allowed must remain false"
        }
        if ($manifest.tile_width -le 1 -or $manifest.tile_height -le 1) {
            Add-ArchitectureError "1x1 placeholder atlas tiles are forbidden for the product skin"
        }

        $atlasHash = (Get-FileHash -LiteralPath $atlasPath -Algorithm SHA256).Hash.ToLowerInvariant()
        $manifestHash = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($provenance.atlas_png_sha256 -ne $atlasHash -or $report.atlas_png_sha256 -ne $atlasHash) {
            Add-ArchitectureError "default atlas provenance hash does not match atlas.png"
        }
        if ($provenance.manifest_sha256 -ne $manifestHash -or $report.manifest_sha256 -ne $manifestHash) {
            Add-ArchitectureError "default manifest provenance hash does not match skin.json"
        }
    } catch {
        Add-ArchitectureError "default skin JSON/provenance validation failed: $($_.Exception.Message)"
    }

    $atlasBytes = [System.IO.File]::ReadAllBytes($atlasPath)
    if ($atlasBytes.Length -lt 24) {
        Add-ArchitectureError "default atlas PNG is truncated"
    } else {
        $atlasWidth = ($atlasBytes[16] -shl 24) -bor ($atlasBytes[17] -shl 16) -bor ($atlasBytes[18] -shl 8) -bor $atlasBytes[19]
        $atlasHeight = ($atlasBytes[20] -shl 24) -bor ($atlasBytes[21] -shl 16) -bor ($atlasBytes[22] -shl 8) -bor $atlasBytes[23]
        if ($atlasWidth -le 1 -or $atlasHeight -le 1) {
            Add-ArchitectureError "default product atlas must not be a 1x1 placeholder"
        }
    }

    $rawSvgAssets = @(Get-ChildItem -LiteralPath (Join-Path $Root "assets") -Recurse -File -Filter "*.svg" -ErrorAction SilentlyContinue)
    foreach ($asset in $rawSvgAssets) {
        Add-ArchitectureError "runtime raw SVG asset is forbidden: $($asset.FullName)"
    }

    $rasterizer = Read-Text "tools/asset-import/svg_rasterize.ps1"
    if ($rasterizer -like "*always throw*" -or $rasterizer -like "*contract placeholder*") {
        Add-ArchitectureError "SVG rasterizer command must execute the reviewed importer"
    }
    $renderCargo = Read-Text "crates/clearra-render/Cargo.toml"
    foreach ($marker in @('default = []', 'asset-import = ["dep:resvg"]', 'optional = true')) {
        if ($renderCargo -notlike "*$marker*") {
            Add-ArchitectureError "resvg must remain build-time feature gated: missing '$marker'"
        }
    }
}
