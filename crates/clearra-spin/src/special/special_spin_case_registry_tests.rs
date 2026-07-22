use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_replay::RotationRequest;

use crate::{
    evidence::{BoardAnchor, KickEvidence},
    special::{
        CornerRuleOverride, KickSignature, SpecialSpinCase, SpecialSpinCaseId,
        SpecialSpinVerificationState,
    },
};

use super::*;

#[test]
fn iso_neo_are_special_spin_cases_not_kick_tables() {
    let registry = SpecialSpinCaseRegistry::with_builtin_descriptors();

    assert!(registry.get(&SpecialSpinCaseId::Fin).is_some());
    assert!(registry.get(&SpecialSpinCaseId::Iso).is_some());
    assert!(registry.get(&SpecialSpinCaseId::Neo).is_some());
    let fin = registry
        .get(&SpecialSpinCaseId::Fin)
        .expect("fin descriptor");
    assert_eq!(fin.corner_rule_override, CornerRuleOverride::ForceRegular);
    assert_eq!(fin.mini_override, Some(false));
    let neo = registry
        .get(&SpecialSpinCaseId::Neo)
        .expect("neo descriptor");
    assert_eq!(neo.corner_rule_override, CornerRuleOverride::ForceMini);
    assert_eq!(neo.mini_override, Some(true));
    assert_eq!(
        registry
            .cases_for_piece(PieceKind::T)
            .map(|case| case.id.as_str())
            .collect::<Vec<_>>(),
        vec!["fin", "iso", "neo"]
    );
}

#[test]
fn fin_requires_kick_evidence_and_first_success() {
    let signature = KickSignature::new(
        RotationState::Zero,
        RotationState::Right,
        RotationRequest::Clockwise,
        2,
        1,
        -1,
    );
    let case = SpecialSpinCase::fin_descriptor()
        .with_required_kick_signature(signature)
        .with_allowed_profile("exact-special")
        .with_verification_state(SpecialSpinVerificationState::VerifiedImport);
    let kick = KickEvidence::first_success(
        RotationState::Zero,
        RotationState::Right,
        RotationRequest::Clockwise,
        2,
        1,
        -1,
        "srs-plus",
    )
    .with_verified_profile("verified-srs-plus")
    .with_anchors(BoardAnchor::new(3, 4), BoardAnchor::new(4, 3));

    assert!(!case.exact_match("exact-special", None, 0, 0));
    assert!(case.exact_match("exact-special", Some(&kick), 0, 0));
}
