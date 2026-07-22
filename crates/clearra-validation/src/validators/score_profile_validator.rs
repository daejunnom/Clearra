use clearra_scoring::{
    import::{ScoreProfileImport, ScoreProfileImportError},
    profile::ScoreProfile,
};

use crate::diagnostic::{
    diagnostic::Diagnostic, diagnostic_code::DiagnosticCode, diagnostic_report::DiagnosticReport,
};

pub fn validate_score_profile(profile: &ScoreProfile) -> DiagnosticReport {
    let mut report = DiagnosticReport::new();
    if profile.id().trim().is_empty() {
        report.push(Diagnostic::new(
            DiagnosticCode::EScoreProfileInvalid,
            "score profile id must not be empty",
        ));
    }
    if profile.display_name().trim().is_empty() {
        report.push(Diagnostic::new(
            DiagnosticCode::EScoreProfileInvalid,
            "score profile display_name must not be empty",
        ));
    }
    if !profile.score_enabled() && !profile.attack_enabled() {
        report.push(Diagnostic::new(
            DiagnosticCode::EScoreProfileInvalid,
            "score profile must enable at least one score or attack model",
        ));
    }
    if profile.profile_specific_exact() {
        report.push(Diagnostic::new(
            DiagnosticCode::EScoreProfileInvalid,
            "profile-specific exact scoring is not implemented by the MVP2 scoring layer",
        ));
    }
    if !report.has_errors() {
        report.push(Diagnostic::new(
            DiagnosticCode::IScoreProfileMvp2Supported,
            format!(
                "score profile is supported by the MVP2 post-processing scoring layer as {} ({})",
                profile.accuracy_level().as_str(),
                profile.accuracy_reason()
            ),
        ));
    }
    report
}

pub fn validate_score_profile_json(input: &str) -> DiagnosticReport {
    match ScoreProfileImport::from_json(input) {
        Ok(profile) => validate_score_profile(&profile),
        Err(error) => {
            let mut report = DiagnosticReport::new();
            report.push(Diagnostic::new(
                DiagnosticCode::EScoreProfileInvalid,
                import_error_message(&error),
            ));
            report
        }
    }
}

fn import_error_message(error: &ScoreProfileImportError) -> String {
    match error {
        ScoreProfileImportError::InvalidJson => "score profile JSON is invalid".to_owned(),
        ScoreProfileImportError::MissingField(field) => {
            format!("score profile JSON is missing required field '{field}'")
        }
        ScoreProfileImportError::UnknownScoringField(field) => {
            format!("score profile JSON contains unknown scoring field '{field}'")
        }
        ScoreProfileImportError::UnknownScoreModel(model) => {
            format!("score profile references unknown score model '{model}'")
        }
        ScoreProfileImportError::UnknownAttackModel(model) => {
            format!("score profile references unknown attack model '{model}'")
        }
        ScoreProfileImportError::UnsupportedSpinRule(rule) => {
            format!("score profile references unsupported spin rule '{rule}'")
        }
        ScoreProfileImportError::UnsupportedAccuracyLevel(level) => {
            format!("score profile references unsupported accuracy level '{level}'")
        }
        ScoreProfileImportError::InvalidComboSetting(reason) => {
            format!("score profile has invalid combo setting: {reason}")
        }
        ScoreProfileImportError::InvalidB2BSetting(reason) => {
            format!("score profile has invalid B2B setting: {reason}")
        }
        ScoreProfileImportError::UnsupportedPolicySetting(setting) => {
            format!("score profile references unsupported policy setting '{setting}'")
        }
    }
}

#[cfg(test)]
#[path = "score_profile_validator_tests.rs"]
mod tests;
