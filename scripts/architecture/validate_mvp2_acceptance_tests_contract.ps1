# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-Mvp2AcceptanceTestsContractValidation() {
$requiredFiles = @(
        "scripts/mvp2-acceptance.ps1",
        "scripts/score-profile-object-check.ps1",
        "scripts/score-aware-objective-check.ps1",
        "scripts/spin-target-contract-check.ps1",
        "scripts/setup-raw-metrics-v2-check.ps1",
        "scripts/fumen-render-product-check.ps1",
        "crates/clearra-output/src/scoring/score_profile_output_contract.rs",
        "crates/clearra-core-executor/src/spin/spin_target_runner_tests.rs",
        "crates/clearra-objectives/src/max_score/max_score_cover.rs",
        "crates/clearra-output/src/json/setup_json_contract_tests.rs",
        "crates/clearra-render/src/lib.rs",
        "crates/clearra-host-contract/src/render_capability_report.rs",
        "crates/clearra-app/src/app_response.rs",
        "crates/clearra-gui-host/src/display/render/render_capability_view.rs",
        "packages/clearra-ui/src/lib/render/renderCapabilityReport.ts",
        "packages/clearra-ui/src/lib/render/RenderStatusPanel.svelte",
        "packages/clearra-ui/src/lib/components/DesktopHostShell.svelte",
        "packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts",
        "docs/architecture.md",
        "docs/test-policy.md"
    )
foreach ($relativePath in $requiredFiles) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $relativePath))) {
            Add-ArchitectureError "T6 MVP2 acceptance tests required file is missing: $relativePath"
        }
    }
$scoreProfileContract = Read-Text "crates/clearra-output/src/scoring/score_profile_output_contract.rs"
foreach ($requiredMarker in @(
        "score_profile_reports_accuracy_level",
        "tetrio_not_profile_specific_exact_until_exact_supported",
        "tetrio_profile_reports_basic_approximation_until_exact",
        "accuracy_level",
        "basic-approximation",
        "profile_specific_exact",
        "exact_claim_allowed"
    )) {
        if ($scoreProfileContract -notlike "*$requiredMarker*") {
            Add-ArchitectureError "score profile output contract must keep T6 marker '$requiredMarker'"
        }
    }
$spinTargetTests = Read-Text "crates/clearra-core-executor/src/spin/spin_target_runner_tests.rs"
foreach ($requiredMarker in @(
        "spin_target_requires_classifier",
        "spin_target_runner_rejects_missing_spin_classifier",
        "MissingSpinClassifier",
        "missing_kick_evidence_is_incomplete_not_exact",
        "missing_kick_evidence_is_incomplete_not_exact_spin",
        "CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING",
        "W_SPIN_TARGET_PROBABILITY_INCOMPLETE"
    )) {
        if ($spinTargetTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "spin target tests must keep T6 marker '$requiredMarker'"
        }
    }
$maxScoreCoverTests = Read-Text "crates/clearra-objectives/src/max_score/max_score_cover.rs"
foreach ($requiredMarker in @(
        "max_score_cover_does_not_double_count_probability",
        "score_aware_cover_uses_pattern_union_probability_not_variant_sum",
        "union_probability",
        "covered_probability().get(), 0.4",
        "selected_candidate_ids(), &[8]"
    )) {
        if (-not $maxScoreCoverTests.Contains($requiredMarker)) {
            Add-ArchitectureError "max score cover tests must keep T6 marker '$requiredMarker'"
        }
    }
$setupJsonTests = Read-Text "crates/clearra-output/src/json/setup_json_contract_tests.rs"
foreach ($requiredMarker in @(
        "setup_raw_metrics_no_condition_summary",
        "setup_contract_exposes_x3_raw_metrics_without_condition_summary",
        "setup_raw_metrics",
        "raw_metrics",
        "condition_summary"
    )) {
        if ($setupJsonTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "setup output tests must keep T6 marker '$requiredMarker'"
        }
    }
if ($setupJsonTests -notlike "*!format*condition_summary*") {
        Add-ArchitectureError "setup_raw_metrics_no_condition_summary must assert condition_summary is absent"
    }
$renderTests = @(
    Read-Text "crates/clearra-render/src/lib.rs"
    Read-Text "crates/clearra-render/src/lib_tests.rs"
    Read-Text "crates/clearra-render/src/bitmap/bitmap_renderer_tests.rs"
    Read-Text "crates/clearra-host-contract/src/render_capability_report.rs"
    Read-Text "crates/clearra-app/src/app_response.rs"
    Read-Text "crates/clearra-gui-host/src/display/render/render_capability_view.rs"
) -join "`n"
foreach ($requiredMarker in @(
        "renderer_connected_exact",
        "renderer_capability_matches_runtime_report",
        "render_ui_matches_runtime_capability",
        "runtime_render_capability_report",
        "render_capability_reports_exact_for_both_formats",
        "RenderCapabilityReport::current",
        "assert!(capability.supported())",
        "assert!(capability.render_exact())",
        "png_lock_frame_render_golden",
        "gif_timeline_render_golden"
    )) {
        if ($renderTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "render capability tests must keep T6 marker '$requiredMarker'"
        }
    }
$renderStatusUi = Read-Text "packages/clearra-ui/src/lib/render/RenderStatusPanel.svelte"
$desktopHostShell = Read-Text "packages/clearra-ui/src/lib/components/DesktopHostShell.svelte"
$wasmCommandClient = Read-Text "packages/clearra-ui/src/lib/wasm/wasmCommandClient.ts"
foreach ($requiredMarker in @(
        "export let capability",
        "capability?.png_supported",
        "capability?.gif_supported",
        "capability.render_exact",
        "capability.unsupported_reason"
    )) {
    if ($renderStatusUi -notlike "*$requiredMarker*") {
        Add-ArchitectureError "render_status_ui_uses_product_capability failed: missing '$requiredMarker'"
    }
}
if ($desktopHostShell -notlike '*state.result?.capability_report.render_capability*' -or
    $desktopHostShell -notlike '*<RenderStatusPanel {capability} />*') {
    Add-ArchitectureError "render_status_ui_uses_product_capability failed: desktop shell must pass AppResponse render capability"
}
if ($wasmCommandClient -notlike '*render_capability: RenderCapabilityReport*') {
    Add-ArchitectureError "render_status_ui_uses_product_capability failed: WASM AppResponse type must share the render capability contract"
}
foreach ($staleUiMarker in @(
        '<dd>unsupported</dd>',
        '<dd>false</dd>',
        'renderer_not_connected',
        'render_capability_unavailable'
    )) {
    if ($renderStatusUi -like "*$staleUiMarker*") {
        Add-ArchitectureError "architecture_validation_rejects_stale_renderer_not_connected_ui failed: '$staleUiMarker'"
    }
}
$acceptanceGate = Read-Text "scripts/mvp2-acceptance.ps1"
foreach ($requiredMarker in @(
        "MVP2 Scoring tests",
        "scripts/score-profile-object-check.ps1",
        "MVP2 Score objective tests",
        "scripts/score-aware-objective-check.ps1",
        "SpinTarget coverage tests",
        "scripts/spin-target-contract-check.ps1",
        "Setup raw metrics tests",
        "scripts/setup-raw-metrics-v2-check.ps1",
        "Render/Fumen transform tests",
        "scripts/fumen-render-product-check.ps1",
        "mvp2_exact_claims_guarded=true",
        "mvp2_scoring_basic_approximation_disclosed=true",
        "mvp2_renderer_exact_only_when_supported=true"
    )) {
        if ($acceptanceGate -notlike "*$requiredMarker*") {
            Add-ArchitectureError "MVP2 acceptance gate must keep T6 marker '$requiredMarker'"
        }
    }
$checkScripts = @(
        Read-Text "scripts/score-profile-object-check.ps1"
        Read-Text "scripts/score-aware-objective-check.ps1"
        Read-Text "scripts/spin-target-contract-check.ps1"
        Read-Text "scripts/setup-raw-metrics-v2-check.ps1"
        Read-Text "scripts/fumen-render-product-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
        "cargo check -p clearra-output --tests",
        "cargo check -p clearra-objectives --tests",
        "cargo check -p clearra-core-executor --tests",
        "cargo check -p clearra-render --tests",
        "test_executable_launched=false"
    )) {
        if ($checkScripts -notlike "*$requiredMarker*") {
            Add-ArchitectureError "MVP2 compile-only check scripts must keep T6 marker '$requiredMarker'"
        }
    }
$taskList = Read-Text "scripts/lib/architecture-validation-tasks.ps1"
foreach ($requiredMarker in @(
        "T6 MVP2 Acceptance Tests",
        "Invoke-Mvp2AcceptanceTestsContractValidation"
    )) {
        if ($taskList -notlike "*$requiredMarker*") {
            Add-ArchitectureError "architecture validation task list must include T6 marker '$requiredMarker'"
        }
    }
$docSurface = @(
        Read-Text "docs/architecture.md"
        Read-Text "docs/test-policy.md"
    ) -join "`n"
foreach ($requiredMarker in @(
        "T6 MVP2 Acceptance Tests",
        "score_profile_reports_accuracy_level",
        "tetrio_not_profile_specific_exact_until_exact_supported",
        "spin_target_requires_classifier",
        "missing_kick_evidence_is_incomplete_not_exact",
        "max_score_cover_does_not_double_count_probability",
        "setup_raw_metrics_no_condition_summary",
        "renderer_connected_exact",
        "renderer_capability_matches_runtime_report",
        "render_status_ui_uses_product_capability"
    )) {
        if ($docSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs must document T6 marker '$requiredMarker'"
        }
    }
foreach ($staleDocMarker in @(
        "renderer_supported_false_until_connected",
        "renderer_not_connected",
        "render_capability_unavailable"
    )) {
    if ($docSurface -like "*$staleDocMarker*") {
        Add-ArchitectureError "render_docs_match_connected_exact_state failed: stale marker '$staleDocMarker'"
    }
}
}
