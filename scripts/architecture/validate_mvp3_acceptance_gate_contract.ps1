# This file is dot-sourced by scripts/lib/architecture-validation.ps1.

function Invoke-Mvp3AcceptanceGateContractValidation() {
foreach ($requiredFile in @(
            "scripts/mvp3-acceptance.ps1",
            "scripts/product-e2e.ps1",
            "scripts/mvp2-acceptance.ps1",
            "scripts/mvp3-scope-gate-check.ps1",
            "scripts/custom-piece-domain-check.ps1",
            "scripts/mixed-supply-generalization-check.ps1",
            "scripts/board128-wide-runtime-check.ps1",
            "scripts/generic-operation-candidate-reachability-check.ps1",
            "scripts/area-multiset-feasibility-check.ps1",
            "scripts/generic-exact-cover-dlx-check.ps1",
            "scripts/generic-buildup-check.ps1",
            "scripts/custom-rule-editor-check.ps1",
            "scripts/generic-gpu-descriptor-check.ps1",
            "scripts/custom-skin-theme-editor-check.ps1",
            "scripts/clearra.ps1",
            "docs/test-policy.md",
            "docs/architecture.md",
            "docs/mvp-scope.md"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "G11 MVP3 acceptance gate required file is missing: $requiredFile"
        }
    }
$gateScript = Read-Text "scripts/mvp3-acceptance.ps1"
foreach ($requiredMarker in @(
            "Invoke-Mvp3AcceptanceStep",
            "standard_fast_path_unchanged_under_mvp3",
            "MVP1 ProductE2E",
            "scripts/product-e2e.ps1",
            "MVP2 Acceptance",
            "scripts/mvp2-acceptance.ps1",
            "MVP3 Scope Gate",
            "scripts/mvp3-scope-gate-check.ps1",
            "Custom piece schema tests",
            "scripts/custom-piece-domain-check.ps1",
            "Mixed bag schema tests",
            "scripts/mixed-supply-generalization-check.ps1",
            "Board128/Wide descriptor tests",
            "scripts/board128-wide-runtime-check.ps1",
            "Area multiset feasibility tests",
            "scripts/area-multiset-feasibility-check.ps1",
            "DLX tests",
            "scripts/generic-exact-cover-dlx-check.ps1",
            "Unsupported runtime guard tests",
            "scripts/generic-buildup-check.ps1",
            "Custom rule editor validation tests",
            "scripts/custom-rule-editor-check.ps1",
            "Generic GPU descriptor tests",
            "scripts/generic-gpu-descriptor-check.ps1",
            "Architecture validation",
            "scripts/validate_architecture.ps1",
            "custom_features_guarded_until_runtime_connected=true",
            "no_silent_fallback_to_standard_path=true",
            "generic_cache_keys_include_piece_board_rule_supply_identity=true"
        )) {
        if ($gateScript -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G11 MVP3 acceptance gate must expose marker '$requiredMarker'"
        }
    }
$productIndex = $gateScript.IndexOf("scripts/product-e2e.ps1", [StringComparison]::Ordinal)
$mvp2Index = $gateScript.IndexOf("scripts/mvp2-acceptance.ps1", [StringComparison]::Ordinal)
if ($productIndex -lt 0 -or $mvp2Index -lt 0 -or $mvp2Index -lt $productIndex) {
        Add-ArchitectureError "G11 MVP3 acceptance gate must run MVP1 ProductE2E before MVP2 Acceptance"
    }
foreach ($laterMarker in @(
            "scripts/custom-piece-domain-check.ps1",
            "scripts/mixed-supply-generalization-check.ps1",
            "scripts/board128-wide-runtime-check.ps1",
            "scripts/area-multiset-feasibility-check.ps1",
            "scripts/generic-exact-cover-dlx-check.ps1",
            "scripts/custom-rule-editor-check.ps1",
            "scripts/generic-gpu-descriptor-check.ps1",
            "scripts/validate_architecture.ps1"
        )) {
        $laterIndex = $gateScript.IndexOf($laterMarker, [StringComparison]::Ordinal)
        if ($laterIndex -lt $productIndex) {
            Add-ArchitectureError "G11 MVP3 acceptance gate must run MVP1 ProductE2E before '$laterMarker'"
        }
    }
$entrypointSurface = @(
        Read-Text "scripts/clearra.ps1"
        Read-Text "scripts/lib/clearra-task-ui-helpers.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "Mvp3Acceptance",
            "mvp3-acceptance",
            "scripts/mvp3-acceptance.ps1"
        )) {
        if ($entrypointSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G11 entrypoint surface must expose marker '$requiredMarker'"
        }
    }
$identitySurface = @(
        Read-Text "core-c/src/cache/cache_identity.h"
        Read-Text "core-c/src/cache/cache_key.c"
        Read-Text "crates/clearra-core-ffi/src/problem/mod.rs"
        Read-Text "docs/architecture.md"
    ) -join "`n"
foreach ($requiredMarker in @(
            "piece_definition_id_fingerprint",
            "piece_area_multiset_fingerprint",
            "rule_kick_profile",
            "supply_provenance",
            "queue_pattern_id",
            "goal_id"
        )) {
        if ($identitySurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G11 generic cache identity must include marker '$requiredMarker'"
        }
    }
$docSurface = @(
        Read-Text "docs/architecture.md"
        Read-Text "docs/mvp-scope.md"
        Read-Text "docs/test-policy.md"
    ) -join "`n"
foreach ($requiredMarker in @(
            "G11 MVP3 Acceptance Gate",
            "standard_fast_path_unchanged_under_mvp3",
            "custom_features_guarded_until_runtime_connected",
            "no_silent_fallback_to_standard_path",
            "generic_cache_keys_include_piece_board_rule_supply_identity",
            "custom unsupported를 empty success로 처리",
            "standard와 generic cache key 충돌"
        )) {
        if ($docSurface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G11 docs must expose marker '$requiredMarker'"
        }
    }
}
