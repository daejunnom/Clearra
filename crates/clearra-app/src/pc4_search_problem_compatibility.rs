// SRP rationale: this module's single change reason is validating one PC4 rule profile against the exact compiled Clearra rule and kick identities.

use clearra_pc4_tablebase::Pc4RuleProfile;
use clearra_problem::SearchProblem;
use clearra_rules::{kicks::KickTableProfileId, profile::rule_profile::RuleProfileId};

/// A checked bridge between a dataset profile name and the rule identities
/// compiled into one Clearra search problem.
///
/// This token grants no dataset, target, candidate-completeness, or execution
/// authority. It only prevents an otherwise complete candidate universe from
/// entering a problem compiled for a different kick table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4SearchProblemCompatibility {
    profile: Pc4RuleProfile,
    rule_profile: RuleProfileId,
    kick_profile: KickTableProfileId,
}

impl Pc4SearchProblemCompatibility {
    pub const fn profile(self) -> Pc4RuleProfile {
        self.profile
    }

    pub const fn rule_profile(self) -> RuleProfileId {
        self.rule_profile
    }

    pub const fn kick_profile(self) -> KickTableProfileId {
        self.kick_profile
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4SearchProblemCompatibilityError {
    RuleProfileMismatch {
        expected: RuleProfileId,
        actual: RuleProfileId,
    },
    KickProfileMismatch {
        expected: KickTableProfileId,
        actual: KickTableProfileId,
    },
    KickSourceRuleMismatch {
        expected: RuleProfileId,
        actual: RuleProfileId,
    },
    KickProfileNotVerified,
}

impl Pc4SearchProblemCompatibilityError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::RuleProfileMismatch { .. } => "pc4_search_problem_rule_profile_mismatch",
            Self::KickProfileMismatch { .. } => "pc4_search_problem_kick_profile_mismatch",
            Self::KickSourceRuleMismatch { .. } => "pc4_search_problem_kick_source_rule_mismatch",
            Self::KickProfileNotVerified => "pc4_search_problem_kick_profile_not_verified",
        }
    }
}

/// Validates the rule layer only. Initial field, target height, queue/hold,
/// candidate identity, and snapshot freshness remain owned by their existing
/// boundaries and must be checked separately before execution.
pub fn validate_pc4_search_problem_compatibility(
    profile: Pc4RuleProfile,
    problem: &SearchProblem,
) -> Result<Pc4SearchProblemCompatibility, Pc4SearchProblemCompatibilityError> {
    let (expected_rule, expected_kick) = expected_profiles(profile);
    let actual_rule = problem.rule_profile_value().id();
    if actual_rule != expected_rule {
        return Err(Pc4SearchProblemCompatibilityError::RuleProfileMismatch {
            expected: expected_rule,
            actual: actual_rule,
        });
    }

    let actual_kick = problem.kick_profile();
    if actual_kick.profile_id() != expected_kick {
        return Err(Pc4SearchProblemCompatibilityError::KickProfileMismatch {
            expected: expected_kick,
            actual: actual_kick.profile_id(),
        });
    }
    if actual_kick.source_rule() != expected_rule {
        return Err(Pc4SearchProblemCompatibilityError::KickSourceRuleMismatch {
            expected: expected_rule,
            actual: actual_kick.source_rule(),
        });
    }
    if !actual_kick.verified() {
        return Err(Pc4SearchProblemCompatibilityError::KickProfileNotVerified);
    }

    Ok(Pc4SearchProblemCompatibility {
        profile,
        rule_profile: actual_rule,
        kick_profile: actual_kick.profile_id(),
    })
}

const fn expected_profiles(profile: Pc4RuleProfile) -> (RuleProfileId, KickTableProfileId) {
    match profile {
        Pc4RuleProfile::Srs => (RuleProfileId::Srs, KickTableProfileId::Srs90),
        Pc4RuleProfile::SrsPlus => (RuleProfileId::SrsPlus, KickTableProfileId::SrsPlus),
        Pc4RuleProfile::SrsX => (RuleProfileId::SrsX, KickTableProfileId::SrsX),
        Pc4RuleProfile::Jstris180 => (RuleProfileId::Jstris180, KickTableProfileId::Jstris180),
        Pc4RuleProfile::NoKick => (RuleProfileId::NoKick, KickTableProfileId::NoKick),
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::pc::pc_target::PcTarget;
    use clearra_pc_graph::request::OpeningPcSearchQuery;
    use clearra_problem::compile::problem_compiler::ProblemCompiler;
    use clearra_rules::profile::{
        builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x},
        rule_profile::RuleProfile,
    };

    use super::*;

    fn compile(rule: RuleProfile) -> SearchProblem {
        ProblemCompiler::compile_opening_pc(
            &OpeningPcSearchQuery::new(PcTarget::four_lines()).with_rule(rule),
        )
        .expect("compile opening problem")
    }

    #[test]
    fn all_five_profiles_match_only_their_exact_builtin_rule_and_kick_table() {
        for (profile, rule) in [
            (Pc4RuleProfile::Srs, srs()),
            (Pc4RuleProfile::SrsPlus, srs_plus()),
            (Pc4RuleProfile::SrsX, srs_x()),
            (Pc4RuleProfile::Jstris180, jstris_180()),
            (Pc4RuleProfile::NoKick, no_kick()),
        ] {
            let compatibility = validate_pc4_search_problem_compatibility(profile, &compile(rule))
                .expect("matching built-in profile");
            let (expected_rule, expected_kick) = expected_profiles(profile);
            assert_eq!(compatibility.profile(), profile);
            assert_eq!(compatibility.rule_profile(), expected_rule);
            assert_eq!(compatibility.kick_profile(), expected_kick);
        }
    }

    #[test]
    fn a_qualified_profile_never_borrows_another_profiles_problem() {
        let srs_problem = compile(srs());
        for profile in [
            Pc4RuleProfile::SrsPlus,
            Pc4RuleProfile::SrsX,
            Pc4RuleProfile::Jstris180,
            Pc4RuleProfile::NoKick,
        ] {
            assert!(matches!(
                validate_pc4_search_problem_compatibility(profile, &srs_problem),
                Err(Pc4SearchProblemCompatibilityError::RuleProfileMismatch { .. })
            ));
        }
    }
}
