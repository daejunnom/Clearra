use clearra_rules::{
    kicks::{NoKick, SrsKicks, VerifiedKickTableProfile},
    profile::{
        builtin_rules::{ars, asc, custom_rule, srs, srs_plus, srs_x},
        rule_profile::{RuleProfile, RuleProfileId},
    },
};

use crate::diagnostic::diagnostic_code::DiagnosticCode;

use super::{validate_rule_profile, validate_rule_profile_with_verified_kick_profile};

#[test]
fn builtin_rule_is_supported() {
    let report = validate_rule_profile(srs());

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IRuleMvpSupported));
}

#[test]
fn custom_rule_is_rejected_for_mvp() {
    let report = validate_rule_profile(custom_rule());

    assert!(report.has_errors());
    assert!(report.contains_code(DiagnosticCode::ERuleUnsupportedMvp));
}

#[test]
fn srs_plus_reports_180_capable_supported_rule() {
    let report = validate_rule_profile(srs_plus());

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IRuleMvpSupported));
    assert!(!report.contains_code(DiagnosticCode::WRuleSrsPlusExtensionsDisabled));
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "supports_180" && evidence.value() == "true")));
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic
            .evidence()
            .iter()
            .any(|evidence| evidence.key() == "supports_exact_180" && evidence.value() == "true")
    }));
}

#[test]
fn srs_x_builtin_profile_is_supported() {
    let report = validate_rule_profile(srs_x());

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IRuleMvpSupported));
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "supports_exact_180" && evidence.value() == "true")));
}

#[test]
fn srs_x_is_supported_when_a_verified_kick_profile_override_is_supplied() {
    let verified = VerifiedKickTableProfile::try_new(clearra_rules::kicks::KickTableProfile::new(
        clearra_rules::kicks::KickTableProfileId::Imported,
        RuleProfileId::SrsX,
        SrsKicks::srs_plus_profile().entries().to_vec(),
    ))
    .expect("verified");
    let report = validate_rule_profile_with_verified_kick_profile(
        RuleProfile::new(RuleProfileId::SrsX),
        Some(&verified),
    );

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IRuleMvpSupported));
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "verified_profile" && evidence.value() == "true")));
}

#[test]
fn srs_x_verified_profile_requires_exact_180_transition_set() {
    let verified = VerifiedKickTableProfile::try_new(clearra_rules::kicks::KickTableProfile::new(
        clearra_rules::kicks::KickTableProfileId::Imported,
        RuleProfileId::SrsX,
        SrsKicks::profile().entries().to_vec(),
    ))
    .expect("verified without 180");
    let report = validate_rule_profile_with_verified_kick_profile(
        RuleProfile::new(RuleProfileId::SrsX),
        Some(&verified),
    );

    assert!(report.has_errors());
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "reason"
            && evidence.value() == "verified_profile_missing_required_180")));
}

#[test]
fn asc_profile_validates_as_guarded_descriptor() {
    let report = validate_rule_profile(asc());

    assert!(report.has_errors());
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "reason"
            && evidence.value() == "asc_profile_requires_spawn_reachability")));
}

#[test]
fn ars_profile_validates_as_guarded_descriptor() {
    let report = validate_rule_profile(ars());

    assert!(report.has_errors());
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "reason"
            && evidence.value() == "ars_profile_requires_spawn_reachability")));
}

#[test]
fn verified_profile_source_rule_must_match_query_rule() {
    let verified = VerifiedKickTableProfile::try_new(NoKick::profile()).expect("verified");
    let report = validate_rule_profile_with_verified_kick_profile(srs(), Some(&verified));

    assert!(report.has_errors());
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "reason"
            && evidence.value() == "verified_profile_rule_mismatch")));
}

#[test]
fn verified_spawn_aware_profile_is_still_rejected_until_backend_support_exists() {
    let verified = VerifiedKickTableProfile::try_new(clearra_rules::kicks::KickTableProfile::new(
        clearra_rules::kicks::KickTableProfileId::Imported,
        RuleProfileId::Ars,
        SrsKicks::profile().entries().to_vec(),
    ))
    .expect("verified");
    let report = validate_rule_profile_with_verified_kick_profile(
        RuleProfile::new(RuleProfileId::Ars),
        Some(&verified),
    );

    assert!(report.has_errors());
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "reason"
            && evidence.value() == "spawn_aware_profile_unsupported")));
}
