use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::disabled_reason::UiDisabledReason;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomRuleEditorSchema {
    enabled: bool,
    disabled_reason: Option<UiDisabledReason>,
    raw_editor_schema_type: &'static str,
    validation_adapter: &'static str,
    verified_profile_type: &'static str,
    search_capability_report_type: &'static str,
    search_input_allowed: bool,
    sections: Vec<CustomRuleEditorSectionSchema>,
}

impl CustomRuleEditorSchema {
    pub fn mvp3_guarded() -> Self {
        Self {
            enabled: false,
            disabled_reason: Some(UiDisabledReason::new(
                DiagnosticCode::ERuleUnsupportedMvp,
                "Full custom rule editing is MVP3; raw editor schemas must validate into VerifiedCustomRuleProfile before search capability is reported.",
            )),
            raw_editor_schema_type: "clearra-rules::CustomRuleEditorSchema",
            validation_adapter: "clearra-validation::RuleEditorValidator::validate_custom_rule_editor_schema",
            verified_profile_type: "clearra-rules::VerifiedCustomRuleProfile",
            search_capability_report_type: "clearra-rules::CustomRuleSearchCapabilityReport",
            search_input_allowed: false,
            sections: vec![
                CustomRuleEditorSectionSchema::new(
                    "rotation-states",
                    "Rotation states",
                    "clearra-rules::CustomRuleEditorSchema::rotation_states",
                    true,
                ),
                CustomRuleEditorSectionSchema::new(
                    "spawn-rules",
                    "Spawn rules",
                    "clearra-rules::CustomRuleEditorSchema::spawn_rules",
                    true,
                ),
                CustomRuleEditorSectionSchema::new(
                    "kick-transitions",
                    "Kick transitions",
                    "clearra-rules::CustomRuleEditorSchema::kick_transitions",
                    true,
                ),
                CustomRuleEditorSectionSchema::new(
                    "first-success-order",
                    "First-success order",
                    "clearra-rules::CustomRuleEditorSchema::first_success_order",
                    true,
                ),
                CustomRuleEditorSectionSchema::new(
                    "180-support",
                    "180 support",
                    "clearra-rules::CustomRuleEditorSchema::supports_180",
                    true,
                ),
                CustomRuleEditorSectionSchema::new(
                    "piece-specific-overrides",
                    "Piece-specific overrides",
                    "clearra-rules::CustomRuleEditorSchema::piece_specific_overrides",
                    true,
                ),
                CustomRuleEditorSectionSchema::new(
                    "line-clear-policy",
                    "Line clear policy",
                    "clearra-rules::CustomRuleEditorSchema::line_clear_policy",
                    true,
                ),
                CustomRuleEditorSectionSchema::new(
                    "lock-reachability-mode",
                    "Lock and reachability mode",
                    "clearra-rules::CustomRuleEditorSchema::lock_reachability_mode",
                    true,
                ),
                CustomRuleEditorSectionSchema::new(
                    "verification-report",
                    "Verification report",
                    "missing_transition|duplicate_transition|invalid_rotation|unsupported_piece|unsupported_board_backend|unsupported_runtime_feature",
                    true,
                ),
            ],
        }
    }
}
impl CustomRuleEditorSchema {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
impl CustomRuleEditorSchema {
    pub fn disabled_reason(&self) -> Option<&UiDisabledReason> {
        self.disabled_reason.as_ref()
    }
}
impl CustomRuleEditorSchema {
    pub fn raw_editor_schema_type(&self) -> &'static str {
        self.raw_editor_schema_type
    }
}
impl CustomRuleEditorSchema {
    pub fn validation_adapter(&self) -> &'static str {
        self.validation_adapter
    }
}
impl CustomRuleEditorSchema {
    pub fn verified_profile_type(&self) -> &'static str {
        self.verified_profile_type
    }
}
impl CustomRuleEditorSchema {
    pub fn search_capability_report_type(&self) -> &'static str {
        self.search_capability_report_type
    }
}
impl CustomRuleEditorSchema {
    pub fn search_input_allowed(&self) -> bool {
        self.search_input_allowed
    }
}
impl CustomRuleEditorSchema {
    pub fn sections(&self) -> &[CustomRuleEditorSectionSchema] {
        &self.sections
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomRuleEditorSectionSchema {
    id: &'static str,
    label: &'static str,
    adapter_type: &'static str,
    requires_validation: bool,
}

impl CustomRuleEditorSectionSchema {
    pub fn new(
        id: &'static str,
        label: &'static str,
        adapter_type: &'static str,
        requires_validation: bool,
    ) -> Self {
        Self {
            id,
            label,
            adapter_type,
            requires_validation,
        }
    }
}
impl CustomRuleEditorSectionSchema {
    pub fn id(&self) -> &'static str {
        self.id
    }
}
impl CustomRuleEditorSectionSchema {
    pub fn label(&self) -> &'static str {
        self.label
    }
}
impl CustomRuleEditorSectionSchema {
    pub fn adapter_type(&self) -> &'static str {
        self.adapter_type
    }
}
impl CustomRuleEditorSectionSchema {
    pub fn requires_validation(&self) -> bool {
        self.requires_validation
    }
}

#[cfg(test)]
#[path = "custom_rule_editor_schema_tests.rs"]
mod tests;
