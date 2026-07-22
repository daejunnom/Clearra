function Invoke-ReplayOutputBridgeValidation() {
    
foreach ($requiredPath in @(
        "crates/clearra-replay/src/replay/mod.rs",
        "crates/clearra-replay/src/replay/replay_engine.rs",
        "crates/clearra-replay/src/replay/replay_engine_tests.rs",
        "crates/clearra-replay/src/replay/replay_event.rs",
        "crates/clearra-replay/src/ownership/mod.rs",
        "crates/clearra-replay/src/ownership/colored_cell_owner.rs",
        "crates/clearra-replay/src/trace/solution_trace_builder.rs",
        "crates/clearra-fumen/src/codec/fumen_like_writer.rs",
        "crates/clearra-fumen/src/codec/fumen_like_reader.rs",
        "crates/clearra-fumen/src/codec/fumen_like_trace.rs",
        "crates/clearra-fumen/src/adapter/replay_to_fumen.rs",
        "crates/clearra-fumen/src/adapter/fumen_to_replay.rs",
        "crates/clearra-render/src/skin/skin_manifest.rs",
        "crates/clearra-render/src/skin/skin_atlas.rs",
        "crates/clearra-render/src/skin/skin_provenance.rs",
        "crates/clearra-render/src/skin/atlas_bounds_validator.rs",
        "crates/clearra-render/src/skin/skin_manifest_validator.rs",
        "crates/clearra-render/src/skin/skin_provenance_validator.rs",
        "crates/clearra-render/src/capability/render_capability.rs",
        "crates/clearra-render/src/options/render_options.rs",
        "assets/skins/default/skin.json",
        "assets/skins/default/provenance.json",
        "assets/skins/default/atlas.png",
        "tests/fixtures/render/skin_manifest_invalid_missing_piece.json",
        "tests/golden/render/render_capability_exact.json",
        "crates/clearra-render/tests/skin_asset_contract.rs",
        "crates/clearra-output/src/json/json_contract.rs",
        "crates/clearra-output/src/json/backend_gpu_worker_contract.rs",
        "crates/clearra-output/src/json/json_writer.rs",
        "crates/clearra-output/src/fumen_like/mod.rs",
        "crates/clearra-output/src/text/backend_summary_text.rs",
        "crates/clearra-output/src/text/text_writer.rs",
        "crates/clearra-output/src/render.rs"
    )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M16 Replay and Output Bridge required file is missing: $requiredPath"
        }
    }
$replayLib = Read-Text "crates/clearra-replay/src/lib.rs"
foreach ($requiredMarker in @("pub mod replay", "pub mod ownership", "ReplayTrace", "ReplayEngine", "ColoredCellOwnership", "SolutionTraceBuilder")) {
        if ($replayLib -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-replay lib.rs must export M16 replay marker '$requiredMarker'"
        }
    }
$replayEngine = (Read-Text "crates/clearra-replay/src/replay/replay_engine.rs") + "`n" + (Read-Text "crates/clearra-replay/src/replay/replay_engine_tests.rs")
foreach ($requiredMarker in @("BuildVariantReplayInput", "BuildVariantOperation", "ReplayEngine::build_variant_to_trace", "SolutionTraceBuilder", "ColoredCellOwnership::from_trace", "ReplayTraceMarker", "ReplayLineClearEvent", "build_variant_becomes_representative_sample_replay_trace")) {
        if ($replayEngine -notlike "*$requiredMarker*") {
            Add-ArchitectureError "Replay engine surface must implement M16 marker '$requiredMarker'"
        }
    }
$solutionTraceBuilder = Read-Text "crates/clearra-replay/src/trace/solution_trace_builder.rs"
foreach ($requiredMarker in @("representative_order", "LineClearEvent::new", "BoardAfterStep::new", "builder_preserves_line_clear_events")) {
        if ($solutionTraceBuilder -notlike "*$requiredMarker*") {
            Add-ArchitectureError "solution_trace_builder.rs must preserve M16 replay marker '$requiredMarker'"
        }
    }
$coloredOwnership = Read-Text "crates/clearra-replay/src/ownership/colored_cell_owner.rs"
foreach ($requiredMarker in @("ColoredCellOwnership", "ColoredCellOwner", "compact_owners_after_line_clear", "ownership_compacts_with_line_clears")) {
        if ($coloredOwnership -notlike "*$requiredMarker*") {
            Add-ArchitectureError "colored_cell_owner.rs must preserve M16 ownership marker '$requiredMarker'"
        }
    }
$jsonContract = Get-JsonContractValidationSurface
$backendGpuWorkerContract = Read-Text "crates/clearra-output/src/json/backend_gpu_worker_contract.rs"
foreach ($requiredMarker in @(
        "backend_gpu_worker_contract",
        "gpu_worker_state",
        "gpu_trust_state",
        "gpu_memory_ticket_id",
        "gpu_fence_epoch",
        "cpu_confirm_required",
        "gpu_can_source_exact_probability",
        "gpu_worker_fallback_reason",
        "gpu_backpressure_gpu_queue_depth",
        "gpu_backpressure_throttle_reason",
        "json_backend_report_includes_gpu_worker_trust_state",
        "json_gpu_worker_report_shows_connected_confirmed_state",
        "json_backend_report_includes_memory_ticket_and_fence_epoch",
        "json_gpu_worker_report_shows_memory_ticket_and_fence"
    )) {
        if ($backendGpuWorkerContract -notlike "*$requiredMarker*") {
            Add-ArchitectureError "backend_gpu_worker_contract.rs must expose Phase 8 marker '$requiredMarker'"
        }
    }
$backendSummaryText = Read-Text "crates/clearra-output/src/text/backend_summary_text.rs"
foreach ($requiredMarker in @(
        "BackendSummaryText",
        "default_lines",
        "verbose_lines",
        "gpu: unavailable",
        "memory: clean",
        "gpu_worker_state",
        "gpu_memory_ticket_id",
        "gpu_backpressure_gpu_queue_depth",
        "text_default_summarizes_gpu_worker_without_internal_noise",
        "text_verbose_includes_gpu_worker_backpressure"
    )) {
        if ($backendSummaryText -notlike "*$requiredMarker*") {
            Add-ArchitectureError "backend_summary_text.rs must expose Phase 8 text marker '$requiredMarker'"
        }
    }
foreach ($requiredMarker in @(
        "backend_gpu_worker_contract",
        "gpu_worker",
        "backend_report",
        "memory_ticket_id",
        "fence_epoch",
        "can_source_exact_probability"
    )) {
        if ($jsonContract -notlike "*$requiredMarker*") {
            Add-ArchitectureError "json contract validation surface must expose Phase 8 backend GPU worker marker '$requiredMarker'"
        }
    }
$replayJsonContract = Read-Text "crates/clearra-output/src/json/replay_json_contract.rs"
$jsonReplaySurface = "$jsonContract`n$replayJsonContract"
foreach ($requiredMarker in @("from_replay_trace", "replay_trace_object", "colored_cell_ownership", "line-clear", "representative", "sample", "replay_trace_contract_preserves_marker_line_clear_and_colored_ownership")) {
        if ($jsonReplaySurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "json_contract.rs must expose M16 ReplayTrace JSON marker '$requiredMarker'"
        }
    }
$fumenWriter = Read-Text "crates/clearra-fumen/src/codec/fumen_like_writer.rs"
foreach ($requiredMarker in @("write_replay_trace", "kind=replay-trace", "kind=replay-step", "writes_replay_trace_as_fumen_like_comment_pages")) {
        if ($fumenWriter -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-fumen fumen_like_writer.rs must expose M16 replay fumen-like marker '$requiredMarker'"
        }
    }
$outputFumenBridge = Read-Text "crates/clearra-output/src/fumen_like/mod.rs"
foreach ($requiredMarker in @("Compatibility bridge", "clearra-fumen", "pub use clearra_fumen::codec", "FumenLikeWriter")) {
        if ($outputFumenBridge -notlike "*$requiredMarker*") {
            Add-ArchitectureError "clearra-output fumen_like bridge must only re-export clearra-fumen codec marker '$requiredMarker'"
        }
    }
foreach ($forbiddenOutputCodecPath in @(
        "crates/clearra-output/src/fumen_like/fumen_like_reader.rs",
        "crates/clearra-output/src/fumen_like/fumen_like_writer.rs",
        "crates/clearra-output/src/fumen_like/fumen_like_trace.rs"
    )) {
        if (Test-Path -LiteralPath (Join-Path $Root $forbiddenOutputCodecPath)) {
            Add-ArchitectureError "clearra-output must not own fumen codec implementation file after clearra-fumen split: $forbiddenOutputCodecPath"
        }
    }
$fumenCargo = Read-Text "crates/clearra-fumen/Cargo.toml"
if (-not (Test-DependencyLine $fumenCargo "clearra-replay")) {
        Add-ArchitectureError "clearra-fumen must depend on clearra-replay for replay-to-fumen adapters"
    }
$outputCargo = Read-Text "crates/clearra-output/Cargo.toml"
if (-not (Test-DependencyLine $outputCargo "clearra-fumen")) {
    Add-ArchitectureError "clearra-output must depend on clearra-fumen for fumen-like dispatch without owning codec"
}
if (-not (Test-DependencyLine $outputCargo "clearra-render")) {
    Add-ArchitectureError "clearra-output must depend on clearra-render for exact PNG/GIF dispatch"
}
foreach ($requiredRenderMarker in @(
        "SkinManifest",
        "SkinAtlas",
        "SkinProvenance",
        "AtlasBoundsValidator",
        "SkinManifestValidator",
        "SkinProvenanceValidator",
        "RenderOptions",
        "RenderError",
        "RenderCapabilityReport",
        "RenderFrameFormat",
        "RenderUnsupportedReason",
        "render_exact",
        "RenderScene",
        "ExactBitmapRenderer",
        "connected_exact",
        "render_reports_png_and_gif_connected_exact",
        "default_product_skin_manifest_is_valid",
        "unsupported_frame_format_error_carries_capability_reason"
    )) {
        $renderSurface = (Read-Text "crates/clearra-render/src/lib.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/capability/render_capability.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/skin/skin_manifest.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/skin/skin_atlas.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/skin/skin_provenance.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/skin/atlas_bounds_validator.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/skin/skin_manifest_validator.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/skin/skin_provenance_validator.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/options/render_options.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/error/render_error.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/scene.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/bitmap/bitmap_renderer.rs") + "`n" +
            (Read-Text "crates/clearra-render/src/lib_tests.rs") + "`n" +
            (Read-Text "crates/clearra-render/tests/skin_asset_contract.rs")
        if ($renderSurface -notlike "*$requiredRenderMarker*") {
            Add-ArchitectureError "clearra-render must own connected render contract marker '$requiredRenderMarker'"
        }
    }
foreach ($requiredRenderAssetTestMarker in @(
        "default_product_skin_manifest_is_valid",
        "default_skin_provenance_is_valid",
        "skin_manifest_requires_all_standard_pieces",
        "skin_manifest_rejects_out_of_bounds_atlas_rect",
        "skin_provenance_required_for_builtin_asset",
        "renderer_reports_exact_for_png_and_gif"
    )) {
        $renderAssetTests = Read-Text "crates/clearra-render/tests/skin_asset_contract.rs"
        if ($renderAssetTests -notlike "*$requiredRenderAssetTestMarker*") {
            Add-ArchitectureError "clearra-render asset contract tests must pin marker '$requiredRenderAssetTestMarker'"
        }
    }
$skinManifest = Read-Text "assets/skins/default/skin.json"
foreach ($requiredMarker in @('"skin_id": "default"', '"atlas_path": "atlas.png"', '"atlas_format": "png"', '"runtime_raw_svg_allowed": false', '"required_pieces"', '"I"', '"O"', '"T"', '"S"', '"Z"', '"J"', '"L"', '"render_exact": true', '"supported": true')) {
        if ($skinManifest -notlike "*$requiredMarker*") {
            Add-ArchitectureError "default skin manifest must pin asset contract marker '$requiredMarker'"
        }
    }
$skinProvenance = Read-Text "assets/skins/default/provenance.json"
foreach ($requiredMarker in @('"raw_svg_runtime_rendering": false', "sanitize-rasterize-at-build-time", '"atlas_format": "png"', '"atlas_png_sha256"', '"manifest_sha256"')) {
        if ($skinProvenance -notlike "*$requiredMarker*") {
            Add-ArchitectureError "default skin provenance must pin sanitized PNG atlas marker '$requiredMarker'"
        }
    }
$invalidRenderFixture = Read-Text "tests/fixtures/render/skin_manifest_invalid_missing_piece.json"
if ($invalidRenderFixture -notlike '*"expected_error": "missing_piece:T"*') {
        Add-ArchitectureError "invalid render fixture must pin missing T piece diagnostic marker"
    }
$renderCapabilityGolden = Read-Text "tests/golden/render/render_capability_exact.json"
foreach ($requiredMarker in @('"render_exact": true', '"supported": true', '"runtime_asset_format": "png-atlas"')) {
        if ($renderCapabilityGolden -notlike "*$requiredMarker*") {
            Add-ArchitectureError "render capability exact golden must pin marker '$requiredMarker'"
        }
    }
$skinAssetDir = Join-Path $Root "assets/skins"
if (Test-Path -LiteralPath $skinAssetDir) {
        $rawSvgAssets = @(Get-ChildItem -LiteralPath $skinAssetDir -Recurse -File -Filter "*.svg")
        foreach ($rawSvgAsset in $rawSvgAssets) {
            Add-ArchitectureError "runtime skin assets must be sanitized/rasterized PNG atlas files, not raw SVG: $($rawSvgAsset.FullName)"
        }
    }
$textWriter = Read-Text "crates/clearra-output/src/text/text_writer.rs"
foreach ($requiredMarker in @("replay_trace_lines", "replay_trace", "cleared_lines", "replay_trace_renders_to_text")) {
        if ($textWriter -notlike "*$requiredMarker*") {
            Add-ArchitectureError "text_writer.rs must expose M16 replay text marker '$requiredMarker'"
        }
    }
$renderDispatcher = Read-Text "crates/clearra-output/src/render.rs"
foreach ($requiredMarker in @("render_replay_trace", "TextWriter::replay_trace", "JsonContract::from_replay_trace", "FumenLikeWriter::write_replay_trace", "dispatches_replay_trace_to_all_output_formats")) {
        if ($renderDispatcher -notlike "*$requiredMarker*") {
            Add-ArchitectureError "RenderFormatDispatcher must route M16 ReplayTrace marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @("M16 Replay and Output Bridge", "BuildVariantReplayInput", "ReplayEngine::build_variant_to_trace", "colored cell ownership", "line clear events", "representative=true", "sample=true", "clearra-fumen", "clearra-render", "render_exact=true", "supported=true", "must not grow raw fumen")) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M16 marker '$requiredMarker'"
        }
    }
$algorithmsDoc = Read-Text "docs/algorithms.md"
foreach ($requiredMarker in @("Replay/output reduction", "SolutionTraceBuilder", "preserves colored cell ownership", "line clear event payloads", "text, typed JSON, and fumen-like", "clearra-fumen", "clearra-render", "render_exact=true", "PNG atlas")) {
        if ($algorithmsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/algorithms.md must include M16 marker '$requiredMarker'"
        }
    }
$outputFormatsDoc = Read-Text "docs/output-formats.md"
foreach ($requiredMarker in @(
        "backend_report.gpu_worker",
        "memory_ticket_id",
        "fence_epoch",
        "can_source_exact_probability",
        "BackendSummaryText",
        "json_backend_report_includes_gpu_worker_trust_state",
        "json_backend_report_includes_memory_ticket_and_fence_epoch",
        "text_default_summarizes_gpu_worker_without_internal_noise",
        "text_verbose_includes_gpu_worker_backpressure"
    )) {
        if ($outputFormatsDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/output-formats.md must document Phase 8 backend output marker '$requiredMarker'"
        }
    }
}
