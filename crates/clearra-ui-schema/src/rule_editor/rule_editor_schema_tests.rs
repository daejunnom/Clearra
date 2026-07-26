use clearra_rules::{
    kicks::KickTableProfileId,
    profile::{builtin_rules::custom_rule, rule_profile::RuleProfileId},
};

use super::*;

#[test]
fn rule_presets_use_canonical_rule_ids() {
    let schema = RuleEditorSchema::mvp2();

    assert!(schema
        .presets()
        .iter()
        .any(|option| option.value() == RuleProfileId::Srs.as_str()));
    assert!(schema
        .presets()
        .iter()
        .any(|option| option.value() == RuleProfileId::Jstris180.as_str()));
    assert_eq!(
        schema.presets().last().map(DropdownOption::value),
        Some(custom_rule().id().as_str())
    );
}

#[test]
fn disabled_rule_editor_features_expose_diagnostic_codes_for_unsupported_profiles() {
    let schema = RuleEditorSchema::mvp2();
    let srs_x = schema
        .presets()
        .iter()
        .find(|option| option.value() == RuleProfileId::SrsX.as_str())
        .expect("srs-x option");
    let custom = schema.presets().last().expect("custom option");

    assert_eq!(
        srs_x.disabled_reason().map(|reason| reason.code()),
        Some(DiagnosticCode::ERuleUnsupportedMvp)
    );
    assert_eq!(
        custom.disabled_reason().map(|reason| reason.code()),
        Some(DiagnosticCode::ERuleUnsupportedMvp)
    );
    assert!(schema.kick_table().editable());
    assert!(schema
        .kick_table()
        .previews()
        .iter()
        .any(|preview| preview.profile_id() == KickTableProfileId::Srs90.as_str()));
    assert!(!schema.custom_rule_editor().enabled());
    assert_eq!(
        schema
            .custom_rule_editor()
            .disabled_reason()
            .map(|reason| reason.code()),
        Some(DiagnosticCode::ERuleUnsupportedMvp)
    );
    assert_eq!(
        schema.unsupported_reason_field(),
        "search_unsupported_reason"
    );
    assert!(schema
        .capability_result_fields()
        .iter()
        .any(|field| field == "search_backend_supported"));
    assert!(schema
        .capability_result_fields()
        .iter()
        .any(|field| field == "c_compact_descriptor_ready"));
    assert!(schema
        .capability_result_fields()
        .iter()
        .any(|field| field == "unsupported_backend_reason"));
}
