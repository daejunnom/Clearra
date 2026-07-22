use clearra_rules::{kicks::KickImport, profile::rule_profile::RuleProfileId};

use super::*;

#[test]
fn parses_srs_90_alias_and_verified_kick_profile() {
    let rule = RuleProfileAssembler::parse_rule("srs-90").expect("rule");
    let import_json =
        KickImport::to_json(&clearra_rules::kicks::NoKick::profile()).expect("no-kick json");
    let profile = RuleProfileAssembler::parse_verified_kick_profile(Some(&import_json))
        .expect("verified profile");

    assert_eq!(rule.id(), RuleProfileId::Srs);
    assert!(profile.is_some());
}

#[test]
fn rejects_unverified_kick_profile_override_before_search_query_runs() {
    let incomplete = r#"{"id":"imported","source_rule":"custom","entries":[]}"#;

    assert!(matches!(
        RuleProfileAssembler::parse_verified_kick_profile(Some(incomplete)),
        Err(RuleProfileAssemblyError::UnverifiedKickProfile { .. })
    ));
}
