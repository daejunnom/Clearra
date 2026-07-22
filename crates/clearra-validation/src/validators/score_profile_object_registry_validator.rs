use clearra_scoring::{
    model::{AttackModelRegistry, ScoreModelRegistry},
    spin::SpinClassifierRegistry,
};

use crate::{
    diagnostic::{diagnostic_code::DiagnosticCode, diagnostic_report::DiagnosticReport},
    evidence::validation_evidence::ValidationEvidence,
};

use super::{
    score_profile_object_diagnostic_builder::score_object_diagnostic,
    score_profile_object_validator::ScoreProfileObjectDescriptor,
};

pub(crate) fn validate_model_registry_ids(
    object: &ScoreProfileObjectDescriptor,
    report: &mut DiagnosticReport,
) {
    if ScoreModelRegistry::parse(object.score_model_id()).is_none() {
        report.push(score_object_diagnostic(
            object,
            DiagnosticCode::EScoreProfileInvalid,
            "score profile object references an unknown score_model_id",
            "unknown_score_model_id",
        ));
    }

    if AttackModelRegistry::parse(object.attack_model_id()).is_none() {
        report.push(score_object_diagnostic(
            object,
            DiagnosticCode::EScoreProfileInvalid,
            "score profile object references an unknown attack_model_id",
            "unknown_attack_model_id",
        ));
    }
}

pub(crate) fn validate_spin_classifier_registry_id(
    object: &ScoreProfileObjectDescriptor,
    report: &mut DiagnosticReport,
) {
    if spin_classifier_id_known(object.spin_classifier_id()) {
        return;
    }

    report.push(
        score_object_diagnostic(
            object,
            DiagnosticCode::ESpinClassifierIncompatible,
            "score profile object references an unknown spin_classifier_id",
            "unknown_spin_classifier_id",
        )
        .with_evidence(ValidationEvidence::new(
            "spin_classifier_id",
            object.spin_classifier_id(),
        )),
    );
}

pub(crate) fn spin_classifier_supports_all_piece(id: &str) -> bool {
    SpinClassifierRegistry::supports_all_piece(id)
}

pub(crate) fn is_tetrio_style_default_profile(profile_id: &str) -> bool {
    matches!(
        normalize_id(profile_id).as_str(),
        "tetrio" | "tetrio-default"
    )
}

fn spin_classifier_id_known(id: &str) -> bool {
    SpinClassifierRegistry::get(id).is_some()
}

fn normalize_id(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}
