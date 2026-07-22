use crate::profile::builtin_rules::{ars, asc, custom_rule, no_kick, srs, srs_plus, srs_x};

use super::*;

#[test]
fn srs_plus_builtin_profile_supports_exact_180() {
    let capability = RuleCapability::from_rule(srs_plus());

    assert_eq!(capability.kick_model(), RuleKickModel::SrsPlus180);
    assert!(capability.supports_180());
    assert!(capability.supports_exact_180());
    assert!(capability.search_backend_supported());
    assert!(!capability.srs_plus_extensions_disabled());
}

#[test]
fn srs_and_no_kick_expose_distinct_effective_kick_models() {
    assert_eq!(
        RuleCapability::from_rule(srs()).kick_model(),
        RuleKickModel::Srs90
    );
    assert_eq!(
        RuleCapability::from_rule(no_kick()).kick_model(),
        RuleKickModel::NoKick
    );
    assert_eq!(
        RuleCapability::from_rule(custom_rule()).kick_model(),
        RuleKickModel::UnsupportedCustom
    );
}

#[test]
fn mvp2_extension_profiles_report_capability_and_backend_limits() {
    let srs_x_capability = RuleCapability::from_rule(srs_x());
    let asc_capability = RuleCapability::from_rule(asc());
    let ars_capability = RuleCapability::from_rule(ars());

    assert!(srs_x_capability.supports_180());
    assert!(srs_x_capability.supports_exact_180());
    assert!(srs_x_capability.search_backend_supported());
    assert!(srs_x_capability.unsupported_reason().is_none());
    assert!(asc_capability.requires_spawn_reachability());
    assert_eq!(
        asc_capability.unsupported_reason(),
        Some("asc_profile_requires_spawn_reachability")
    );
    assert!(!ars_capability.supports_180());
    assert_eq!(
        ars_capability.unsupported_reason(),
        Some("ars_profile_requires_spawn_reachability")
    );
}
