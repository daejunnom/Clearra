use clearra_profiles::bundle::standard_profile_bundle::standard_profile_bundle;
use clearra_rules::profile::{builtin_rules::custom_rule, rule_profile::RuleProfileId};

use super::*;

#[test]
fn dropdown_values_come_from_canonical_profile_and_rule_ids() {
    let profiles = standard_profile_bundle();
    let schema = ProfileDropdowns::standard_mvp();

    assert_eq!(schema.boards()[0].value(), profiles.board().id().as_str());
    assert_eq!(
        schema.piece_sets()[0].value(),
        profiles.piece_set().id().as_str()
    );
    assert_eq!(schema.bags()[0].value(), profiles.bag().id().as_str());
    assert!(schema
        .rules()
        .iter()
        .any(|option| option.value() == RuleProfileId::SrsPlus.as_str()));
    assert_eq!(
        schema.rules().last().map(DropdownOption::value),
        Some(custom_rule().id().as_str())
    );
}

#[test]
fn disabled_custom_rule_exposes_validation_code_and_reason() {
    let schema = ProfileDropdowns::standard_mvp();
    let custom = schema.rules().last().expect("custom rule option");

    assert!(custom.is_disabled());
    assert_eq!(
        custom.disabled_reason().map(|reason| reason.code()),
        Some(DiagnosticCode::ERuleUnsupportedMvp)
    );
    assert_eq!(
        custom.disabled_reason().map(|reason| reason.code_str()),
        Some("E_RULE_UNSUPPORTED_MVP")
    );
}
