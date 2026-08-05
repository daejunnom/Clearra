# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# G0 keeps MVP3 generalization out of the standard tetromino fast path.

function Invoke-Mvp3ScopeGateContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-validation/src/capability/mvp3_capability_registry.rs",
            "crates/clearra-validation/src/capability/mod.rs",
            "crates/clearra-validation/src/lib.rs",
            "scripts/mvp3-scope-gate-check.ps1",
            "docs/mvp-scope.md",
            "docs/architecture.md"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "G0 required MVP3 scope gate file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-validation/src/capability/mvp3_capability_registry.rs"
        Read-Text "crates/clearra-validation/src/capability/mod.rs"
        Read-Text "crates/clearra-validation/src/lib.rs"
        Read-Text "scripts/mvp3-scope-gate-check.ps1"
        Read-Text "docs/mvp-scope.md"
        Read-Text "docs/architecture.md"
    ) -join "`n"
foreach ($requiredMarker in @(
            "Mvp3CapabilityReport",
            "Mvp3CapabilityState",
            "SchemaOnly",
            "ValidationGuard",
            "RuntimeConnected",
            "ExactSupported",
            "Unsupported",
            "CustomPieceSchema",
            "MixedPieceSet",
            "CustomBagProfile",
            "CustomBoardWidth",
            "Board128Runtime",
            "WideBoardRuntime",
            "GenericOperationTable",
            "GenericExactCover",
            "DlxSolver",
            "AreaMultisetFeasibility",
            "CustomRuleEditor",
            "GenericGpuDescriptor",
            "GpuBuildUpExpansion",
            "CustomSkinEditor",
            "mvp3_capability_report_lists_all_generalization_features",
            "schema_only_features_do_not_execute_runtime",
            "unsupported_features_emit_disabled_reason",
            "standard_fast_path_unchanged",
            "RuntimeExecutionRequiresRuntimeConnected",
            "ExactClaimRequiresExactSupported",
            "disabled_reason",
            "runtime_transition_condition",
            "standard_fast_path_impact",
            "compile-and-architecture-only",
            "test_executable_launched=false",
            "custom feature를 standard fast path로 조용히 fallback",
            "generic schema 추가 후 runtime이 연결된 것처럼 표시",
            "MVP3 cache key가 standard enum만 사용"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G0 MVP3 scope gate must expose marker '$requiredMarker'"
        }
    }
$registry = Read-Text "crates/clearra-validation/src/capability/mvp3_capability_registry.rs"
foreach ($schemaOnlyId in @(
            "CustomPieceSchema",
            "GenericOperationTable",
            "CustomRuleEditor",
            "CustomSkinEditor"
        )) {
        $runtimePattern = "Mvp3CapabilityId::$schemaOnlyId,\s*`r?`n\s*Mvp3CapabilityState::RuntimeConnected"
        $exactPattern = "Mvp3CapabilityId::$schemaOnlyId,\s*`r?`n\s*Mvp3CapabilityState::ExactSupported"
        if ($registry -match $runtimePattern -or $registry -match $exactPattern) {
            Add-ArchitectureError "G0 schema-only MVP3 capability '$schemaOnlyId' must not execute runtime or claim exact support"
        }
    }
foreach ($generalizationId in @(
            "MixedPieceSet",
            "CustomBagProfile",
            "CustomBoardWidth",
            "Board128Runtime",
            "WideBoardRuntime",
            "GenericExactCover",
            "DlxSolver",
            "AreaMultisetFeasibility",
            "GenericGpuDescriptor",
            "GpuBuildUpExpansion"
        )) {
        $exactPattern = "Mvp3CapabilityId::$generalizationId,\s*`r?`n\s*Mvp3CapabilityState::ExactSupported"
        if ($registry -match $exactPattern) {
            Add-ArchitectureError "G0 MVP3 capability '$generalizationId' must not claim exact support before generic runtime fixtures prove it"
        }
    }
}
