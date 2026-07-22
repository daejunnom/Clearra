use crate::{
    diagnostic::{diagnostic::Diagnostic, diagnostic_code::DiagnosticCode},
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

use super::score_profile_object_validator::ScoreProfileObjectDescriptor;

pub(crate) fn score_object_diagnostic(
    object: &ScoreProfileObjectDescriptor,
    code: DiagnosticCode,
    message: &'static str,
    reason: &'static str,
) -> Diagnostic {
    Diagnostic::new(code, message)
        .with_location(EvidenceLocation::new("score_profile"))
        .with_evidence(ValidationEvidence::new("profile_id", object.profile_id()))
        .with_evidence(ValidationEvidence::new("reason", reason))
}
