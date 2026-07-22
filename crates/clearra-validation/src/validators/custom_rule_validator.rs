use clearra_rules::custom_rule::{
    CustomRuleEditorDraft, CustomRuleEditorSchema, CustomRuleSearchCapabilityReport,
    VerifiedCustomRuleProfile,
};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomRuleValidationResult {
    report: DiagnosticReport,
    verified_profile: Option<VerifiedCustomRuleProfile>,
    search_capability_report: Option<CustomRuleSearchCapabilityReport>,
}

impl CustomRuleValidationResult {
    pub fn new(
        report: DiagnosticReport,
        verified_profile: Option<VerifiedCustomRuleProfile>,
        search_capability_report: Option<CustomRuleSearchCapabilityReport>,
    ) -> Self {
        Self {
            report,
            verified_profile,
            search_capability_report,
        }
    }
}
impl CustomRuleValidationResult {
    pub fn report(&self) -> &DiagnosticReport {
        &self.report
    }
}
impl CustomRuleValidationResult {
    pub fn into_report(self) -> DiagnosticReport {
        self.report
    }
}
impl CustomRuleValidationResult {
    pub fn verified_profile(&self) -> Option<&VerifiedCustomRuleProfile> {
        self.verified_profile.as_ref()
    }
}
impl CustomRuleValidationResult {
    pub fn search_capability_report(&self) -> Option<&CustomRuleSearchCapabilityReport> {
        self.search_capability_report.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CustomRuleValidator;

impl CustomRuleValidator {
    pub fn validate_editor_schema(schema: CustomRuleEditorSchema) -> CustomRuleValidationResult {
        match VerifiedCustomRuleProfile::try_from_editor_schema(schema) {
            Ok(verified_profile) => Self::verified_result(verified_profile),
            Err(verification_report) => Self::invalid_result(verification_report),
        }
    }
}
impl CustomRuleValidator {
    pub fn validate_editor_draft(draft: CustomRuleEditorDraft) -> CustomRuleValidationResult {
        match VerifiedCustomRuleProfile::try_from_editor_draft(draft) {
            Ok(verified_profile) => Self::verified_result(verified_profile),
            Err(verification_report) => Self::invalid_result(verification_report),
        }
    }
}
impl CustomRuleValidator {
    fn verified_result(verified_profile: VerifiedCustomRuleProfile) -> CustomRuleValidationResult {
        let capability = verified_profile.search_capability_report();
        let mut report = DiagnosticReport::new();
        report.push(
            Diagnostic::new(
                DiagnosticCode::ICustomRuleVerified,
                "custom rule editor schema produced a verified rule profile; search capability remains explicit",
            )
            .with_location(EvidenceLocation::new("rules.custom_rule_editor"))
            .with_evidence(ValidationEvidence::new("verified_rule_profile", "true"))
            .with_evidence(ValidationEvidence::new(
                "c_compact_descriptor_ready",
                capability.c_compact_descriptor_ready().to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "search_backend_supported",
                capability.search_backend_supported().to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "unsupported_reason",
                capability
                    .unsupported_reason()
                    .unwrap_or("search_backend_supported"),
            ))
            .with_evidence(ValidationEvidence::new(
                "supports_180",
                capability.supports_180().to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "requires_spawn_reachability",
                capability.requires_spawn_reachability().to_string(),
            )),
        );
        CustomRuleValidationResult::new(report, Some(verified_profile), Some(capability))
    }
}
impl CustomRuleValidator {
    fn invalid_result(
        verification_report: clearra_rules::custom_rule::CustomRuleVerificationReport,
    ) -> CustomRuleValidationResult {
        let mut report = DiagnosticReport::new();
        let mut diagnostic = Diagnostic::new(
            DiagnosticCode::ECustomRuleInvalid,
            "custom rule editor schema must be validated before it can become a verified rule profile",
        )
        .with_location(EvidenceLocation::new("rules.custom_rule_editor"))
        .with_evidence(ValidationEvidence::new("verified_rule_profile", "false"))
        .with_evidence(ValidationEvidence::new(
            "issue_count",
            verification_report.issue_count().to_string(),
        ))
        .with_evidence(ValidationEvidence::new(
            "kick_issue_count",
            verification_report.kick_report().issue_count().to_string(),
        ))
        .with_evidence(ValidationEvidence::new(
            "missing_transition",
            verification_report.missing_transition().to_string(),
        ))
        .with_evidence(ValidationEvidence::new(
            "duplicate_transition",
            verification_report.duplicate_transition().to_string(),
        ))
        .with_evidence(ValidationEvidence::new(
            "invalid_rotation",
            verification_report.invalid_rotation().to_string(),
        ))
        .with_evidence(ValidationEvidence::new(
            "unsupported_piece",
            verification_report.unsupported_piece().to_string(),
        ))
        .with_evidence(ValidationEvidence::new(
            "unsupported_board_backend",
            verification_report.unsupported_board_backend().to_string(),
        ))
        .with_evidence(ValidationEvidence::new(
            "unsupported_runtime_feature",
            verification_report.unsupported_runtime_feature().to_string(),
        ));
        for error in verification_report.errors() {
            diagnostic = diagnostic.with_evidence(ValidationEvidence::new("reason", error.code()));
        }
        report.push(diagnostic.with_suggested_next_step(SuggestedNextStep::new(
            "Fix the raw editor schema and rerun validation; search accepts only verified rule profiles.",
        )));
        CustomRuleValidationResult::new(report, None, None)
    }
}

pub fn validate_custom_rule_editor_draft(
    draft: CustomRuleEditorDraft,
) -> CustomRuleValidationResult {
    CustomRuleValidator::validate_editor_draft(draft)
}

pub fn validate_custom_rule_editor_schema(
    schema: CustomRuleEditorSchema,
) -> CustomRuleValidationResult {
    CustomRuleValidator::validate_editor_schema(schema)
}

#[cfg(test)]
#[path = "custom_rule_validator_tests.rs"]
mod tests;
