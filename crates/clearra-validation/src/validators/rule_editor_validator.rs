use clearra_rules::custom_rule::{
    CustomRuleEditorSchema, CustomRuleVerificationReport, VerifiedCustomRuleProfile,
};

use super::custom_rule_validator::{CustomRuleValidationResult, CustomRuleValidator};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuleEditorValidator;

impl RuleEditorValidator {
    pub fn validate_custom_rule_editor_schema(
        schema: CustomRuleEditorSchema,
    ) -> CustomRuleValidationResult {
        CustomRuleValidator::validate_editor_schema(schema)
    }
}
impl RuleEditorValidator {
    pub fn verify_custom_rule_editor_schema(
        schema: &CustomRuleEditorSchema,
    ) -> CustomRuleVerificationReport {
        CustomRuleVerificationReport::verify_editor_schema(schema)
    }
}
impl RuleEditorValidator {
    pub fn verified_profile(
        schema: CustomRuleEditorSchema,
    ) -> Result<VerifiedCustomRuleProfile, CustomRuleVerificationReport> {
        VerifiedCustomRuleProfile::try_from_editor_schema(schema)
    }
}

#[cfg(test)]
#[path = "rule_editor_validator_tests.rs"]
mod tests;
