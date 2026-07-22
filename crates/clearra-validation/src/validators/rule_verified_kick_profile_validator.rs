use clearra_rules::{
    kicks::VerifiedKickTableProfile,
    profile::{
        rule_capability::RuleCapability,
        rule_profile::{RuleProfile, RuleProfileId},
    },
};

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::rule_diagnostic_builder::{
    spawn_aware_verified_profile_unsupported_diagnostic,
    verified_profile_missing_required_180_diagnostic, verified_profile_rule_mismatch_diagnostic,
    verified_profile_supported_diagnostic,
};

pub(super) fn validate_verified_kick_profile_rule_contract(
    rule: RuleProfile,
    verified_kick_profile: &VerifiedKickTableProfile,
    report: &mut DiagnosticReport,
) {
    let profile = verified_kick_profile.profile();
    if profile.source_rule() != rule.id() {
        report.push(verified_profile_rule_mismatch_diagnostic(rule, profile));
        return;
    }

    if matches!(
        profile.source_rule(),
        RuleProfileId::Asc | RuleProfileId::Ars
    ) {
        report.push(spawn_aware_verified_profile_unsupported_diagnostic(
            rule, profile,
        ));
        return;
    }

    if RuleCapability::from_rule(rule).supports_180() && !profile.supports_180() {
        report.push(verified_profile_missing_required_180_diagnostic(
            rule, profile,
        ));
        return;
    }

    report.push(verified_profile_supported_diagnostic(rule, profile));
}
