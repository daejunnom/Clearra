# This file is dot-sourced by an architecture validation wrapper.

function Invoke-RulesKicksRuntimeValidation() {
foreach ($requiredPath in @(
            "crates/clearra-core-ffi/src/rules/rule_descriptor_compiler.rs",
            "crates/clearra-core-ffi/src/rules/rule_descriptor_compiler_tests.rs",
            "crates/clearra-core-ffi/src/rules/custom_rule_descriptor_compiler_tests.rs",
            "crates/clearra-core-ffi/src/rules/mod.rs",
            "crates/clearra-core-executor/src/packing/packing_runner_tests.rs",
            "core-c/include/clr_rules.h",
            "core-c/src/rules/rule_profile.c",
            "core-c/tests/rule_profile_tests.c"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $Root $requiredPath))) {
            Add-ArchitectureError "M22 rules/kicks product path required file is missing: $requiredPath"
        }
    }
$clrRules = Read-Text "core-c/include/clr_rules.h"
foreach ($requiredMarker in @(
            "CLR_RULE_MAX_KICK_OFFSETS",
            "CLR_RULE_MAX_KICK_TRANSITIONS",
            "clr_kick_transition_descriptor",
            "has_verified_kick_profile",
            "verified_supports_180",
            "verified_transition_count",
            "verified_transitions"
        )) {
        if ($clrRules -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M22 clr_rules.h must carry verified compact kick descriptor marker '$requiredMarker'"
        }
    }
$ruleProfileC = Read-Text "core-c/src/rules/rule_profile.c"
foreach ($requiredMarker in @(
            "fill_verified_kick_table",
            "descriptor->has_verified_kick_profile",
            "descriptor->verified_transition_count",
            "descriptor->verified_transitions",
            "clearra_kick_table_push",
            "clearra_rule_profile_from_descriptor"
        )) {
        if ($ruleProfileC -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M22 C runtime must compile verified descriptors into compact kick tables marker '$requiredMarker'"
        }
    }
$ruleDescriptorCompiler = @(
        Read-Text "crates/clearra-core-ffi/src/rules/rule_descriptor_compiler.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/rule_descriptor_compiler_tests.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/custom_rule_descriptor_compiler_tests.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/rule_capability_descriptor.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/imported_kick_descriptor_compiler.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/kick_table_identity_mapper.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/srs_descriptor_compiler.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/srs_plus_descriptor_compiler.rs"
        Read-Text "crates/clearra-core-ffi/src/rules/no_kick_descriptor_compiler.rs"
    ) -join "`n"
foreach ($requiredMarker in @(
            "RuleDescriptorCompiler",
            "VerifiedKickTableProfile",
            "compile_verified_profile",
            "compile_builtin_profile",
            "UnverifiedRuleProfileRejected",
            "VerifiedKickProfileRuleMismatch",
            "KickTransitionCountTooLarge",
            "KickOffsetSequenceTooLong",
            "imported_verified_kick_profile_compiles_to_c_descriptor",
            "builtin_srs_x_projects_the_canonical_verified_table_to_c",
            "unverified_custom_rule_rejected_before_execution"
        )) {
        if ($ruleDescriptorCompiler -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M22 RuleDescriptorCompiler must connect Rust rules to C descriptors marker '$requiredMarker'"
        }
    }
$packingProblemBuilder = @(
        Read-Text "crates/clearra-core-ffi/src/problem/packing_problem_builder.rs"
        Read-Text "crates/clearra-core-ffi/src/problem/packing_rule_descriptor_builder.rs"
    ) -join "`n"
if ($packingProblemBuilder -notlike "*RuleDescriptorCompiler::compile(problem)*") {
        Add-ArchitectureError "M22 CPackingProblemBuilder must use RuleDescriptorCompiler before C execution"
    }
$packingProblemBuilderFacade = Read-Text "crates/clearra-core-ffi/src/problem/packing_problem_builder.rs"
foreach ($forbiddenMarker in @(
            "fn rule_descriptor(",
            "fn rule_profile_code(",
            "fn kick_profile_code("
        )) {
        if ($packingProblemBuilderFacade -like "*$forbiddenMarker*") {
            Add-ArchitectureError "M22 CPackingProblemBuilder must not own rule/kick mapping marker '$forbiddenMarker'"
        }
    }
$packingRunner = @(
        Read-Text "crates/clearra-core-executor/src/packing/packing_runner.rs"
        Read-Text "crates/clearra-core-executor/src/packing/packing_runner_tests.rs"
    ) -join "`n"
$packingMetrics = Read-Text "crates/clearra-core-executor/src/packing/packing_metrics.rs"
$packingSurface = "$packingRunner`n$packingMetrics"
foreach ($requiredMarker in @(
            "verified_imported_kick_profile_reaches_c_packing_descriptor",
            "builtin_srs_x_uses_the_canonical_verified_table_during_native_packing",
            "has_verified_kick_profile",
            "verified_transition_count"
        )) {
        if ($packingRunner -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M22 PackingRunner must prove verified/unverified rule product path marker '$requiredMarker'"
        }
    }
$pcService = Get-PcServiceValidationSurface
foreach ($requiredMarker in @(
            "compact_has_verified_kick_profile",
            "compact_verified_kick_transition_count"
        )) {
        if ($pcService -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M22 PC service must expose compact verified kick descriptor evidence marker '$requiredMarker'"
        }
    }
$ruleProfileTests = Read-Text "core-c/tests/rule_profile_tests.c"
foreach ($requiredMarker in @(
            "imported_verified_kick_profile_compiles_to_compact_descriptor_fixture",
            "CLR_KICK_IMPORTED",
            "verified_transition_count",
            "clearra_rule_profile_from_descriptor"
        )) {
        if ($ruleProfileTests -notlike "*$requiredMarker*") {
            Add-ArchitectureError "M22 C rule tests must verify imported descriptor compilation marker '$requiredMarker'"
        }
    }
$architectureDoc = Read-Text "docs/architecture.md"
foreach ($requiredMarker in @(
            "M22 Rules / Kicks Runtime",
            "X1 MVP2 Rule / Kick Expansion",
            "clearra-rules",
            "RuleProfile + optional VerifiedKickTableProfile -> RuleDescriptorCompiler -> clr_rule_profile_descriptor -> clearra_rule_profile_from_descriptor -> ClearraCompactRuleProfile",
            "SRS, SRS+, Jstris 180, and NoKick compile to direct built-in C descriptors",
            "Canonical SRS-X is projected through the verified descriptor ABI",
            "Imported kick tables compile only through",
            "Unverified imported and custom profiles, plus unsupported ASC and ARS profiles, are rejected",
            "supports_exact_180",
            "c_compact_descriptor_ready",
            "unsupported_backend_reason"
        )) {
        if ($architectureDoc -notlike "*$requiredMarker*") {
            Add-ArchitectureError "docs/architecture.md must document M22 rules/kicks product path marker '$requiredMarker'"
        }
    }
}



