# This file is dot-sourced by scripts/lib/architecture-validation.ps1.
# X2 keeps scoring profiles as object composition with guarded exact claims.

function Invoke-ScoreProfileObjectModelContractValidation() {
foreach ($requiredFile in @(
            "crates/clearra-scoring/src/profile/score_profile.rs",
            "crates/clearra-scoring/src/profile/score_profile_registry.rs",
            "crates/clearra-scoring/src/model/score_model_registry.rs",
            "crates/clearra-scoring/src/model/attack_model_registry.rs",
            "crates/clearra-scoring/src/spin/spin_classifier_registry.rs",
            "crates/clearra-scoring/src/profile/drop_score_policy_registry.rs",
            "crates/clearra-scoring/src/profile/level_policy_registry.rs",
            "crates/clearra-scoring/src/profile/level_policy.rs",
            "crates/clearra-scoring/src/profile/pc_bonus_policy.rs",
            "crates/clearra-scoring/src/profile/trace_requirement.rs",
            "crates/clearra-scoring/src/import/score_profile_import.rs",
            "crates/clearra-scoring/src/export/score_profile_export.rs",
            "crates/clearra-validation/src/validators/score_profile_object_validator.rs",
            "crates/clearra-validation/src/validators/score_profile_object_policy_validator.rs",
            "crates/clearra-validation/src/validators/score_profile_object_registry_validator.rs",
            "crates/clearra-validation/src/validators/score_profile_object_validator_tests.rs",
            "crates/clearra-output/src/scoring/score_profile_output_contract.rs",
            "crates/clearra-output/src/json/pc_json_contract.rs",
            "crates/clearra-ui-schema/src/score_editor/score_profile_editor_fields.rs",
            "scripts/score-profile-object-check.ps1"
        )) {
        if (-not (Test-Path -LiteralPath $requiredFile)) {
            Add-ArchitectureError "X2 required score profile object model file missing: $requiredFile"
        }
    }
$surface = @(
        Read-Text "crates/clearra-scoring/src/profile/score_profile.rs"
        Read-Text "crates/clearra-scoring/src/profile/score_profile_registry.rs"
        Read-Text "crates/clearra-scoring/src/model/score_model_registry.rs"
        Read-Text "crates/clearra-scoring/src/model/attack_model_registry.rs"
        Read-Text "crates/clearra-scoring/src/spin/spin_classifier_registry.rs"
        Read-Text "crates/clearra-scoring/src/profile/drop_score_policy_registry.rs"
        Read-Text "crates/clearra-scoring/src/profile/level_policy_registry.rs"
        Read-Text "crates/clearra-scoring/src/profile/level_policy.rs"
        Read-Text "crates/clearra-scoring/src/profile/pc_bonus_policy.rs"
        Read-Text "crates/clearra-scoring/src/profile/trace_requirement.rs"
        Read-Text "crates/clearra-scoring/src/import/score_profile_import.rs"
        Read-Text "crates/clearra-scoring/src/export/score_profile_export.rs"
        Read-Text "crates/clearra-validation/src/validators/score_profile_object_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/score_profile_object_policy_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/score_profile_object_registry_validator.rs"
        Read-Text "crates/clearra-validation/src/validators/score_profile_object_validator_tests.rs"
        Read-Text "crates/clearra-output/src/scoring/score_profile_output_contract.rs"
        Read-Text "crates/clearra-output/src/json/pc_json_contract.rs"
        Read-Text "crates/clearra-ui-schema/src/score_editor/score_profile_editor_fields.rs"
        Read-Text "scripts/score-profile-object-check.ps1"
    ) -join "`n"
foreach ($requiredMarker in @(
            "ScoreProfile",
            "score_model_id",
            "attack_model_id",
            "spin_rule_id",
            "spin_award_policy",
            "drop_score_policy",
            "level_policy",
            "combo_policy",
            "b2b_policy",
            "pc_bonus_policy",
            "accuracy_level",
            "accuracy_reason",
            "trace_requirement",
            "ScoreModelRegistry",
            "AttackModelRegistry",
            "SpinClassifierRegistry",
            "DropScorePolicyRegistry",
            "LevelPolicyRegistry",
            "ScoringAccuracyLevel::BasicApproximation",
            "ScoringAccuracyLevel::ProfileSpecificExact",
            "ScoringAccuracyLevel::Unsupported",
            "ScoringAccuracyLevel::InsufficientTrace",
            "exact_score_table_pinned",
            "exact_spin_classifier_available",
            "drop_score_basis_sufficient",
            "profile_specific_fixtures_pass",
            "profile_specific_exact_requires_exact_basis",
            "score_profile_object_validator_rejects_unknown_score_model",
            "score_profile_object_validator_rejects_exact_profile_with_basic_evaluator",
            "score_profile_object_validator_requires_trace_completeness_for_drop_score",
            "tetrio_profile_reports_basic_approximation_until_exact",
            "all_spin_policy_not_enabled_in_default_profile",
            "UnknownScoringField",
            "UnsupportedPolicySetting",
            "ScoreProfileOutputContract",
            "compile-and-architecture-only",
            "test_executable_launched=false"
        )) {
        if ($surface -notlike "*$requiredMarker*") {
            Add-ArchitectureError "X2 score profile object model must expose marker '$requiredMarker'"
        }
    }
$scoreModelEvaluator = Read-Text "crates/clearra-scoring/src/model/score_model_evaluator.rs"
foreach ($forbiddenMarker in @(
            'profile.id() == "tetrio"',
            'profile.id() == "jstris-ultra"',
            'profile.id() == "ppt"',
            'match profile.id()'
        )) {
        if ($scoreModelEvaluator -like "*$forbiddenMarker*") {
            Add-ArchitectureError "X2 ScoreModelEvaluator must not branch on score profile id string marker '$forbiddenMarker'; use ScoreModelId objects"
        }
    }
$policyValidator = Read-Text "crates/clearra-validation/src/validators/score_profile_object_policy_validator.rs"
if ($policyValidator -notlike "*all_spin_requires_all_piece_classifier*") {
        Add-ArchitectureError "X2 all-spin exactness must not reuse T-spin-only classifier without an all-piece classifier"
    }
}
