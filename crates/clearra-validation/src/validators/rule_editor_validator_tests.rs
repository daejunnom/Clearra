use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_rules::{
    custom_rule::{
        CustomRuleBoardBackend, CustomRuleEditorSchema, CustomRuleRuntimeFeature,
        CustomRuleSpawnRule, LockReachabilityPolicy,
    },
    kicks::{KickTableEntry, KickTableProfile, KickTableProfileId, KickTransition, SrsKicks},
    line_clear::LineClearPolicy,
    profile::rule_profile::RuleProfileId,
    spawn::SpawnProfile,
};

use crate::diagnostic::diagnostic_code::DiagnosticCode;

use super::*;

#[test]
fn custom_rule_editor_schema_validates() {
    let result = RuleEditorValidator::validate_custom_rule_editor_schema(valid_schema());

    assert!(!result.report().has_errors());
    assert!(result
        .report()
        .contains_code(DiagnosticCode::ICustomRuleVerified));
    assert!(result.verified_profile().is_some());
}

#[test]
fn custom_rule_verify_reports_missing_transition() {
    let schema = CustomRuleEditorSchema::new(
        "custom:missing",
        "Missing",
        vec![0, 1, 2, 3],
        standard_spawn_rules(),
        KickTableProfile::new(
            KickTableProfileId::Custom,
            RuleProfileId::Custom,
            Vec::new(),
        ),
        Vec::new(),
        true,
        Vec::new(),
        LineClearPolicy::ClearFullRows,
        LockReachabilityPolicy::LockReachability,
        vec![CustomRuleBoardBackend::Board64],
        supported_runtime_features(),
    );

    let report = RuleEditorValidator::verify_custom_rule_editor_schema(&schema);

    assert!(report.missing_transition() > 0);
    assert!(!report.is_verified());
}

#[test]
fn custom_rule_verify_reports_duplicate_transition() {
    let mut first_success_order = first_success_order();
    first_success_order.push(first_success_order[0]);
    let schema = CustomRuleEditorSchema::new(
        "custom:duplicate",
        "Duplicate",
        vec![0, 1, 2, 3],
        standard_spawn_rules(),
        complete_custom_kick_profile(),
        first_success_order,
        true,
        Vec::new(),
        LineClearPolicy::ClearFullRows,
        LockReachabilityPolicy::LockReachability,
        vec![CustomRuleBoardBackend::Board64],
        supported_runtime_features(),
    );

    let report = RuleEditorValidator::verify_custom_rule_editor_schema(&schema);

    assert!(report.duplicate_transition() > 0);
    assert!(!report.is_verified());
}

#[test]
fn unverified_custom_rule_rejected_before_execution() {
    let schema = CustomRuleEditorSchema::new(
        "custom:unverified",
        "Unverified",
        vec![0, 1, 2, 3],
        standard_spawn_rules(),
        KickTableProfile::new(
            KickTableProfileId::Custom,
            RuleProfileId::Custom,
            Vec::new(),
        ),
        Vec::new(),
        true,
        Vec::new(),
        LineClearPolicy::ClearFullRows,
        LockReachabilityPolicy::LockReachability,
        vec![CustomRuleBoardBackend::Board64],
        supported_runtime_features(),
    );

    let result = RuleEditorValidator::validate_custom_rule_editor_schema(schema);

    assert!(result.report().has_errors());
    assert!(result
        .report()
        .contains_code(DiagnosticCode::ECustomRuleInvalid));
    assert!(result.verified_profile().is_none());
}

fn valid_schema() -> CustomRuleEditorSchema {
    CustomRuleEditorSchema::new(
        "custom:valid",
        "Valid",
        vec![0, 1, 2, 3],
        standard_spawn_rules(),
        complete_custom_kick_profile(),
        first_success_order(),
        true,
        Vec::new(),
        LineClearPolicy::ClearFullRows,
        LockReachabilityPolicy::LockReachability,
        vec![CustomRuleBoardBackend::Board64],
        supported_runtime_features(),
    )
}

fn complete_custom_kick_profile() -> KickTableProfile {
    KickTableProfile::new(
        KickTableProfileId::Custom,
        RuleProfileId::Custom,
        SrsKicks::srs_plus_profile().entries().to_vec(),
    )
}

fn first_success_order() -> Vec<KickTransition> {
    complete_custom_kick_profile()
        .entries()
        .iter()
        .map(KickTableEntry::transition)
        .collect()
}

fn standard_spawn_rules() -> Vec<CustomRuleSpawnRule> {
    PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .map(|piece| CustomRuleSpawnRule::for_piece(piece, SpawnProfile::STANDARD_10))
        .collect()
}

fn supported_runtime_features() -> Vec<CustomRuleRuntimeFeature> {
    vec![
        CustomRuleRuntimeFeature::CompactCDescriptor,
        CustomRuleRuntimeFeature::StandardTetrominoPieces,
        CustomRuleRuntimeFeature::Board64Search,
    ]
}
