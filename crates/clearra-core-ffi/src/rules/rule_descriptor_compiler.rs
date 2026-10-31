use clearra_problem::SearchProblem;
use clearra_rules::{
    kicks::{SrsKicks, VerifiedKickTableProfile},
    profile::rule_profile::RuleProfileId,
};

use crate::problem::{CRuleProfileDescriptor, FfiProblemError};

use super::{
    imported_kick_descriptor_compiler::compile_verified_profile,
    kick_table_identity_mapper::{
        bag_profile_code, piece_set_profile_code, rule_profile_code, spawn_profile_code,
    },
    rule_capability_descriptor::compile_builtin_profile,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuleDescriptorCompiler;

impl RuleDescriptorCompiler {
    pub fn compile(problem: &SearchProblem) -> Result<CRuleProfileDescriptor, FfiProblemError> {
        let rule = problem.rule_profile().rule();
        let rule_profile_id = rule_profile_code(rule.id());
        let base = CRuleProfileDescriptor {
            piece_set_profile_id: piece_set_profile_code(problem.piece_set().id().as_str()),
            bag_profile_id: bag_profile_code(problem.supply().bag().id().as_str()),
            rule_profile_id,
            kick_profile_id: 0,
            spawn_profile_id: spawn_profile_code(
                problem.rule_profile().spawn_profile().id().as_str(),
            ),
            ..CRuleProfileDescriptor::default()
        };

        if let Some(profile) = problem.rule_profile().verified_kick_profile() {
            return compile_verified_profile(base, rule, profile);
        }

        // TETR.IO profiles use the canonical Rust table through the same
        // verified-profile ABI as imported profiles. SRS+ must not silently
        // select a separately maintained C table during native execution.
        let canonical = match rule.id() {
            RuleProfileId::SrsPlus => Some(SrsKicks::srs_plus_profile()),
            RuleProfileId::SrsX => Some(SrsKicks::srs_x_profile()),
            _ => None,
        };
        if let Some(canonical) = canonical {
            let profile = VerifiedKickTableProfile::try_new(canonical)
                .map_err(|_| FfiProblemError::UnverifiedRuleProfileRejected { rule_profile_id })?;
            return compile_verified_profile(base, rule, &profile);
        }

        compile_builtin_profile(base, rule)
    }
}

#[cfg(test)]
#[path = "rule_descriptor_compiler_tests.rs"]
mod tests;
