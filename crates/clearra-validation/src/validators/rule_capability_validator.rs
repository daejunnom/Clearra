use clearra_rules::profile::{
    rule_capability::RuleCapability,
    rule_profile::{RuleProfile, RuleProfileId},
};

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::rule_diagnostic_builder::{supported_rule_diagnostic, unsupported_rule_diagnostic};

pub(super) fn validate_rule_capability(rule: RuleProfile, report: &mut DiagnosticReport) {
    let capability = RuleCapability::from_rule(rule);
    if capability.two_line_supported()
        && rule.id() != RuleProfileId::Custom
        && capability.search_backend_supported()
    {
        report.push(supported_rule_diagnostic(rule, &capability));
    } else {
        report.push(unsupported_rule_diagnostic(rule, &capability));
    }
}
