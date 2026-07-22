use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    kicks::{KickTableEntry, KickTableProfile, KickTableProfileId, KickTransition, SrsKicks},
    line_clear::LineClearPolicy,
    profile::rule_profile::RuleProfileId,
    rotation::RotationSystem,
    spawn::SpawnProfile,
};

use super::*;

#[test]
fn custom_rule_editor_schema_validates() {
    let schema = custom_rule_editor_schema();

    let report = CustomRuleVerificationReport::verify_editor_schema(&schema);
    let verified =
        VerifiedCustomRuleProfile::try_from_editor_schema(schema).expect("verified rule");

    assert!(report.is_verified());
    assert_eq!(verified.rule_profile().id(), RuleProfileId::Custom);
    assert_eq!(
        verified.verified_kick_profile().profile().source_rule(),
        RuleProfileId::Custom
    );
    assert!(verified.can_compile_to_c_descriptor());
    assert_eq!(report.missing_transition(), 0);
    assert_eq!(report.duplicate_transition(), 0);
    assert_eq!(report.invalid_rotation(), 0);
    assert_eq!(report.unsupported_piece(), 0);
    assert_eq!(report.unsupported_board_backend(), 0);
    assert_eq!(report.unsupported_runtime_feature(), 0);
}

#[test]
fn custom_rule_verify_reports_missing_transition() {
    let schema = CustomRuleEditorSchema::new(
        "custom:missing",
        "Missing",
        vec![0, 1, 2, 3],
        standard_spawn_rules(),
        KickTableProfile::new(
            KickTableProfileId::Custom,
            RuleProfileId::Custom,
            Vec::new(),
        ),
        Vec::new(),
        true,
        Vec::new(),
        LineClearPolicy::ClearFullRows,
        LockReachabilityPolicy::LockReachability,
        vec![CustomRuleBoardBackend::Board64],
        supported_runtime_features(),
    );

    let report = CustomRuleVerificationReport::verify_editor_schema(&schema);

    assert!(report.missing_transition() > 0);
    assert!(report
        .errors()
        .contains(&CustomRuleVerificationIssue::MissingTransition));
    assert!(report
        .errors()
        .contains(&CustomRuleVerificationIssue::MissingFirstSuccessOrder));
    assert!(!report.is_verified());
}

#[test]
fn custom_rule_verify_reports_duplicate_transition() {
    let mut first_success_order = first_success_order();
    first_success_order.push(first_success_order[0]);
    let schema = CustomRuleEditorSchema::new(
        "custom:duplicate",
        "Duplicate",
        vec![0, 1, 2, 3],
        standard_spawn_rules(),
        complete_custom_kick_profile(),
        first_success_order,
        true,
        Vec::new(),
        LineClearPolicy::ClearFullRows,
        LockReachabilityPolicy::LockReachability,
        vec![CustomRuleBoardBackend::Board64],
        supported_runtime_features(),
    );

    let report = CustomRuleVerificationReport::verify_editor_schema(&schema);

    assert!(report.duplicate_transition() > 0);
    assert!(report
        .errors()
        .contains(&CustomRuleVerificationIssue::DuplicateTransition));
    assert!(!report.is_verified());
}

#[test]
fn custom_rule_editor_reports_invalid_rotation_piece_backend_and_runtime_feature() {
    let schema = CustomRuleEditorSchema::new(
        "custom:bad-surface",
        "Bad Surface",
        vec![0, 1, 4],
        vec![CustomRuleSpawnRule::new("P", SpawnProfile::STANDARD_10)],
        complete_custom_kick_profile(),
        first_success_order(),
        true,
        vec![CustomRulePieceSpecificOverride::new("Q")],
        LineClearPolicy::ClearFullRows,
        LockReachabilityPolicy::LockReachability,
        vec![CustomRuleBoardBackend::Board128],
        vec![CustomRuleRuntimeFeature::WideBoardSearch],
    );

    let report = CustomRuleVerificationReport::verify_editor_schema(&schema);

    assert_eq!(report.invalid_rotation(), 1);
    assert_eq!(report.unsupported_piece(), 2);
    assert_eq!(report.unsupported_board_backend(), 1);
    assert!(report.unsupported_runtime_feature() > 0);
    assert!(report
        .errors()
        .contains(&CustomRuleVerificationIssue::InvalidRotation));
    assert!(report
        .errors()
        .contains(&CustomRuleVerificationIssue::UnsupportedPiece));
    assert!(report
        .errors()
        .contains(&CustomRuleVerificationIssue::UnsupportedBoardBackend));
    assert!(report
        .errors()
        .contains(&CustomRuleVerificationIssue::UnsupportedRuntimeFeature));
}

#[test]
fn custom_rule_editor_draft_verifies_into_profile_before_search_capability() {
    let draft = custom_rule_editor_draft();

    let verified = VerifiedCustomRuleProfile::try_from_editor_draft(draft).expect("verified rule");
    let capability = verified.search_capability_report();

    assert_eq!(verified.rule_profile().id(), RuleProfileId::Custom);
    assert!(!capability.search_backend_supported());
    assert_eq!(
        capability.unsupported_reason(),
        Some("custom_rule_search_backend_not_connected")
    );
    assert!(capability.supports_180());
    assert!(capability.requires_lock_reachability());
    assert!(capability.c_compact_descriptor_ready());
}

#[test]
fn raw_custom_rule_draft_rejects_unverified_kick_profile_before_verified_profile_exists() {
    let draft = CustomRuleEditorDraft::new(
        "custom:bad",
        "Bad",
        KickTableProfile::new(
            KickTableProfileId::Custom,
            RuleProfileId::Custom,
            Vec::new(),
        ),
        SpawnProfile::STANDARD_10,
        RotationSystem::Srs,
        LockReachabilityPolicy::LockReachability,
        LineClearPolicy::ClearFullRows,
    );

    let report = VerifiedCustomRuleProfile::try_from_editor_draft(draft).expect_err("unverified");

    assert!(report
        .errors()
        .contains(&CustomRuleVerificationIssue::MissingTransition));
    assert!(!report.is_verified());
}

fn custom_rule_editor_draft() -> CustomRuleEditorDraft {
    CustomRuleEditorDraft::new(
        "custom:clearra-test-rule",
        "Clearra Test Rule",
        complete_custom_kick_profile(),
        SpawnProfile::STANDARD_10,
        RotationSystem::Srs,
        LockReachabilityPolicy::LockReachability,
        LineClearPolicy::ClearFullRows,
    )
}

fn custom_rule_editor_schema() -> CustomRuleEditorSchema {
    CustomRuleEditorSchema::new(
        "custom:clearra-test-rule",
        "Clearra Test Rule",
        vec![0, 1, 2, 3],
        standard_spawn_rules(),
        complete_custom_kick_profile(),
        first_success_order(),
        true,
        Vec::new(),
        LineClearPolicy::ClearFullRows,
        LockReachabilityPolicy::LockReachability,
        vec![CustomRuleBoardBackend::Board64],
        supported_runtime_features(),
    )
}

fn complete_custom_kick_profile() -> KickTableProfile {
    KickTableProfile::new(
        KickTableProfileId::Custom,
        RuleProfileId::Custom,
        SrsKicks::srs_plus_profile().entries().to_vec(),
    )
}

fn first_success_order() -> Vec<KickTransition> {
    complete_custom_kick_profile()
        .entries()
        .iter()
        .map(KickTableEntry::transition)
        .collect()
}

fn standard_spawn_rules() -> Vec<CustomRuleSpawnRule> {
    PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .map(|piece| CustomRuleSpawnRule::for_piece(piece, SpawnProfile::STANDARD_10))
        .collect()
}

fn supported_runtime_features() -> Vec<CustomRuleRuntimeFeature> {
    vec![
        CustomRuleRuntimeFeature::CompactCDescriptor,
        CustomRuleRuntimeFeature::StandardTetrominoPieces,
        CustomRuleRuntimeFeature::Board64Search,
    ]
}
