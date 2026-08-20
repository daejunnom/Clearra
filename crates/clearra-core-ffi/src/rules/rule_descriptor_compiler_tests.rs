use clearra_core_domain::{pc::pc_target::PcTarget, piece::piece_kind::PieceKind};
use clearra_pc_graph::request::{OpeningPcSearchQuery, PcQueueInput};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_rules::{
    kicks::{KickTableProfile, KickTableProfileId, NoKick, SrsKicks, VerifiedKickTableProfile},
    profile::{
        builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x},
        rule_capability::RuleCapability,
        rule_profile::{RuleProfile, RuleProfileId},
    },
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::problem::{
    C_KICK_IMPORTED, C_KICK_JSTRIS_180, C_KICK_NO_KICK, C_KICK_SRS_90, C_KICK_SRS_PLUS_180,
    C_KICK_SRS_X, C_RULE_JSTRIS_180, C_RULE_NO_KICK, C_RULE_SRS, C_RULE_SRS_PLUS, C_RULE_SRS_X,
};

use super::*;

#[test]
fn srs_profile_compiles_to_c_descriptor() {
    let problem = opening_problem_with_rule(srs(), None);

    let descriptor = RuleDescriptorCompiler::compile(&problem).expect("descriptor");

    assert_eq!(descriptor.rule_profile_id, C_RULE_SRS);
    assert_eq!(descriptor.kick_profile_id, C_KICK_SRS_90);
    assert_eq!(descriptor.has_verified_kick_profile, 0);
}

#[test]
fn srs_plus_profile_compiles_to_c_descriptor() {
    let problem = opening_problem_with_rule(srs_plus(), None);

    let descriptor = RuleDescriptorCompiler::compile(&problem).expect("descriptor");

    assert_eq!(descriptor.rule_profile_id, C_RULE_SRS_PLUS);
    assert_eq!(descriptor.kick_profile_id, C_KICK_SRS_PLUS_180);
    assert_eq!(descriptor.has_verified_kick_profile, 0);
}

#[test]
fn jstris_180_profile_compiles_to_c_descriptor() {
    let problem = opening_problem_with_rule(jstris_180(), None);

    let descriptor = RuleDescriptorCompiler::compile(&problem).expect("descriptor");

    assert_eq!(descriptor.rule_profile_id, C_RULE_JSTRIS_180);
    assert_eq!(descriptor.kick_profile_id, C_KICK_JSTRIS_180);
    assert_eq!(descriptor.has_verified_kick_profile, 0);
}

#[test]
fn no_kick_profile_compiles_to_c_descriptor() {
    let problem = opening_problem_with_rule(no_kick(), None);

    let descriptor = RuleDescriptorCompiler::compile(&problem).expect("descriptor");

    assert_eq!(descriptor.rule_profile_id, C_RULE_NO_KICK);
    assert_eq!(descriptor.kick_profile_id, C_KICK_NO_KICK);
    assert_eq!(descriptor.has_verified_kick_profile, 0);
}

#[test]
fn imported_verified_kick_profile_compiles_to_c_descriptor() {
    let verified = verified_imported_srs_x_profile();
    let problem = opening_problem_with_rule(srs_x(), Some(verified));

    let descriptor = RuleDescriptorCompiler::compile(&problem).expect("descriptor");

    assert_eq!(descriptor.rule_profile_id, C_RULE_SRS_X);
    assert_eq!(descriptor.kick_profile_id, C_KICK_IMPORTED);
    assert_eq!(descriptor.has_verified_kick_profile, 1);
    assert_eq!(descriptor.verified_supports_180, 1);
    assert_eq!(descriptor.verified_transition_count, 80);
    assert_eq!(descriptor.verified_transitions[0].sequence.count, 5);
}

#[test]
fn verified_180_capable_rule_rejects_profile_without_180_transitions() {
    let verified = VerifiedKickTableProfile::try_new(KickTableProfile::new(
        KickTableProfileId::Imported,
        RuleProfileId::SrsX,
        SrsKicks::profile().entries().to_vec(),
    ))
    .expect("verified 90-only profile");
    let problem = opening_problem_with_rule(srs_x(), Some(verified));

    assert_eq!(
        RuleDescriptorCompiler::compile(&problem),
        Err(FfiProblemError::VerifiedKickProfileMissingRequired180 {
            rule_profile_id: C_RULE_SRS_X
        })
    );
}

#[test]
fn builtin_srs_x_projects_the_canonical_verified_table_to_c() {
    let problem = opening_problem_with_rule(srs_x(), None);

    let descriptor = RuleDescriptorCompiler::compile(&problem).expect("built-in SRS-X descriptor");

    assert_eq!(descriptor.rule_profile_id, C_RULE_SRS_X);
    assert_eq!(descriptor.kick_profile_id, C_KICK_SRS_X);
    assert_eq!(descriptor.has_verified_kick_profile, 1);
    assert_eq!(descriptor.verified_supports_180, 1);
    assert_eq!(descriptor.verified_transition_count, 80);
}

#[test]
fn srs_x_capability_and_verified_c_boundary_agree() {
    let rule = srs_x();
    let capability = RuleCapability::from_rule(rule);
    let problem = opening_problem_with_rule(rule, None);

    assert!(capability.search_backend_supported());
    assert_eq!(capability.unsupported_reason(), None);
    let descriptor = RuleDescriptorCompiler::compile(&problem).expect("SRS-X C descriptor");
    assert_eq!(descriptor.rule_profile_id, C_RULE_SRS_X);
    assert_eq!(descriptor.kick_profile_id, C_KICK_SRS_X);
    assert_eq!(descriptor.has_verified_kick_profile, 1);
}

#[test]
fn verified_no_kick_table_can_compile_as_verified_descriptor_too() {
    let verified = VerifiedKickTableProfile::try_new(NoKick::profile()).expect("verified");
    let problem = opening_problem_with_rule(no_kick(), Some(verified));

    let descriptor = RuleDescriptorCompiler::compile(&problem).expect("descriptor");

    assert_eq!(descriptor.rule_profile_id, C_RULE_NO_KICK);
    assert_eq!(descriptor.kick_profile_id, C_KICK_NO_KICK);
    assert_eq!(descriptor.has_verified_kick_profile, 1);
    assert_eq!(descriptor.verified_transition_count, 56);
}

fn opening_problem_with_rule(
    rule: RuleProfile,
    verified_profile: Option<VerifiedKickTableProfile>,
) -> SearchProblem {
    let mut query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
        ])))
        .with_rule(rule);
    if let Some(profile) = verified_profile {
        query = query.with_verified_kick_table_profile(profile);
    }
    ProblemCompiler::compile_opening_pc(&query).expect("problem")
}

fn verified_imported_srs_x_profile() -> VerifiedKickTableProfile {
    let entries = SrsKicks::srs_plus_profile().entries().to_vec();
    VerifiedKickTableProfile::try_new(KickTableProfile::new(
        KickTableProfileId::Imported,
        RuleProfileId::SrsX,
        entries,
    ))
    .expect("verified imported profile")
}
