use clearra_scoring::spin::{SpecialSpinCase, SpecialSpinVerificationState};

use crate::diagnostic::diagnostic_code::DiagnosticCode;

use super::*;

#[test]
fn special_spin_profile_validator_requires_verified_import() {
    let special_case = SpecialSpinCase::fin_descriptor();
    let context = SpecialSpinProfileValidationContext::exact(true);

    let report = validate_special_spin_case(&special_case, context);

    assert!(report.contains_code(DiagnosticCode::ESpinProfileUnverified));
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "reason"
            && evidence.value() == "verified_fixture_required")));
}

#[test]
fn verified_special_spin_profile_enables_exact_when_kick_evidence_exists() {
    let special_case = SpecialSpinCase::fin_descriptor()
        .with_verification_state(SpecialSpinVerificationState::VerifiedImport);
    let context = SpecialSpinProfileValidationContext::exact(true);

    let report = validate_special_spin_case(&special_case, context);

    assert!(!report.has_errors());
}

#[test]
fn exact_special_spin_profile_requires_kick_evidence() {
    let special_case = SpecialSpinCase::fin_descriptor()
        .with_verification_state(SpecialSpinVerificationState::VerifiedImport);
    let context = SpecialSpinProfileValidationContext::exact(false);

    let report = validate_special_spin_case(&special_case, context);

    assert!(report.contains_code(DiagnosticCode::ESpinKickEvidenceMissing));
}
