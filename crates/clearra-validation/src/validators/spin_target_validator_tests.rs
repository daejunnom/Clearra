use clearra_scoring::{
    profile::ScoreProfileRegistry,
    spin::{RequiredSpinKind, SpinTarget, SpinTargetId},
};

use crate::diagnostic::diagnostic_code::DiagnosticCode;

use super::*;

#[test]
fn spin_target_validator_rejects_missing_classifier() {
    let registry = ScoreProfileRegistry::builtins();
    let target = SpinTarget::tsd("tsd");
    let context = SpinTargetValidationContext::new(
        SpinTargetCapability::disabled(),
        SpinTargetValidationMode::Exact,
        &registry,
    );

    let report = validate_spin_target(&target, context);

    assert!(report.contains_code(DiagnosticCode::ESpinClassifierIncompatible));
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic
            .evidence()
            .iter()
            .any(|evidence| evidence.key() == "reason"
                && evidence.value() == "missing_spin_classifier")));
}

#[test]
fn spin_target_validator_rejects_unverified_special_spin_exact() {
    let registry = ScoreProfileRegistry::builtins();
    let target = SpinTarget::new(
        SpinTargetId::new("fin-target"),
        RequiredSpinKind::ProfileSpecific("fin"),
    )
    .with_required_score_profile("tetrio");
    let context = SpinTargetValidationContext::new(
        SpinTargetCapability::exact_supported()
            .with_special_spin_profile_verified(false)
            .with_kick_evidence_available(true),
        SpinTargetValidationMode::Exact,
        &registry,
    );

    let report = validate_spin_target(&target, context);

    assert!(report.contains_code(DiagnosticCode::ESpinProfileUnverified));
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "reason"
            && evidence.value() == "special_spin_profile_unverified")));
}
