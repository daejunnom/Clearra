use super::*;

#[test]
fn custom_rule_editor_schema_exposes_raw_validate_verify_capability_pipeline() {
    let schema = CustomRuleEditorSchema::mvp3_guarded();

    assert!(!schema.enabled());
    assert!(!schema.search_input_allowed());
    assert_eq!(
        schema.disabled_reason().map(|reason| reason.code()),
        Some(DiagnosticCode::ERuleUnsupportedMvp)
    );
    assert_eq!(
        schema.raw_editor_schema_type(),
        "clearra-rules::CustomRuleEditorSchema"
    );
    assert_eq!(
        schema.validation_adapter(),
        "clearra-validation::RuleEditorValidator::validate_custom_rule_editor_schema"
    );
    assert_eq!(
        schema.verified_profile_type(),
        "clearra-rules::VerifiedCustomRuleProfile"
    );
    assert_eq!(
        schema.search_capability_report_type(),
        "clearra-rules::CustomRuleSearchCapabilityReport"
    );
}

#[test]
fn custom_rule_editor_sections_cover_rotation_spawn_kicks_reachability_and_line_clear() {
    let schema = CustomRuleEditorSchema::mvp3_guarded();
    let section_ids = schema
        .sections()
        .iter()
        .map(CustomRuleEditorSectionSchema::id)
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
    assert!(schema
        .sections()
        .iter()
        .all(CustomRuleEditorSectionSchema::requires_validation));
}
