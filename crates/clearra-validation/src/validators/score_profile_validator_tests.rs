use clearra_scoring::profile::{AttackModelId, ScoreProfile, ScoringAccuracyLevel};

use super::*;

#[test]
fn valid_score_profile_reports_mvp2_supported_info() {
    let profile =
        ScoreProfile::new("profile", "Profile").with_attack_model(AttackModelId::Guideline);

    let report = validate_score_profile(&profile);

    assert!(!report.has_errors());
    assert!(report.contains_code(DiagnosticCode::IScoreProfileMvp2Supported));
    assert!(report.diagnostics()[0]
        .message()
        .contains("basic-approximation"));
}

#[test]
fn score_profile_json_rejects_unknown_fields_and_unsupported_spin_rules() {
    let unknown_field = validate_score_profile_json(
        r#"{"id":"x","display_name":"X","attack_model":"guideline","extra":true}"#,
    );
    let unsupported_spin = validate_score_profile_json(
        r#"{"id":"x","display_name":"X","attack_model":"guideline","spin_rule":"unknown-spin"}"#,
    );
    let unsupported_accuracy = validate_score_profile_json(
        r#"{"id":"x","display_name":"X","attack_model":"guideline","accuracy_level":"profile-specific-exact"}"#,
    );

    assert!(unknown_field.contains_code(DiagnosticCode::EScoreProfileInvalid));
    assert!(unknown_field.diagnostics()[0]
        .message()
        .contains("unknown scoring field"));
    assert!(unsupported_spin.contains_code(DiagnosticCode::EScoreProfileInvalid));
    assert!(unsupported_spin.diagnostics()[0]
        .message()
        .contains("unsupported spin rule"));
    assert!(unsupported_accuracy.contains_code(DiagnosticCode::EScoreProfileInvalid));
    assert!(unsupported_accuracy.diagnostics()[0]
        .message()
        .contains("unsupported accuracy level"));
}

#[test]
fn score_profile_rejects_profile_specific_exact_until_exact_models_exist() {
    let profile = ScoreProfile::new("exact", "Exact")
        .with_attack_model(AttackModelId::Guideline)
        .with_accuracy(
            ScoringAccuracyLevel::ProfileSpecificExact,
            "not implemented",
        );

    let report = validate_score_profile(&profile);

    assert!(report.contains_code(DiagnosticCode::EScoreProfileInvalid));
    assert!(report.diagnostics()[0]
        .message()
        .contains("profile-specific exact scoring is not implemented"));
}

#[test]
fn score_profile_json_rejects_invalid_combo_and_b2b_settings() {
    let invalid_combo = validate_score_profile_json(
        r#"{"id":"x","display_name":"X","attack_model":"guideline","combo":{"enabled":false,"attack_bonus_per_combo":1}}"#,
    );
    let invalid_b2b = validate_score_profile_json(
        r#"{"id":"x","display_name":"X","attack_model":"guideline","b2b":{"enabled":false,"attack_bonus":1}}"#,
    );

    assert!(invalid_combo.diagnostics()[0]
        .message()
        .contains("invalid combo setting"));
    assert!(invalid_b2b.diagnostics()[0]
        .message()
        .contains("invalid B2B setting"));
}

#[test]
fn score_profile_json_rejects_unsupported_policy_settings() {
    let invalid_drop_policy = validate_score_profile_json(
        r#"{"id":"x","display_name":"X","attack_model":"guideline","drop_score_policy":"mystery"}"#,
    );

    assert!(invalid_drop_policy.contains_code(DiagnosticCode::EScoreProfileInvalid));
    assert!(invalid_drop_policy.diagnostics()[0]
        .message()
        .contains("unsupported policy setting"));
}
