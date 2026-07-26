use clearra_rules::profile::{
    rule_capability::RuleCapability,
    rule_profile::{RuleProfile, RuleProfileId},
};

use crate::problem::{CRuleProfileDescriptor, FfiProblemError, C_KICK_JSTRIS_180};

use super::{
    no_kick_descriptor_compiler::no_kick_profile_id, srs_descriptor_compiler::srs_kick_profile_id,
    srs_plus_descriptor_compiler::srs_plus_kick_profile_id,
};

pub(crate) fn compile_builtin_profile(
    mut descriptor: CRuleProfileDescriptor,
    rule: RuleProfile,
) -> Result<CRuleProfileDescriptor, FfiProblemError> {
    let capability = RuleCapability::from_rule(rule);
    if !capability.search_backend_supported() {
        return Err(FfiProblemError::UnverifiedRuleProfileRejected {
            rule_profile_id: descriptor.rule_profile_id,
        });
    }

    descriptor.kick_profile_id = match rule.id() {
        RuleProfileId::Srs => srs_kick_profile_id(),
        RuleProfileId::SrsPlus => srs_plus_kick_profile_id(),
        RuleProfileId::Jstris180 => C_KICK_JSTRIS_180,
        RuleProfileId::NoKick => no_kick_profile_id(),
        RuleProfileId::SrsX | RuleProfileId::Asc | RuleProfileId::Ars | RuleProfileId::Custom => {
            return Err(FfiProblemError::UnverifiedRuleProfileRejected {
                rule_profile_id: descriptor.rule_profile_id,
            });
        }
    };
    descriptor.has_verified_kick_profile = 0;
    Ok(descriptor)
}
