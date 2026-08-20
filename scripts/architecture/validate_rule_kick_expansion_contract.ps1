# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# X1 keeps rule/kick expansion visible, strict, and guarded before C execution.

function Invoke-RuleKickExpansionContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-rules/src/profile/rule_profile.rs",
            "crates/clearra-rules/src/kicks/kick_profile_registry.rs",
            "crates/clearra-rules/src/kicks/kick_import.rs",
            "crates/clearra-rules/src/kicks/kick_verification.rs",
            "crates/clearra-validation/src/validators/rule_validator.rs",
            "crates/clearra-validation/src/validators/rule_verified_kick_profile_validator.rs",
            "crates/clearra-core-ffi/src/rules/imported_kick_descriptor_compiler.rs",
            "crates/clearra-core-ffi/src/rules/rule_descriptor_compiler.rs",
            "crates/clearra-core-ffi/src/rules/rule_descriptor_compiler_tests.rs",
            "crates/clearra-core-ffi/src/rules/custom_rule_descriptor_compiler_tests.rs",
            "crates/clearra-core-executor/src/packing/packing_runner_tests.rs",
            "core-c/include/clr_rules.h",
            "core-c/src/rules/kick_table.c",
            "core-c/src/candidate/candidate_search_dispatch.c",
            "crates/clearra-ui-schema/src/rule_editor/kick_table_preview_schema.rs",
            "docs/rules-and-kicks.md",
            "scripts/rule-kick-expansion-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath $requiredFile)) {
            Add-ArchitectureError "X1 required rule/kick expansion file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-rules/src/profile/rule_profile.rs"
        Read-Text "crates/clearra-rules/src/kicks/kick_profile_registry.rs"
        Read-Text "crates/clearra-rules/src/kicks/kick_import.rs"
        Read-Text "crates/clearra-rules/src/kicks/kick_verification.rs"
        Read-Text "crates/clearra-validation/src/validators/rule_diagnostic_builder.rs"
        Read-Text "crates/clearra-validation/src/validators/rule_verified_kick_profile_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/rule_validator_tests.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/imported_kick_descriptor_compiler.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/rule_descriptor_compiler.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/rule_descriptor_compiler_tests.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/custom_rule_descriptor_compiler_tests.rs"
        Read-Text "crates/clearra-core-executor/src/packing/packing_runner_tests.rs"
        Read-Text "core-c/src/rules/kick_table.c"
        Read-Text "crates/clearra-ui-schema/src/rule_editor/kick_table_preview_schema.rs"
        Read-Text "docs/rules-and-kicks.md"
        Read-Text "scripts/rule-kick-expansion-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "RuleProfileId::SrsX",
            "RuleProfileId::Asc",
            "RuleProfileId::Ars",
            "KickProfileSourceKind::ImportedVerified",
            "transition_count",
            "supports_180",
            "supports_exact_180",
            "first_success_order_preserved",
            "provenance",
            "verified",
            "exact_object",
            "duplicate_transition_count",
            "missing_transition_count",
            "UnknownRotation",
            "UnknownPiece",
            "VerifiedKickProfileMissingRequired180",
            "verified_profile_missing_required_180",
            "srs_x_verified_profile_requires_exact_180_transition_set",
            "asc_profile_validates_as_guarded_descriptor",
            "ars_profile_validates_as_guarded_descriptor",
            "imported_verified_kick_profile_compiles_to_c_descriptor",
            "builtin_srs_x_projects_the_canonical_verified_table_to_c",
            "builtin_srs_x_uses_the_canonical_verified_table_during_native_packing",
            "unverified_custom_rule_rejected_before_execution",
            "clearra_rule_transition_is_180",
            "!table->supports_180",
            "compile-and-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X1 rule/kick expansion must expose marker '$requiredMarker'"
        }
    }
$ffiBuiltinCompiler = Read-Text "crates/clearra-core-ffi/src/rules/rule_capability_descriptor.rs"
foreach ($forbiddenMarker in @(
            "RuleProfileId::SrsX => no_kick_profile_id",
            "RuleProfileId::Asc => no_kick_profile_id",
            "RuleProfileId::Ars => no_kick_profile_id",
            "RuleProfileId::Custom => no_kick_profile_id"
        )) {
        if ($ffiBuiltinCompiler -like "*$forbiddenMarker*") {
            Add-ArchitectureError "X1 unsupported rule profiles must not silently compile as NoKick marker '$forbiddenMarker'"
        }
    }
}
