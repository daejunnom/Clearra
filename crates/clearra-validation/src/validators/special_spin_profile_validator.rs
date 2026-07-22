use clearra_scoring::spin::{
    SpecialSpinCase, SpecialSpinCaseId, SpecialSpinVerificationState, SpinClassifierCapability,
    VerifiedSpecialSpinProfile,
};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpecialSpinProfileValidationMode {
    Exact,
    EstimatedAllowed,
}

impl SpecialSpinProfileValidationMode {
    fn requires_exact(self) -> bool {
        matches!(self, Self::Exact)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpecialSpinProfileValidationContext {
    mode: SpecialSpinProfileValidationMode,
    kick_evidence_available: bool,
}

impl SpecialSpinProfileValidationContext {
    pub fn new(mode: SpecialSpinProfileValidationMode, kick_evidence_available: bool) -> Self {
        Self {
            mode,
            kick_evidence_available,
        }
    }
}
impl SpecialSpinProfileValidationContext {
    pub fn exact(kick_evidence_available: bool) -> Self {
        Self::new(
            SpecialSpinProfileValidationMode::Exact,
            kick_evidence_available,
        )
    }
}
impl SpecialSpinProfileValidationContext {
    pub fn estimated_allowed(kick_evidence_available: bool) -> Self {
        Self::new(
            SpecialSpinProfileValidationMode::EstimatedAllowed,
            kick_evidence_available,
        )
    }
}
impl SpecialSpinProfileValidationContext {
    pub fn mode(self) -> SpecialSpinProfileValidationMode {
        self.mode
    }
}
impl SpecialSpinProfileValidationContext {
    pub fn kick_evidence_available(self) -> bool {
        self.kick_evidence_available
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpecialSpinProfileValidator;

impl SpecialSpinProfileValidator {
    pub fn validate_case(
        special_case: &SpecialSpinCase,
        context: SpecialSpinProfileValidationContext,
    ) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();
        validate_case_verification_state(special_case, context, &mut report);
        validate_case_kick_evidence(special_case, context, &mut report);
        report
    }
}
impl SpecialSpinProfileValidator {
    pub fn validate_verified_profile(
        profile: &VerifiedSpecialSpinProfile,
        context: SpecialSpinProfileValidationContext,
    ) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();
        if context.mode().requires_exact() && !profile.spin_classifier_capability().supports_exact()
        {
            report.push(
                Diagnostic::new(
                    DiagnosticCode::ESpinProfileUnverified,
                    "verified special spin profile does not expose exact classifier capability",
                )
                .with_location(EvidenceLocation::new("special_spin_profile"))
                .with_evidence(ValidationEvidence::new("profile_id", profile.id()))
                .with_evidence(ValidationEvidence::new(
                    "capability",
                    capability_label(profile.spin_classifier_capability()),
                ))
                .with_evidence(ValidationEvidence::new(
                    "reason",
                    "classifier_capability_not_exact",
                )),
            );
        }

        if context.mode().requires_exact() && profile.special_cases().is_empty() {
            report.push(
                Diagnostic::new(
                    DiagnosticCode::ESpinProfileUnverified,
                    "exact special spin profile must pin at least one verified special spin case",
                )
                .with_location(EvidenceLocation::new("special_spin_profile.special_cases"))
                .with_evidence(ValidationEvidence::new("profile_id", profile.id()))
                .with_evidence(ValidationEvidence::new(
                    "reason",
                    "missing_verified_special_cases",
                )),
            );
        }

        report
    }
}

pub fn validate_special_spin_case(
    special_case: &SpecialSpinCase,
    context: SpecialSpinProfileValidationContext,
) -> DiagnosticReport {
    SpecialSpinProfileValidator::validate_case(special_case, context)
}

pub fn validate_verified_special_spin_profile(
    profile: &VerifiedSpecialSpinProfile,
    context: SpecialSpinProfileValidationContext,
) -> DiagnosticReport {
    SpecialSpinProfileValidator::validate_verified_profile(profile, context)
}

fn validate_case_verification_state(
    special_case: &SpecialSpinCase,
    context: SpecialSpinProfileValidationContext,
    report: &mut DiagnosticReport,
) {
    if special_case
        .verification_state()
        .enables_exact_classification()
    {
        return;
    }

    let exact_case_requires_fixture = context.mode().requires_exact()
        && is_source_named_special_case(special_case.id())
        && matches!(
            special_case.verification_state(),
            SpecialSpinVerificationState::DescriptorOnly | SpecialSpinVerificationState::Disabled
        );

    if exact_case_requires_fixture {
        report.push(
            special_case_diagnostic(
                special_case,
                DiagnosticCode::ESpinProfileUnverified,
                "Fin/ISO/NEO exact classification requires a verified import or source-pinned fixture",
                "verified_fixture_required",
            )
            .with_suggested_next_step(SuggestedNextStep::new(
                "Promote descriptor-only special spin cases to verified imports or source-pinned fixtures before exact classification.",
            )),
        );
        return;
    }

    let code = if context.mode().requires_exact() {
        DiagnosticCode::ESpinProfileUnverified
    } else {
        DiagnosticCode::WSpinClassificationEstimated
    };
    report.push(special_case_diagnostic(
        special_case,
        code,
        "descriptor-only special spin cases cannot produce exact spin classification",
        "special_spin_profile_unverified",
    ));
}

fn validate_case_kick_evidence(
    special_case: &SpecialSpinCase,
    context: SpecialSpinProfileValidationContext,
    report: &mut DiagnosticReport,
) {
    if !special_case.kick_evidence_requirement().requires_evidence()
        || context.kick_evidence_available()
    {
        return;
    }

    let code = if context.mode().requires_exact() {
        DiagnosticCode::ESpinKickEvidenceMissing
    } else {
        DiagnosticCode::WSpinClassificationEstimated
    };
    report.push(special_case_diagnostic(
        special_case,
        code,
        "special spin classification requires kick evidence that is not available in the trace",
        "spin_kick_evidence_missing",
    ));
}

fn special_case_diagnostic(
    special_case: &SpecialSpinCase,
    code: DiagnosticCode,
    message: &'static str,
    reason: &'static str,
) -> Diagnostic {
    Diagnostic::new(code, message)
        .with_location(EvidenceLocation::new("special_spin_case"))
        .with_evidence(ValidationEvidence::new(
            "special_spin_case_id",
            special_case.id().as_str(),
        ))
        .with_evidence(ValidationEvidence::new(
            "verification_state",
            verification_state_label(special_case.verification_state()),
        ))
        .with_evidence(ValidationEvidence::new(
            "kick_evidence_requirement",
            format!("{:?}", special_case.kick_evidence_requirement()),
        ))
        .with_evidence(ValidationEvidence::new("reason", reason))
}

fn is_source_named_special_case(id: &SpecialSpinCaseId) -> bool {
    matches!(
        id,
        SpecialSpinCaseId::Fin | SpecialSpinCaseId::Iso | SpecialSpinCaseId::Neo
    )
}

fn verification_state_label(state: SpecialSpinVerificationState) -> &'static str {
    match state {
        SpecialSpinVerificationState::SourcePinnedFixture => "source-pinned-fixture",
        SpecialSpinVerificationState::VerifiedImport => "verified-import",
        SpecialSpinVerificationState::DescriptorOnly => "descriptor-only",
        SpecialSpinVerificationState::Disabled => "disabled",
    }
}

fn capability_label(capability: SpinClassifierCapability) -> &'static str {
    match capability {
        SpinClassifierCapability::Disabled => "disabled",
        SpinClassifierCapability::DescriptorOnly => "descriptor-only",
        SpinClassifierCapability::ExactWithKickEvidence => "exact-with-kick-evidence",
        SpinClassifierCapability::SourcePinnedExact => "source-pinned-exact",
    }
}

#[cfg(test)]
#[path = "special_spin_profile_validator_tests.rs"]
mod tests;
