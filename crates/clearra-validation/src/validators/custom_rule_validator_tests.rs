use clearra_rules::{
    custom_rule::{CustomRuleEditorDraft, LockReachabilityPolicy},
    kicks::{KickTableProfile, KickTableProfileId, SrsKicks},
    line_clear::LineClearPolicy,
    profile::rule_profile::RuleProfileId,
    rotation::RotationSystem,
    spawn::SpawnProfile,
};

use crate::diagnostic::diagnostic_code::DiagnosticCode;

use super::*;

#[test]
fn custom_rule_editor_validation_returns_verified_profile_and_capability_report() {
    let result = validate_custom_rule_editor_draft(valid_custom_rule_draft());

    assert!(!result.report().has_errors());
    assert!(result
        .report()
        .contains_code(DiagnosticCode::ICustomRuleVerified));
    assert!(result.verified_profile().is_some());
    let capability = result.search_capability_report().expect("capability");
    assert!(!capability.search_backend_supported());
    assert_eq!(
        capability.unsupported_reason(),
        Some("custom_rule_search_backend_not_connected")
    );
}

#[test]
fn raw_custom_rule_editor_schema_cannot_skip_validation_into_search() {
    let invalid_kick_table = KickTableProfile::new(
        KickTableProfileId::Custom,
        RuleProfileId::Custom,
        Vec::new(),
    );
    let invalid = CustomRuleEditorDraft::new(
        "custom:bad",
        "Bad",
        invalid_kick_table,
        SpawnProfile::STANDARD_10,
        RotationSystem::Srs,
        LockReachabilityPolicy::LockReachability,
        LineClearPolicy::ClearFullRows,
    );

    let result = validate_custom_rule_editor_draft(invalid);

    assert!(result.report().has_errors());
    assert!(result
        .report()
        .contains_code(DiagnosticCode::ECustomRuleInvalid));
    assert!(result.verified_profile().is_none());
    assert!(result.search_capability_report().is_none());
}

fn valid_custom_rule_draft() -> CustomRuleEditorDraft {
    let kick_table = KickTableProfile::new(
        KickTableProfileId::Custom,
        RuleProfileId::Custom,
        SrsKicks::srs_plus_profile().entries().to_vec(),
    );
    CustomRuleEditorDraft::new(
        "custom:test-rule",
        "Test Rule",
        kick_table,
        SpawnProfile::STANDARD_10,
        RotationSystem::Srs,
        LockReachabilityPolicy::LockReachability,
        LineClearPolicy::ClearFullRows,
    )
}
