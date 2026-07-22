use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_rules::{
    custom_rule::{
        CustomRuleBoardBackend, CustomRuleEditorSchema, CustomRuleRuntimeFeature,
        CustomRuleSpawnRule, LockReachabilityPolicy,
    },
    kicks::{KickTableEntry, KickTableProfile, KickTableProfileId, SrsKicks},
    line_clear::LineClearPolicy,
    profile::rule_profile::RuleProfileId,
    spawn::SpawnProfile,
};

use crate::problem::{C_KICK_CUSTOM, C_RULE_CUSTOM};

use super::*;

#[test]
fn verified_custom_rule_can_compile_to_descriptor_when_supported() {
    let schema = valid_schema();
    let verified =
        VerifiedCustomRuleProfile::try_from_editor_schema(schema).expect("verified custom");

    let descriptor = CustomRuleDescriptorCompiler::compile_verified(&verified).expect("descriptor");

    assert_eq!(descriptor.rule_profile_id, C_RULE_CUSTOM);
    assert_eq!(descriptor.kick_profile_id, C_KICK_CUSTOM);
    assert_eq!(descriptor.has_verified_kick_profile, 1);
    assert_eq!(descriptor.verified_supports_180, 1);
    assert_eq!(descriptor.verified_transition_count, 80);
}

#[test]
fn unverified_custom_rule_rejected_before_execution() {
    let schema = valid_schema();

    let result = CustomRuleDescriptorCompiler::compile_editor_schema(&schema);

    assert_eq!(
        result,
        Err(FfiProblemError::UnverifiedCustomRuleRejectedBeforeExecution)
    );
}

fn valid_schema() -> CustomRuleEditorSchema {
    let kick_profile = complete_custom_kick_profile();
    let first_success_order = kick_profile
        .entries()
        .iter()
        .map(KickTableEntry::transition)
        .collect();
    CustomRuleEditorSchema::new(
        "custom:ffi",
        "FFI",
        vec![0, 1, 2, 3],
        PieceKind::STANDARD_TETROMINOES
            .into_iter()
            .map(|piece| CustomRuleSpawnRule::for_piece(piece, SpawnProfile::STANDARD_10))
            .collect(),
        kick_profile,
        first_success_order,
        true,
        Vec::new(),
        LineClearPolicy::ClearFullRows,
        LockReachabilityPolicy::LockReachability,
        vec![CustomRuleBoardBackend::Board64],
        vec![
            CustomRuleRuntimeFeature::CompactCDescriptor,
            CustomRuleRuntimeFeature::StandardTetrominoPieces,
            CustomRuleRuntimeFeature::Board64Search,
        ],
    )
}

fn complete_custom_kick_profile() -> KickTableProfile {
    KickTableProfile::new(
        KickTableProfileId::Custom,
        RuleProfileId::Custom,
        SrsKicks::srs_plus_profile().entries().to_vec(),
    )
}
