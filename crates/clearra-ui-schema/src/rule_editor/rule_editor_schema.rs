use clearra_rules::{
    kicks::{KickProfileDescriptor, KickProfileRegistry},
    profile::builtin_rules::custom_rule,
};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::dropdown::dropdown_option::DropdownOption;

use super::{
    custom_rule_editor_schema::CustomRuleEditorSchema,
    kick_table_editor_schema::KickTableEditorSchema,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuleEditorSchema {
    presets: Vec<DropdownOption>,
    kick_table: KickTableEditorSchema,
    custom_rule_editor: CustomRuleEditorSchema,
    capability_result_fields: Vec<String>,
    unsupported_reason_field: String,
}

impl RuleEditorSchema {
    pub fn mvp() -> Self {
        Self::mvp2()
    }
}
impl RuleEditorSchema {
    pub fn mvp2() -> Self {
        let mut presets = KickProfileRegistry::builtin_profiles()
            .into_iter()
            .map(rule_option_from_kick_profile)
            .collect::<Vec<_>>();
        presets.push(
            DropdownOption::new(custom_rule().id().as_str(), "Custom").disabled_for(
                DiagnosticCode::ERuleUnsupportedMvp,
                "Custom rule profile editing is outside MVP2.",
            ),
        );

        Self {
            presets,
            kick_table: KickTableEditorSchema::mvp2(),
            custom_rule_editor: CustomRuleEditorSchema::mvp3_guarded(),
            capability_result_fields: rule_capability_result_fields(),
            unsupported_reason_field: "search_unsupported_reason".to_owned(),
        }
    }
}
impl RuleEditorSchema {
    pub fn presets(&self) -> &[DropdownOption] {
        &self.presets
    }
}
impl RuleEditorSchema {
    pub fn kick_table(&self) -> &KickTableEditorSchema {
        &self.kick_table
    }
}
impl RuleEditorSchema {
    pub fn custom_rule_editor(&self) -> &CustomRuleEditorSchema {
        &self.custom_rule_editor
    }
}
impl RuleEditorSchema {
    pub fn capability_result_fields(&self) -> &[String] {
        &self.capability_result_fields
    }
}
impl RuleEditorSchema {
    pub fn unsupported_reason_field(&self) -> &str {
        &self.unsupported_reason_field
    }
}

impl Default for RuleEditorSchema {
    fn default() -> Self {
        Self::mvp2()
    }
}

fn rule_option_from_kick_profile(descriptor: KickProfileDescriptor) -> DropdownOption {
    let option = DropdownOption::new(descriptor.rule_profile_id().as_str(), descriptor.label());
    match descriptor.capability().unsupported_reason() {
        Some(reason) => option.disabled_for(DiagnosticCode::ERuleUnsupportedMvp, reason),
        None => option,
    }
}

fn rule_capability_result_fields() -> Vec<String> {
    [
        "rule_profile",
        "effective_kick_model",
        "verified_kick_profile",
        "supports_exact_180",
        "search_backend_supported",
        "c_compact_descriptor_ready",
        "search_unsupported_reason",
        "unsupported_backend_reason",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

#[cfg(test)]
#[path = "rule_editor_schema_tests.rs"]
mod tests;
