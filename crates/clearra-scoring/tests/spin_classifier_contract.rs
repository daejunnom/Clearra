use clearra_scoring::{
    profile::ScoreProfile,
    spin::{
        KickEvidence, KickSensitiveSpinRule, MovementInfo, RotationRequest, SpecialSpinCase,
        SpecialSpinCaseRegistry, SpecialSpinVerificationState, SpinAccuracy,
        SpinClassificationInput, SpinClassifier, SpinKind, TSpinCornerRule,
    },
};

#[test]
fn corner_rule_classifies_t_spin_from_corner_evidence() {
    let profile = ScoreProfile::new("basic", "Basic");
    let mut input = SpinClassificationInput::new('T', 2).with_blocked_corners(3);
    input.front_corners = 2;
    input.movement_info = MovementInfo {
        immobile: false,
        rotation_used: true,
        evidence_complete: true,
    };

    let classification = TSpinCornerRule.classify(input, &profile);
    let result = classification.result();

    assert_eq!(result.spin_kind(), SpinKind::TSpin);
    assert_eq!(result.cleared_lines(), 2);
    assert_eq!(result.accuracy(), SpinAccuracy::Exact);

    let mut neo = SpinClassificationInput::new('T', 2)
        .with_blocked_corners(3)
        .with_kick_evidence(KickEvidence::first_success(
            2,
            1,
            RotationRequest::CounterClockwise,
            3,
            0,
            2,
            "srs-plus",
        ));
    neo.front_corners = 1;
    neo.movement_info = MovementInfo {
        immobile: false,
        rotation_used: true,
        evidence_complete: true,
    };
    let neo = TSpinCornerRule.classify(neo, &profile).result();
    assert_eq!(neo.spin_kind(), SpinKind::TSpinMini);
    assert!(neo.is_mini());

    let mut fin = SpinClassificationInput::new('T', 2)
        .with_blocked_corners(3)
        .with_kick_evidence(KickEvidence::first_success(
            2,
            1,
            RotationRequest::CounterClockwise,
            4,
            1,
            2,
            "srs-plus",
        ));
    fin.front_corners = 1;
    fin.movement_info = MovementInfo {
        immobile: false,
        rotation_used: true,
        evidence_complete: true,
    };
    let fin = TSpinCornerRule.classify(fin, &profile).result();
    assert_eq!(fin.spin_kind(), SpinKind::TSpin);
    assert!(!fin.is_mini());
}

#[test]
fn kick_sensitive_rule_requires_kick_evidence_for_exact_result() {
    let profile = ScoreProfile::new("special", "Special");
    let input_without_evidence = SpinClassificationInput::new('T', 2);

    let rule = KickSensitiveSpinRule::without_special_cases();
    let missing = rule.classify(input_without_evidence, &profile);

    assert!(!missing.result().is_spin());
    assert_eq!(
        missing.result().accuracy(),
        SpinAccuracy::KickSensitiveUnavailable
    );

    let evidence = KickEvidence::first_success(0, 1, RotationRequest::Clockwise, 4, 1, 0, "srs");
    let exact_input = SpinClassificationInput::new('T', 2)
        .with_blocked_corners(3)
        .with_kick_evidence(evidence);
    let exact = rule.classify(exact_input, &profile);

    assert_eq!(exact.result().spin_kind(), SpinKind::TSpin);
    assert!(exact.result().kick_used());
    assert_eq!(exact.result().accuracy(), SpinAccuracy::Exact);
}

#[test]
fn kick_sensitive_spin_requires_kick_evidence() {
    let profile = ScoreProfile::new("special", "Special");
    let rule = KickSensitiveSpinRule::without_special_cases();
    let missing = rule.classify(SpinClassificationInput::new('T', 2), &profile);

    assert_eq!(
        missing.result().accuracy(),
        SpinAccuracy::KickSensitiveUnavailable
    );
    assert!(!missing.result().is_spin());

    let evidence = KickEvidence::first_success(0, 1, RotationRequest::Clockwise, 2, 1, 0, "srs");
    let exact = rule.classify(
        SpinClassificationInput::new('T', 2)
            .with_blocked_corners(3)
            .with_kick_evidence(evidence),
        &profile,
    );

    assert_eq!(exact.result().accuracy(), SpinAccuracy::Exact);
    assert!(exact.result().kick_used());
}

#[test]
fn kick_sensitive_rule_uses_verified_special_spin_registry() {
    let profile = ScoreProfile::new("special", "Special");
    let evidence = KickEvidence::first_success(0, 1, RotationRequest::Clockwise, 2, 1, 0, "srs");
    let signature = evidence.stable_signature();
    let registry = SpecialSpinCaseRegistry::new([SpecialSpinCase::fin_descriptor()
        .with_verification_state(SpecialSpinVerificationState::VerifiedImport)
        .with_allowed_profile("special")
        .with_required_kick_signature(signature)
        .with_board_signature_predicate("blocked-corners>=3")]);
    let rule = KickSensitiveSpinRule::new(&registry);

    let classification = rule.classify(
        SpinClassificationInput::new('T', 2)
            .with_blocked_corners(3)
            .with_kick_evidence(evidence),
        &profile,
    );

    assert_eq!(
        classification.result().spin_kind(),
        SpinKind::ProfileSpecific("fin")
    );
    assert_eq!(classification.result().accuracy(), SpinAccuracy::Exact);
    assert!(classification.result().kick_used());
}

#[test]
fn kick_sensitive_rule_does_not_enable_unverified_special_case() {
    let profile = ScoreProfile::new("special", "Special");
    let evidence = KickEvidence::first_success(0, 1, RotationRequest::Clockwise, 2, 1, 0, "srs");
    let registry = SpecialSpinCaseRegistry::new([SpecialSpinCase::fin_descriptor()
        .with_allowed_profile("special")
        .with_required_kick_signature(evidence.stable_signature())
        .with_board_signature_predicate("blocked-corners>=3")]);
    let rule = KickSensitiveSpinRule::new(&registry);

    let classification = rule.classify(
        SpinClassificationInput::new('T', 2)
            .with_blocked_corners(3)
            .with_kick_evidence(evidence),
        &profile,
    );

    assert_eq!(classification.result().spin_kind(), SpinKind::TSpinMini);
    assert_eq!(classification.result().accuracy(), SpinAccuracy::Exact);
}

#[test]
fn kick_sensitive_rule_checks_profile_kick_signature_and_board_predicate() {
    let profile = ScoreProfile::new("other-profile", "Other");
    let evidence = KickEvidence::first_success(0, 1, RotationRequest::Clockwise, 2, 1, 0, "srs");
    let registry = SpecialSpinCaseRegistry::new([SpecialSpinCase::fin_descriptor()
        .with_verification_state(SpecialSpinVerificationState::VerifiedImport)
        .with_allowed_profile("special")
        .with_required_kick_signature(evidence.stable_signature())
        .with_board_signature_predicate("blocked-corners>=3")]);
    let rule = KickSensitiveSpinRule::new(&registry);

    let profile_mismatch = rule.classify(
        SpinClassificationInput::new('T', 2)
            .with_blocked_corners(3)
            .with_kick_evidence(evidence.clone()),
        &profile,
    );
    assert_eq!(profile_mismatch.result().spin_kind(), SpinKind::TSpinMini);

    let board_mismatch = rule.classify(
        SpinClassificationInput::new('T', 2)
            .with_blocked_corners(2)
            .with_kick_evidence(evidence),
        &ScoreProfile::new("special", "Special"),
    );
    assert!(!board_mismatch.result().is_spin());
    assert_eq!(board_mismatch.result().accuracy(), SpinAccuracy::Incomplete);
}

#[test]
fn kick_sensitive_rule_falls_back_to_all_spin_for_immobile_non_t() {
    let profile = ScoreProfile::new("special", "Special");
    let evidence = KickEvidence::first_success(0, 1, RotationRequest::Clockwise, 2, 1, 0, "srs");
    let mut input = SpinClassificationInput::new('L', 1).with_kick_evidence(evidence);
    input.movement_info = MovementInfo {
        immobile: true,
        rotation_used: true,
        evidence_complete: true,
    };

    let classification = KickSensitiveSpinRule::without_special_cases().classify(input, &profile);

    assert_eq!(classification.result().spin_kind(), SpinKind::AllSpin);
    assert_eq!(classification.result().accuracy(), SpinAccuracy::Exact);
}
