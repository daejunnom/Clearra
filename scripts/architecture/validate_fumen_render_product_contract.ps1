# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-FumenRenderProductContractValidation() {
foreach ($requiredPath in @(
            "crates/clearra-fumen/src/transform/fumen_transform_contract.rs",
            "crates/clearra-fumen/src/transform/page_transforms.rs",
            "crates/clearra-fumen/src/adapter/replay_to_fumen.rs",
            "crates/clearra-fumen/src/adapter/fumen_to_replay.rs",
            "crates/clearra-render/src/bitmap/bitmap_renderer.rs",
            "crates/clearra-render/src/bitmap/png_encoder.rs",
            "crates/clearra-render/src/bitmap/gif_encoder.rs",
            "crates/clearra-render/src/scene.rs",
            "crates/clearra-render/src/skin/skin_atlas.rs",
            "crates/clearra-render/src/export/render_export_limits.rs",
            "crates/clearra-output/src/render.rs",
            "scripts/fumen-render-product-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "X7 Fumen Transform / PNG / GIF Renderer required file is missing: $requiredPath"
        }
    }
$fumenTransform = @(
        Read-Text "crates/clearra-fumen/src/transform/fumen_transform_contract.rs"
        Read-Text "crates/clearra-fumen/src/transform/page_transforms.rs"
        Read-Text "crates/clearra-fumen/src/adapter/replay_to_fumen.rs"
        Read-Text "crates/clearra-fumen/src/adapter/fumen_to_replay.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "FumenTransformContract",
            "page_roundtrip",
            "combine",
            "split",
            "mirror",
            "field_mirror",
            "grayout",
            "remove_comments",
            "preserve_comments",
            "page_shift",
            "ReplayToFumenAdapter",
            "FumenToReplayAdapter",
            "FumenToBuildTemplateAdapter",
            "BuildTemplateDraft",
            "fumen_page_roundtrip",
            "fumen_mirror_roundtrip",
            "fumen_to_build_template_adapter_validates_input"
        )) {
        if ($fumenTransform -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X7 fumen transform contract must expose marker '$requiredMarker'"
        }
    }
$renderSurface = @(
        Read-Text "crates/clearra-render/src/bitmap/bitmap_renderer.rs"
        Read-Text "crates/clearra-render/src/bitmap/render_board.rs"
        Read-Text "crates/clearra-render/src/bitmap/png_encoder.rs"
        Read-Text "crates/clearra-render/src/bitmap/gif_encoder.rs"
        Read-Text "crates/clearra-render/src/export/render_export_limits.rs"
        Read-Text "crates/clearra-render/src/capability/render_capability.rs"
        Read-Text "crates/clearra-render/src/scene.rs"
        Read-Text "crates/clearra-render/src/skin/skin_atlas.rs"
        Read-Text "crates/clearra-render/src/bitmap/bitmap_renderer_tests.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "ExactBitmapRenderer",
            "RenderBoard",
            "render_board_png",
            "render_minos_crop_png",
            "render_lock_frame_png",
            "render_after_clear_png",
            "render_timeline_gif",
            "render_replay_png",
            "render_replay_timeline_gif",
            "RenderScene::from_replay_trace",
            "SkinAtlas::from_manifest_and_png",
            "RenderExportLimits",
            "RenderCapabilityReport::current",
            "png_board_render_golden",
            "png_lock_frame_render_golden",
            "gif_timeline_render_golden",
            "renderer_reports_export_limits"
        )) {
        if ($renderSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X7 renderer contract must expose marker '$requiredMarker'"
        }
    }
$outputSurface = Read-Text "crates/clearra-output/src/render.rs"
foreach ($requiredMarker in @(
            "BitmapExportLimitReport",
            "bitmap_export_limits",
            "renderer_reports_export_limits",
            "render_replay_trace",
            "ExactBitmapOutput"
        )) {
        if ($outputSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X7 output render surface must expose marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "X7 Fumen Transform / PNG / GIF Renderer",
            "fumen parser stays out of search core",
            "ReplayTrace -> FumenLike output",
            "fumen-to-build-template validates input",
            "PNG board render golden",
            "PNG lock-frame render golden",
            "GIF timeline render golden",
            "renderer reports export limits",
            "runtime raw SVG rendering remains forbidden"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document X7 marker '$requiredMarker'"
        }
    }
$renderSource = Read-Text "crates/clearra-render/src/bitmap/bitmap_renderer.rs"
foreach ($forbiddenMarker in @(
            "BuildVariant",
            "PackingCandidate",
            "CoverageRow",
            "raw_svg",
            ".svg"
        )) {
        if ($renderSource -like "*$forbiddenMarker*") {
            Add-ArchitectureError "X7 bitmap renderer must not mutate solver/build data or consume raw SVG marker '$forbiddenMarker'"
        }
    }
$searchCoreSurface = @(
        Read-Text "crates/clearra-core-executor/src/lib.rs"
        Read-Text "crates/clearra-problem/src/lib.rs"
    ) -join "`n"
foreach ($forbiddenMarker in @("FumenLikeReader", "FumenNormalizer", "FumenToBuildTemplateAdapter")) {
        if ($searchCoreSurface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "X7 fumen parser stays out of search core; found '$forbiddenMarker'"
        }
    }
$coreCFiles = @(Get-ChildItem -LiteralPath "core-c" -Recurse -File -Include "*.c", "*.h" -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -notmatch "[\\/](build|coverage|dist|node_modules)[\\/]" })
foreach ($file in $coreCFiles) {
        $text = Get-Content -LiteralPath $file.FullName -Raw
        foreach ($forbiddenMarker in @("Fumen", "fumen")) {
            if ($text -like "*$forbiddenMarker*") {
                Add-ArchitectureError "core_c_has_no_fumen_dependency failed: core-c must not know fumen adapters; found '$forbiddenMarker' in $($file.FullName)"
            }
        }
    }
}
