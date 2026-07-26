use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

use crate::{
    kicks::{
        kick_table::{KickOffset, KickTableEntry, KickTableProfile, KickTableProfileId},
        KickTransition,
    },
    profile::rule_profile::RuleProfileId,
};

use super::*;

#[test]
fn verification_case_passes_when_profile_sequence_matches() {
    let transition = KickTransition::new(PieceKind::T, RotationState::Zero, RotationState::Right);
    let sequence = KickOffsetSequence::new(vec![KickOffset::new(0, 0)]);
    let profile = KickTableProfile::new(
        KickTableProfileId::Custom,
        RuleProfileId::Custom,
        vec![KickTableEntry::new(transition, sequence.clone())],
    );
    let case = KickVerificationCase::new("custom T 0R", transition, sequence);

    assert_eq!(case.verify(&profile), KickVerificationOutcome::Passed);
}

#[test]
fn verification_case_reports_missing_transition() {
    let transition = KickTransition::new(PieceKind::T, RotationState::Zero, RotationState::Right);
    let profile = KickTableProfile::new(
        KickTableProfileId::Custom,
        RuleProfileId::Custom,
        Vec::new(),
    );
    let case = KickVerificationCase::new(
        "missing",
        transition,
        KickOffsetSequence::new(vec![KickOffset::new(0, 0)]),
    );

    assert_eq!(
        case.verify(&profile),
        KickVerificationOutcome::Failed {
            reason: KickVerificationFailureReason::MissingTransition,
            actual_sequence: None
        }
    );
}

#[test]
fn imported_profile_verification_reports_completeness_duplicates_and_annotations() {
    let transition = KickTransition::new(PieceKind::T, RotationState::Zero, RotationState::Right);
    let duplicate = KickTableEntry::new(
        transition,
        KickOffsetSequence::new(vec![KickOffset::new(0, 0)]),
    )
    .with_unsupported_reason("manual_review_required");
    let profile = KickTableProfile::new(
        KickTableProfileId::Imported,
        RuleProfileId::Custom,
        vec![
            KickTableEntry::new(
                transition,
                KickOffsetSequence::new(vec![KickOffset::new(0, 0)]),
            ),
            duplicate,
        ],
    );

    let report = KickProfileVerificationReport::verify_imported_profile(&profile);

    assert!(!report.transition_complete());
    assert!(report.missing_transition_count() > 0);
    assert_eq!(report.duplicate_transition_count(), 1);
    assert_eq!(report.unsupported_annotation_count(), 1);
    assert!(report.issue_count() > 1);
}

#[test]
fn verified_kick_table_profile_accepts_complete_profile_contract() {
    let profile = crate::kicks::SrsKicks::profile();
    let verified =
        VerifiedKickTableProfile::try_new(profile.clone()).expect("complete SRS profile");

    assert_eq!(verified.profile(), &profile);
    assert_eq!(verified.report().issue_count(), 0);
}

#[test]
fn verified_kick_table_profile_accepts_jstris_without_o_rotation_transitions() {
    let profile = crate::kicks::SrsKicks::jstris_180_profile();
    let verified =
        VerifiedKickTableProfile::try_new(profile.clone()).expect("complete Jstris profile");

    assert_eq!(verified.profile(), &profile);
    assert_eq!(verified.profile().transition_count(), 72);
    assert_eq!(verified.report().missing_transition_count(), 0);
}

#[test]
fn imported_jstris_profile_uses_the_same_non_o_transition_domain() {
    let profile = crate::kicks::SrsKicks::jstris_180_profile();
    let imported = KickTableProfile::new(
        KickTableProfileId::Imported,
        RuleProfileId::Jstris180,
        profile.entries().to_vec(),
    );

    let verified =
        VerifiedKickTableProfile::try_new(imported).expect("verified imported Jstris profile");

    assert_eq!(verified.report().issue_count(), 0);
}

#[test]
fn verified_kick_table_profile_rejects_incomplete_or_duplicate_imports() {
    let transition = KickTransition::new(PieceKind::T, RotationState::Zero, RotationState::Right);
    let profile = KickTableProfile::new(
        KickTableProfileId::Imported,
        RuleProfileId::Custom,
        vec![
            KickTableEntry::new(
                transition,
                KickOffsetSequence::new(vec![KickOffset::new(0, 0)]),
            ),
            KickTableEntry::new(
                transition,
                KickOffsetSequence::new(vec![KickOffset::new(1, 0)]),
            ),
        ],
    );

    let report =
        VerifiedKickTableProfile::try_new(profile).expect_err("incomplete duplicate profile");

    assert!(report.issue_count() > 0);
    assert_eq!(report.duplicate_transition_count(), 1);
    assert!(!report.transition_complete());
}
