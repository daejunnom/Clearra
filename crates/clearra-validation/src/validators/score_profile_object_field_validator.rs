use crate::{
    diagnostic::{diagnostic_code::DiagnosticCode, diagnostic_report::DiagnosticReport},
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

use super::{
    score_profile_object_diagnostic_builder::score_object_diagnostic,
    score_profile_object_validator::ScoreProfileObjectDescriptor,
};

pub(crate) fn validate_unknown_fields(
    object: &ScoreProfileObjectDescriptor,
    report: &mut DiagnosticReport,
) {
    for field in object.unknown_fields() {
        report.push(
            score_object_diagnostic(
                object,
                DiagnosticCode::EScoreProfileInvalid,
                "score profile object contains an unknown field",
                "unknown_field",
            )
            .with_location(EvidenceLocation::new(format!("score_profile.{field}")))
            .with_evidence(ValidationEvidence::new("unknown_field", field)),
        );
    }
}
