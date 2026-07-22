use clearra_rules::{kicks::VerifiedKickTableProfile, profile::rule_profile::RuleProfile};

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::{
    rule_capability_validator::validate_rule_capability,
    rule_verified_kick_profile_validator::validate_verified_kick_profile_rule_contract,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuleValidator;

impl RuleValidator {
    pub fn validate_rule_profile(rule: RuleProfile) -> DiagnosticReport {
        Self::validate_rule_profile_with_verified_kick_profile(rule, None)
    }
}
impl RuleValidator {
    pub fn validate_rule_profile_with_verified_kick_profile(
        rule: RuleProfile,
        verified_kick_profile: Option<&VerifiedKickTableProfile>,
    ) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();
        if let Some(profile) = verified_kick_profile {
            validate_verified_kick_profile_rule_contract(rule, profile, &mut report);
        } else {
            validate_rule_capability(rule, &mut report);
        }
        report
    }
}

pub fn validate_rule_profile(rule: RuleProfile) -> DiagnosticReport {
    RuleValidator::validate_rule_profile(rule)
}

pub fn validate_rule_profile_with_verified_kick_profile(
    rule: RuleProfile,
    verified_kick_profile: Option<&VerifiedKickTableProfile>,
) -> DiagnosticReport {
    RuleValidator::validate_rule_profile_with_verified_kick_profile(rule, verified_kick_profile)
}

#[cfg(test)]
#[path = "rule_validator_tests.rs"]
mod tests;
