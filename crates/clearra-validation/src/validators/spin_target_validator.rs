use clearra_scoring::spin::{RequiredClearLines, RequiredSpinKind, SpinTarget};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

mod capability {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct SpinTargetCapability {
        spin_classifier_available: bool,
        exact_spin_classifier_available: bool,
        kick_evidence_available: bool,
        special_spin_profile_verified: bool,
    }

    impl SpinTargetCapability {
        pub fn disabled() -> Self {
            Self::default()
        }
    }
    impl SpinTargetCapability {
        pub fn exact_supported() -> Self {
            Self {
                spin_classifier_available: true,
                exact_spin_classifier_available: true,
                kick_evidence_available: true,
                special_spin_profile_verified: true,
            }
        }
    }
    impl SpinTargetCapability {
        pub fn with_spin_classifier_available(mut self, available: bool) -> Self {
            self.spin_classifier_available = available;
            self
        }
    }
    impl SpinTargetCapability {
        pub fn with_exact_spin_classifier_available(mut self, available: bool) -> Self {
            self.exact_spin_classifier_available = available;
            self
        }
    }
    impl SpinTargetCapability {
        pub fn with_kick_evidence_available(mut self, available: bool) -> Self {
            self.kick_evidence_available = available;
            self
        }
    }
    impl SpinTargetCapability {
        pub fn with_special_spin_profile_verified(mut self, verified: bool) -> Self {
            self.special_spin_profile_verified = verified;
            self
        }
    }
    impl SpinTargetCapability {
        pub fn spin_classifier_available(self) -> bool {
            self.spin_classifier_available
        }
    }
    impl SpinTargetCapability {
        pub fn exact_spin_classifier_available(self) -> bool {
            self.exact_spin_classifier_available
        }
    }
    impl SpinTargetCapability {
        pub fn kick_evidence_available(self) -> bool {
            self.kick_evidence_available
        }
    }
    impl SpinTargetCapability {
        pub fn special_spin_profile_verified(self) -> bool {
            self.special_spin_profile_verified
        }
    }
}
mod classifier_capability_validator {
    use super::diagnostic_builder::spin_target_diagnostic;
    use super::*;

    pub(super) fn validate_classifier_capability(
        target: &SpinTarget,
        context: SpinTargetValidationContext<'_>,
        report: &mut DiagnosticReport,
    ) {
        if context.capability().spin_classifier_available() {
            return;
        }
        report.push(
            spin_target_diagnostic(
                target,
                DiagnosticCode::ESpinClassifierIncompatible,
                "SpinTarget queries require a SpinClassifier capability before BuildVariant replay can be classified",
                "missing_spin_classifier",
            )
            .with_suggested_next_step(SuggestedNextStep::new(
                "Enable a compatible SpinClassifier before accepting SpinTarget probability queries.",
            )),
        );
    }
}
mod clear_line_validator {
    use super::diagnostic_builder::spin_target_diagnostic;
    use super::*;

    pub(super) fn validate_clear_line_compatibility(
        target: &SpinTarget,
        report: &mut DiagnosticReport,
    ) {
        if line_requirement_exceeds_piece_capacity(target.clear_lines()) {
            report.push(spin_target_diagnostic(
                target,
                DiagnosticCode::ESpinTargetUnsupported,
                "SpinTarget clear_lines cannot require more than four cleared lines from one placement",
                "clear_lines_exceeds_tetromino_capacity",
            ));
        }
        if mini_spin_quad_requirement_is_incompatible(target.spin_kind(), target.clear_lines()) {
            report.push(spin_target_diagnostic(
                target,
                DiagnosticCode::ESpinTargetUnsupported,
                "mini spin targets cannot require a four-line clear in the exact product contract",
                "mini_spin_quad_clear_incompatible",
            ));
        }
    }

    fn line_requirement_exceeds_piece_capacity(clear_lines: RequiredClearLines) -> bool {
        match clear_lines {
            RequiredClearLines::Any => false,
            RequiredClearLines::Exactly(lines) | RequiredClearLines::AtLeast(lines) => lines > 4,
        }
    }

    fn mini_spin_quad_requirement_is_incompatible(
        spin_kind: RequiredSpinKind,
        clear_lines: RequiredClearLines,
    ) -> bool {
        let mini_kind = matches!(
            spin_kind,
            RequiredSpinKind::MiniSpin
                | RequiredSpinKind::TSpinMini
                | RequiredSpinKind::AllSpinMini
        );
        let requires_quad = match clear_lines {
            RequiredClearLines::Any => false,
            RequiredClearLines::Exactly(lines) | RequiredClearLines::AtLeast(lines) => lines >= 4,
        };
        mini_kind && requires_quad
    }
}
mod context {
    use clearra_scoring::profile::ScoreProfileRegistry;

    use super::{SpinTargetCapability, SpinTargetValidationMode};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct SpinTargetValidationContext<'a> {
        capability: SpinTargetCapability,
        mode: SpinTargetValidationMode,
        pub(super) score_profiles: &'a ScoreProfileRegistry,
    }

    impl<'a> SpinTargetValidationContext<'a> {
        pub fn new(
            capability: SpinTargetCapability,
            mode: SpinTargetValidationMode,
            score_profiles: &'a ScoreProfileRegistry,
        ) -> Self {
            Self {
                capability,
                mode,
                score_profiles,
            }
        }
    }
    impl<'a> SpinTargetValidationContext<'a> {
        pub fn capability(self) -> SpinTargetCapability {
            self.capability
        }
    }
    impl<'a> SpinTargetValidationContext<'a> {
        pub fn mode(self) -> SpinTargetValidationMode {
            self.mode
        }
    }
}
mod diagnostic_builder {
    use super::*;

    pub(super) fn spin_target_diagnostic(
        target: &SpinTarget,
        code: DiagnosticCode,
        message: &'static str,
        reason: &'static str,
    ) -> Diagnostic {
        Diagnostic::new(code, message)
            .with_location(EvidenceLocation::new("spin_target"))
            .with_evidence(ValidationEvidence::new(
                "spin_target_id",
                target.id().as_str(),
            ))
            .with_evidence(ValidationEvidence::new(
                "spin_kind",
                spin_kind_label(target.spin_kind()),
            ))
            .with_evidence(ValidationEvidence::new(
                "clear_lines",
                clear_lines_label(target.clear_lines()),
            ))
            .with_evidence(ValidationEvidence::new("reason", reason))
    }

    fn spin_kind_label(spin_kind: RequiredSpinKind) -> String {
        match spin_kind {
            RequiredSpinKind::RegularSpin => "regular-spin".to_owned(),
            RequiredSpinKind::MiniSpin => "mini-spin".to_owned(),
            RequiredSpinKind::TSpin => "t-spin".to_owned(),
            RequiredSpinKind::TSpinMini => "t-spin-mini".to_owned(),
            RequiredSpinKind::AllSpin => "all-spin".to_owned(),
            RequiredSpinKind::AllSpinMini => "all-spin-mini".to_owned(),
            RequiredSpinKind::ProfileSpecific(id) => format!("profile-specific:{id}"),
        }
    }

    fn clear_lines_label(clear_lines: RequiredClearLines) -> String {
        match clear_lines {
            RequiredClearLines::Any => "any".to_owned(),
            RequiredClearLines::Exactly(lines) => format!("exactly:{lines}"),
            RequiredClearLines::AtLeast(lines) => format!("at-least:{lines}"),
        }
    }
}
mod exactness_validator {
    use super::diagnostic_builder::spin_target_diagnostic;
    use super::*;

    pub(super) fn validate_exactness_policy(
        target: &SpinTarget,
        context: SpinTargetValidationContext<'_>,
        report: &mut DiagnosticReport,
    ) {
        if context.mode().requires_exact()
            && !context.capability().exact_spin_classifier_available()
        {
            report.push(spin_target_diagnostic(
                target,
                DiagnosticCode::ESpinClassifierIncompatible,
                "exact SpinTarget queries require an exact spin classifier",
                "exact_spin_classifier_missing",
            ));
        }

        if requires_special_spin_profile(target.spin_kind())
            && !context.capability().special_spin_profile_verified()
        {
            let code = if context.mode().requires_exact() {
                DiagnosticCode::ESpinProfileUnverified
            } else {
                DiagnosticCode::WSpinClassificationEstimated
            };
            report.push(spin_target_diagnostic(
                target,
                code,
                "profile-specific SpinTarget classification requires a verified special spin profile for exact results",
                "special_spin_profile_unverified",
            ));
        }

        if context.mode().requires_exact()
            && requires_kick_evidence_for_exact(target.spin_kind())
            && !context.capability().kick_evidence_available()
        {
            report.push(spin_target_diagnostic(
                target,
                DiagnosticCode::ESpinKickEvidenceMissing,
                "exact profile-specific SpinTarget classification requires kick evidence",
                "spin_kick_evidence_missing",
            ));
        }
    }

    fn requires_special_spin_profile(spin_kind: RequiredSpinKind) -> bool {
        matches!(spin_kind, RequiredSpinKind::ProfileSpecific(_))
    }

    fn requires_kick_evidence_for_exact(spin_kind: RequiredSpinKind) -> bool {
        matches!(spin_kind, RequiredSpinKind::ProfileSpecific(_))
    }
}
mod score_profile_validator {
    use super::diagnostic_builder::spin_target_diagnostic;
    use super::*;

    pub(super) fn validate_required_score_profile(
        target: &SpinTarget,
        context: SpinTargetValidationContext<'_>,
        report: &mut DiagnosticReport,
    ) {
        if matches!(target.spin_kind(), RequiredSpinKind::ProfileSpecific(_))
            && target.required_score_profile().is_none()
        {
            report.push(
                spin_target_diagnostic(
                    target,
                    DiagnosticCode::EScoreProfileInvalid,
                    "profile-specific SpinTarget queries must name a required score profile",
                    "missing_required_score_profile",
                )
                .with_suggested_next_step(SuggestedNextStep::new(
                    "Attach required_score_profile to profile-specific spin targets.",
                )),
            );
        }

        if let Some(profile_id) = target.required_score_profile() {
            if context.score_profiles.get(profile_id).is_none() {
                report.push(
                    spin_target_diagnostic(
                        target,
                        DiagnosticCode::EScoreProfileInvalid,
                        "SpinTarget references an unknown required score profile",
                        "unknown_required_score_profile",
                    )
                    .with_evidence(ValidationEvidence::new(
                        "required_score_profile",
                        profile_id,
                    )),
                );
            }
        }
    }
}
mod validation_mode {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum SpinTargetValidationMode {
        Exact,
        EstimatedAllowed,
    }

    impl SpinTargetValidationMode {
        pub(super) fn requires_exact(self) -> bool {
            matches!(self, Self::Exact)
        }
    }
}
mod validator {
    use clearra_scoring::spin::SpinTarget;

    use crate::diagnostic::diagnostic_report::DiagnosticReport;

    use super::{
        classifier_capability_validator::validate_classifier_capability,
        clear_line_validator::validate_clear_line_compatibility,
        exactness_validator::validate_exactness_policy,
        score_profile_validator::validate_required_score_profile, SpinTargetValidationContext,
    };

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct SpinTargetValidator;

    impl SpinTargetValidator {
        pub fn validate(
            target: &SpinTarget,
            context: SpinTargetValidationContext<'_>,
        ) -> DiagnosticReport {
            let mut report = DiagnosticReport::new();
            validate_classifier_capability(target, context, &mut report);
            validate_required_score_profile(target, context, &mut report);
            validate_clear_line_compatibility(target, &mut report);
            validate_exactness_policy(target, context, &mut report);
            report
        }
    }

    pub fn validate_spin_target(
        target: &SpinTarget,
        context: SpinTargetValidationContext<'_>,
    ) -> DiagnosticReport {
        SpinTargetValidator::validate(target, context)
    }
}

pub use capability::SpinTargetCapability;
pub use context::SpinTargetValidationContext;
pub use validation_mode::SpinTargetValidationMode;
pub use validator::{validate_spin_target, SpinTargetValidator};

#[cfg(test)]
#[path = "spin_target_validator_tests.rs"]
mod tests;
