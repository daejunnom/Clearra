# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-Mvp2AcceptanceGateContractValidation() {
foreach ($requiredFile in @(
            "scripts/mvp2-acceptance.ps1",
            "scripts/product-e2e.ps1",
            "scripts/rule-kick-expansion-check.ps1",
            "scripts/score-profile-object-check.ps1",
            "scripts/score-aware-objective-check.ps1",
            "scripts/spin-target-contract-check.ps1",
            "scripts/setup-raw-metrics-v2-check.ps1",
            "scripts/fumen-render-product-check.ps1",
            "scripts/gpu-packing-strengthening-check.ps1",
            "scripts/gui-editor-schema-v2-check.ps1",
            "scripts/clearra.ps1",
            "scripts/start-tests.ps1",
            "docs/test-policy.md",
            "docs/architecture.md"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "X10 MVP2 acceptance gate required file is missing: $requiredFile"
        }
    }
$gateScript = Read-Text "scripts/mvp2-acceptance.ps1"
foreach ($requiredMarker in @(
            "Invoke-Mvp2AcceptanceStep",
            "mvp2_acceptance_runs_mvp1_product_e2e_first",
            "MVP1 ProductE2E",
            "scripts/product-e2e.ps1",
            "MVP2 Rule/Kick tests",
            "scripts/rule-kick-expansion-check.ps1",
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
            "GPU portable/reference tests",
            "scripts/gpu-packing-strengthening-check.ps1",
            "GUI schema tests",
            "scripts/gui-editor-schema-v2-check.ps1",
            "Architecture validation",
            "scripts/validate_architecture.ps1",
            "StaticProductContractOnly",
            "mvp2_exact_claims_guarded=true",
            "mvp2_scoring_basic_approximation_disclosed=true",
            "mvp2_renderer_exact_only_when_supported=true",
            "mvp2_gpu_fallback_reason_visible=true"
        )) {
        if ($gateScript -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X10 MVP2 acceptance gate must expose marker '$requiredMarker'"
        }
    }
$productIndex = $gateScript.IndexOf("scripts/product-e2e.ps1", [StringComparison]::Ordinal)
if ($productIndex -lt 0) {
        Add-ArchitectureError "X10 MVP2 acceptance gate must call ProductE2E first"
    } else {
        foreach ($laterMarker in @(
                "scripts/rule-kick-expansion-check.ps1",
                "scripts/score-profile-object-check.ps1",
                "scripts/spin-target-contract-check.ps1",
                "scripts/setup-raw-metrics-v2-check.ps1",
                "scripts/fumen-render-product-check.ps1",
                "scripts/gpu-packing-strengthening-check.ps1",
                "scripts/gui-editor-schema-v2-check.ps1",
                "scripts/validate_architecture.ps1"
            )) {
            $laterIndex = $gateScript.IndexOf($laterMarker, [StringComparison]::Ordinal)
            if ($laterIndex -lt $productIndex) {
                Add-ArchitectureError "X10 MVP2 acceptance gate must run MVP1 ProductE2E before '$laterMarker'"
            }
        }
    }
$entrypointSurface = @(
        Read-Text "scripts/clearra.ps1"
        Read-Text "scripts/start-tests.ps1"
        Read-Text "scripts/lib/clearra-task-ui-helpers.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "Mvp2Acceptance",
            "mvp2-acceptance",
            "scripts/mvp2-acceptance.ps1"
        )) {
        if ($entrypointSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X10 entrypoint surface must expose marker '$requiredMarker'"
        }
    }
$docSurface = @(
        Read-Text "docs/architecture.md"
        Read-Text "docs/test-policy.md"
    ) -join "`n"
foreach ($requiredMarker in @(
            "X10 MVP2 Acceptance Gate",
            "MVP1 ProductE2E",
            "mvp2_acceptance_runs_mvp1_product_e2e_first",
            "mvp2_exact_claims_guarded",
            "mvp2_scoring_basic_approximation_disclosed",
            "mvp2_renderer_exact_only_when_supported",
            "mvp2_gpu_fallback_reason_visible",
            "MVP2 feature failure must not break MVP1 pc/path/percent"
        )) {
        if ($docSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X10 docs must expose marker '$requiredMarker'"
        }
    }
}
