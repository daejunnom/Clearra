# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# X0 keeps MVP2 expansion guarded by capability state and exact-claim gates.

function Invoke-Mvp2ScopeGateContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-validation/src/capability/mvp2_capability_registry.rs",
            "crates/clearra-validation/src/capability/mod.rs",
            "crates/clearra-validation/src/lib.rs",
            "scripts/mvp2-scope-gate-check.ps1",
            "docs/mvp-scope.md"
        )) {
        if (-not (Test-Path -LiteralPath $requiredFile)) {
            Add-ArchitectureError "X0 required MVP2 scope gate file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-validation/src/capability/mvp2_capability_registry.rs"
        Read-Text "crates/clearra-validation/src/capability/mod.rs"
        Read-Text "crates/clearra-validation/src/lib.rs"
        Read-Text "scripts/mvp2-scope-gate-check.ps1"
        Read-Text "docs/mvp-scope.md"
    ) -join "`n"
foreach ($requiredMarker in @(
            "Mvp2CapabilityReport",
            "Mvp2CapabilityState",
            "Unsupported",
            "Preview",
            "Skeleton",
            "BasicApproximation",
            "Exact",
            "RuleKickExpansion",
            "ScoringPostProcessing",
            "SpinTargetSkeleton",
            "SetupRawMetricsV2",
            "BuildEditorSchema",
            "RendererPngSkeleton",
            "RendererGifSkeleton",
            "GpuPackingStrengthening",
            "HybridScheduler",
            "mvp2_capability_report_lists_all_mvp2_features",
            "mvp2_exact_claims_require_capability_exact",
            "mvp2_unsupported_features_emit_disabled_reason",
            "ExactClaimRequiresCapabilityExact",
            "disabled_reason",
            "exact_transition_condition",
            "compile-and-architecture-only",
            "test_executable_launched=false",
            "Skeleton을 exact로 표시",
            "BasicApproximation을 profile-specific exact로 표시",
            "MVP2 feature failure를 MVP1 failure로 처리"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X0 MVP2 scope gate must expose marker '$requiredMarker'"
        }
    }
$registry = Read-Text "crates/clearra-validation/src/capability/mvp2_capability_registry.rs"
foreach ($nonExactId in @(
            "RuleKickExpansion",
            "ScoringPostProcessing",
            "SpinTargetSkeleton",
            "SetupRawMetricsV2",
            "BuildEditorSchema",
            "RendererPngSkeleton",
            "RendererGifSkeleton",
            "GpuPackingStrengthening",
            "HybridScheduler"
        )) {
        $exactPattern = "Mvp2CapabilityId::$nonExactId,\s*`r?`n\s*Mvp2CapabilityState::Exact"
        if ($registry -match $exactPattern) {
            Add-ArchitectureError "X0 MVP2 capability '$nonExactId' must not be marked Exact before its transition condition is proven"
        }
    }
}
