# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# G8 adds the full custom rule editor schema and verification gate while keeping
# execution blocked unless a verified profile is explicitly compiled.

function Invoke-CustomRuleEditorContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-rules/src/custom_rule/custom_rule_editor_contract.rs",
            "crates/clearra-validation/src/validators/rule_editor_validator.rs",
            "crates/clearra-ui-schema/src/rule_editor/custom_rule_editor_schema.rs",
            "crates/clearra-core-ffi/src/rules/custom_rule_descriptor_compiler.rs",
            "docs/architecture.md",
            "docs/future-custom-pieces.md",
            "docs/mvp-scope.md",
            "scripts/custom-rule-editor-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredFile))) {
            Add-ArchitectureError "G8 required custom rule editor file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-rules/src/custom_rule/custom_rule_editor_contract.rs"
        Read-Text "crates/clearra-validation/src/validators/rule_editor_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/mod.rs"
        Read-Text "crates/clearra-ui-schema/src/rule_editor/custom_rule_editor_schema.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/custom_rule_descriptor_compiler.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/mod.rs"
        Read-Text "docs/architecture.md"
        Read-Text "docs/future-custom-pieces.md"
        Read-Text "docs/mvp-scope.md"
        Read-Text "scripts/custom-rule-editor-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "CustomRuleEditorSchema",
            "rotation_states",
            "spawn_rules",
            "kick_transitions",
            "first_success_order",
            "supports_180",
            "piece_specific_overrides",
            "line_clear_policy",
            "lock_reachability_mode",
            "CustomRuleVerificationReport",
            "missing_transition",
            "duplicate_transition",
            "invalid_rotation",
            "unsupported_piece",
            "unsupported_board_backend",
            "unsupported_runtime_feature",
            "VerifiedCustomRuleProfile",
            "CustomRuleDescriptorCompiler",
            "custom_rule_editor_schema_validates",
            "custom_rule_verify_reports_missing_transition",
            "custom_rule_verify_reports_duplicate_transition",
            "verified_custom_rule_can_compile_to_descriptor_when_supported",
            "unverified_custom_rule_rejected_before_execution",
            "compile-rust-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "G8 custom rule editor contract must expose marker '$requiredMarker'"
        }
    }
foreach ($forbiddenMarker in @(
            "custom_rule_fallback_to_srs",
            "unsupported_custom_rule_fallback_to_srs",
            "compile_unverified_custom_rule",
            "first_success_order_omitted",
            "editor_rule_runtime_without_verification"
        )) {
        if ($surface -like "*$forbiddenMarker*") {
            Add-ArchitectureError "G8 must not introduce forbidden custom rule editor shortcut marker '$forbiddenMarker'"
        }
    }
}
