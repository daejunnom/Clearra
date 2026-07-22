use clearra_rules::{
    custom_rule::{CustomRuleEditorSchema, VerifiedCustomRuleProfile},
    profile::rule_profile::{RuleProfile, RuleProfileId},
};

use crate::problem::{
    CRuleProfileDescriptor, FfiProblemError, C_BAG_STANDARD_7_BAG,
    C_PIECE_SET_STANDARD_TETROMINOES, C_RULE_CUSTOM,
};

use super::{
    imported_kick_descriptor_compiler::compile_verified_profile,
    kick_table_identity_mapper::{kick_profile_code, spawn_profile_code},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CustomRuleDescriptorCompiler;

impl CustomRuleDescriptorCompiler {
    pub fn compile_verified(
        profile: &VerifiedCustomRuleProfile,
    ) -> Result<CRuleProfileDescriptor, FfiProblemError> {
        if !profile.can_compile_to_c_descriptor() {
            return Err(FfiProblemError::CustomRuleDescriptorRuntimeNotConnected);
        }

        let base = CRuleProfileDescriptor {
            piece_set_profile_id: C_PIECE_SET_STANDARD_TETROMINOES,
            bag_profile_id: C_BAG_STANDARD_7_BAG,
            rule_profile_id: C_RULE_CUSTOM,
            kick_profile_id: kick_profile_code(profile.verified_kick_profile().profile().id()),
            spawn_profile_id: spawn_profile_code(profile.spawn_profile().id().as_str()),
            ..CRuleProfileDescriptor::default()
        };

        compile_verified_profile(
            base,
            RuleProfile::new(RuleProfileId::Custom),
            profile.verified_kick_profile(),
        )
    }
}
impl CustomRuleDescriptorCompiler {
    pub fn compile_editor_schema(
        _schema: &CustomRuleEditorSchema,
    ) -> Result<CRuleProfileDescriptor, FfiProblemError> {
        Err(FfiProblemError::UnverifiedCustomRuleRejectedBeforeExecution)
    }
}

#[cfg(test)]
#[path = "custom_rule_descriptor_compiler_tests.rs"]
mod tests;
