use clearra_rules::{
    custom_rule::{
        CustomRuleEditorDraft, CustomRuleSearchCapabilityReport, LockReachabilityPolicy,
        VerifiedCustomRuleProfile,
    },
    kicks::{KickTableProfile, KickTableProfileId, SrsKicks},
    line_clear::LineClearPolicy,
    profile::rule_profile::RuleProfileId,
    rotation::RotationSystem,
    spawn::SpawnProfile,
};
use clearra_ui_schema::RuleEditorSchema;
use clearra_validation::{
    diagnostic::diagnostic_code::DiagnosticCode,
    validators::custom_rule_validator::validate_custom_rule_editor_draft,
};

#[test]
fn custom_rule_editor_pipeline_requires_validation_before_verified_profile_and_capability() {
    let draft = custom_rule_editor_draft();

    let result = validate_custom_rule_editor_draft(draft);

    assert!(!result.report().has_errors());
    assert!(result
        .report()
        .contains_code(DiagnosticCode::ICustomRuleVerified));
    let verified = result.verified_profile().expect("verified custom rule");
    assert_eq!(verified.rule_profile().id(), RuleProfileId::Custom);
    assert_eq!(
        verified.verified_kick_profile().profile().source_rule(),
        RuleProfileId::Custom
    );

    let capability: &CustomRuleSearchCapabilityReport =
        result.search_capability_report().expect("capability");
    assert!(!capability.search_backend_supported());
    assert_eq!(
        capability.unsupported_reason(),
        Some("custom_rule_search_backend_not_connected")
    );
}

#[test]
fn raw_custom_rule_editor_draft_is_not_a_search_input_contract() {
    let incomplete = KickTableProfile::new(
        KickTableProfileId::Custom,
        RuleProfileId::Custom,
        Vec::new(),
    );
    let raw_draft = CustomRuleEditorDraft::new(
        "custom:raw",
        "Raw",
        incomplete,
        SpawnProfile::STANDARD_10,
        RotationSystem::Srs,
        LockReachabilityPolicy::LockReachability,
        LineClearPolicy::ClearFullRows,
    );

    assert!(VerifiedCustomRuleProfile::try_from_editor_draft(raw_draft.clone()).is_err());

    let result = validate_custom_rule_editor_draft(raw_draft);
    assert!(result.report().has_errors());
    assert!(result
        .report()
        .contains_code(DiagnosticCode::ECustomRuleInvalid));
    assert!(result.verified_profile().is_none());
}

#[test]
fn rule_editor_schema_exposes_full_custom_rule_sections_but_keeps_mvp3_guard() {
    let schema = RuleEditorSchema::mvp2();
    let custom = schema.custom_rule_editor();

    assert!(!custom.enabled());
    assert!(!custom.search_input_allowed());
    assert_eq!(
        custom.disabled_reason().map(|reason| reason.code()),
        Some(DiagnosticCode::ERuleUnsupportedMvp)
    );
    assert_eq!(
        custom.raw_editor_schema_type(),
        "clearra-rules::CustomRuleEditorSchema"
    );
    assert_eq!(
        custom.validation_adapter(),
        "clearra-validation::RuleEditorValidator::validate_custom_rule_editor_schema"
    );
    assert_eq!(
        custom.verified_profile_type(),
        "clearra-rules::VerifiedCustomRuleProfile"
    );

    let section_ids = custom
        .sections()
        .iter()
        .map(|section| section.id())
        .collect::<Vec<_>>();
    assert!(section_ids.contains(&"rotation-states"));
    assert!(section_ids.contains(&"spawn-rules"));
    assert!(section_ids.contains(&"kick-transitions"));
    assert!(section_ids.contains(&"first-success-order"));
    assert!(section_ids.contains(&"180-support"));
    assert!(section_ids.contains(&"piece-specific-overrides"));
    assert!(section_ids.contains(&"line-clear-policy"));
    assert!(section_ids.contains(&"lock-reachability-mode"));
    assert!(section_ids.contains(&"verification-report"));
}

fn custom_rule_editor_draft() -> CustomRuleEditorDraft {
    let kick_table = KickTableProfile::new(
        KickTableProfileId::Custom,
        RuleProfileId::Custom,
        SrsKicks::srs_plus_profile().entries().to_vec(),
    );
    CustomRuleEditorDraft::new(
        "custom:integration-rule",
        "Integration Rule",
        kick_table,
        SpawnProfile::STANDARD_10,
        RotationSystem::Srs,
        LockReachabilityPolicy::LockReachability,
        LineClearPolicy::ClearFullRows,
    )
}
