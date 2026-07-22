use clearra_rules::kicks::KickTableProfileId;
use clearra_scoring::spin::{
    KickEvidence, KickEvidenceRequirement, RotationRequest, SpecialSpinCase, SpecialSpinCaseId,
    SpecialSpinCaseRegistry, SpecialSpinVerificationState, SpinClassificationInput,
    SpinClassifierCapability, SpinKind, VerifiedSpecialSpinProfile,
};

#[test]
fn fin_iso_neo_are_special_spin_cases_not_kick_tables() {
    assert_eq!(SpecialSpinCaseId::Fin.as_str(), "fin");
    assert_eq!(SpecialSpinCaseId::Iso.as_str(), "iso");
    assert_eq!(SpecialSpinCaseId::Neo.as_str(), "neo");

    let fin = SpecialSpinCase::fin_descriptor();

    assert_eq!(fin.id(), &SpecialSpinCaseId::Fin);
    assert_eq!(fin.corner_rule_override(), Some("force-regular"));
    assert_eq!(fin.mini_override(), Some(false));
    assert_eq!(fin.regular_override(), Some(true));
    assert_eq!(
        fin.verification_state(),
        SpecialSpinVerificationState::DescriptorOnly
    );
    assert!(!fin.exact_enabled(true));
    assert_eq!(
        fin.disabled_reason(true),
        Some("special_spin_profile_unverified")
    );

    let neo = SpecialSpinCase::neo_descriptor();
    assert_eq!(neo.corner_rule_override(), Some("force-mini"));
    assert_eq!(neo.mini_override(), Some(true));
    assert_eq!(neo.regular_override(), Some(false));

    let registry = SpecialSpinCaseRegistry::with_builtin_descriptors();
    for id in [
        SpecialSpinCaseId::Fin,
        SpecialSpinCaseId::Iso,
        SpecialSpinCaseId::Neo,
    ] {
        let case = registry.get(&id).expect("builtin special spin case");
        assert_eq!(
            case.verification_state(),
            SpecialSpinVerificationState::DescriptorOnly
        );
        assert_eq!(
            case.disabled_reason(true),
            Some("special_spin_profile_unverified")
        );
    }
}

#[test]
fn special_spin_case_is_not_kick_table_profile() {
    for id in [
        "fin",
        "iso",
        "neo",
        "fin-special",
        "iso-special",
        "neo-special",
    ] {
        assert_eq!(KickTableProfileId::parse(id), None);
    }

    let registry = SpecialSpinCaseRegistry::with_builtin_descriptors();
    assert!(registry.get(&SpecialSpinCaseId::Fin).is_some());
    assert!(registry.get(&SpecialSpinCaseId::Iso).is_some());
    assert!(registry.get(&SpecialSpinCaseId::Neo).is_some());
}

#[test]
fn special_spin_case_requires_kick_evidence() {
    let verified = SpecialSpinCase::fin_descriptor()
        .with_verification_state(SpecialSpinVerificationState::VerifiedImport);

    assert_eq!(
        verified.kick_evidence_requirement(),
        KickEvidenceRequirement::RequiredForExact
    );
    assert!(!verified.exact_enabled(false));
    assert_eq!(
        verified.disabled_reason(false),
        Some("spin_kick_evidence_missing")
    );
    assert!(verified.exact_enabled(true));
}

#[test]
fn special_spin_case_matches_profile_kick_signature_and_board_signature() {
    let evidence = KickEvidence::first_success(0, 1, RotationRequest::Clockwise, 2, 1, 0, "srs");
    let special_case = SpecialSpinCase::neo_descriptor()
        .with_verification_state(SpecialSpinVerificationState::VerifiedImport)
        .with_allowed_profile("special")
        .with_required_kick_signature(evidence.stable_signature())
        .with_board_signature_predicate("blocked-corners>=3");
    let input = SpinClassificationInput::new('T', 2)
        .with_blocked_corners(3)
        .with_kick_evidence(evidence.clone());

    assert!(special_case.exact_enabled(input.has_kick_evidence()));
    assert!(special_case.allowed_for_profile("special"));
    assert!(!special_case.allowed_for_profile("other"));
    assert!(special_case.required_kick_signature_matches(&evidence));
    assert!(special_case.board_signature_matches(&input));

    let classification = special_case.classify(&input);
    assert_eq!(
        classification.result().spin_kind(),
        SpinKind::ProfileSpecific("neo")
    );
    assert!(classification.result().is_mini());
}

#[test]
fn unverified_fin_iso_neo_profile_is_disabled() {
    let registry = SpecialSpinCaseRegistry::with_builtin_descriptors();

    for id in [
        SpecialSpinCaseId::Fin,
        SpecialSpinCaseId::Iso,
        SpecialSpinCaseId::Neo,
    ] {
        let case = registry.get(&id).expect("builtin special spin descriptor");

        assert_eq!(
            case.verification_state(),
            SpecialSpinVerificationState::DescriptorOnly
        );
        assert!(!case.exact_enabled(true));
        assert_eq!(
            case.disabled_reason(true),
            Some("special_spin_profile_unverified")
        );
    }
}

#[test]
fn verified_special_spin_profile_requires_kick_capability_for_exact_cases() {
    let profile = VerifiedSpecialSpinProfile::new(
        "fin-fixture",
        "srs",
        "source-pinned-fin",
        SpinClassifierCapability::ExactWithKickEvidence,
    )
    .with_special_case(SpecialSpinCaseId::Fin);

    assert_eq!(profile.base_kick_profile(), "srs");
    assert!(profile.spin_classifier_capability().supports_exact());
    assert!(profile.search_backend_supported());
    assert_eq!(profile.unsupported_reason(), None);
    assert_eq!(profile.special_cases(), &[SpecialSpinCaseId::Fin]);
}

#[test]
fn verified_special_spin_profile_enables_kick_sensitive_classifier() {
    let profile = VerifiedSpecialSpinProfile::new(
        "fin-fixture",
        "srs",
        "source-pinned-fin",
        SpinClassifierCapability::ExactWithKickEvidence,
    )
    .with_special_case(SpecialSpinCaseId::Fin);

    assert!(profile.search_backend_supported());
    assert_eq!(profile.unsupported_reason(), None);
    assert!(profile.spin_classifier_capability().supports_exact());
    assert_eq!(profile.base_kick_profile(), "srs");
}

#[test]
fn unverified_special_spin_profile_is_not_search_backend_supported() {
    let descriptor_only = VerifiedSpecialSpinProfile::new(
        "fin-descriptor",
        "srs",
        "descriptor-only",
        SpinClassifierCapability::DescriptorOnly,
    )
    .with_special_case(SpecialSpinCaseId::Fin);

    assert!(!descriptor_only.search_backend_supported());
    assert_eq!(
        descriptor_only.unsupported_reason(),
        Some("special_spin_profile_unverified")
    );

    let no_cases = VerifiedSpecialSpinProfile::new(
        "empty-fixture",
        "srs",
        "source-pinned-empty",
        SpinClassifierCapability::SourcePinnedExact,
    );

    assert!(!no_cases.search_backend_supported());
    assert_eq!(
        no_cases.unsupported_reason(),
        Some("verified_special_spin_profile_missing_cases")
    );
}
