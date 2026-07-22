use super::*;

#[test]
fn imported_score_profile_json_roundtrips_profile_contract_values() {
    let json = r#"{
        "id": "custom-score",
        "display_name": "Custom Score",
        "score_model": "tetrio",
        "attack_model": "tetrio",
        "spin_rule": "t-spin-corner-based",
        "accuracy_level": "basic-approximation",
        "accuracy_reason": "profile-specific basic score/attack tables with configurable spin detection",
        "combo": {
            "enabled": true,
            "score_bonus_per_combo": 50,
            "attack_bonus_per_combo": 1
        },
        "b2b": {
            "enabled": true,
            "score_bonus": 200,
            "attack_bonus": 1
        }
    }"#;

    let profile = ScoreProfileImport::from_json(json).expect("valid score profile");

    assert_eq!(profile.id(), "custom-score");
    assert_eq!(profile.score_model(), ScoreModelId::Tetrio);
    assert_eq!(profile.attack_model(), AttackModelId::Tetrio);
    assert_eq!(profile.spin_rule(), SpinRuleId::TSpinCornerBased);
    assert_eq!(
        profile.accuracy_level(),
        ScoringAccuracyLevel::BasicApproximation
    );
    assert!(!profile.profile_specific_exact());
    assert_eq!(profile.combo_policy().attack_bonus_per_combo(), 1);
    assert_eq!(profile.b2b_policy().score_bonus(), 200);
}

#[test]
fn imported_score_profile_rejects_unknown_scoring_field() {
    let err = ScoreProfileImport::from_json(
        r#"{"id":"x","display_name":"X","score_model":"guideline","mystery":1}"#,
    )
    .expect_err("unknown field");

    assert_eq!(err.code(), "unknown_scoring_field");
}

#[test]
fn imported_score_profile_rejects_unsupported_spin_rule_and_invalid_policies() {
    let unsupported_spin = ScoreProfileImport::from_json(
        r#"{"id":"x","display_name":"X","spin_rule":"unknown-spin"}"#,
    )
    .expect_err("unsupported spin rule");
    let unsupported_accuracy = ScoreProfileImport::from_json(
        r#"{"id":"x","display_name":"X","attack_model":"guideline","accuracy_level":"profile-specific-exact"}"#,
    )
    .expect_err("unsupported exact accuracy level");
    let invalid_combo = ScoreProfileImport::from_json(
        r#"{"id":"x","display_name":"X","combo":{"enabled":false,"attack_bonus_per_combo":1}}"#,
    )
    .expect_err("invalid combo");
    let invalid_b2b = ScoreProfileImport::from_json(
        r#"{"id":"x","display_name":"X","b2b":{"enabled":false,"attack_bonus":1}}"#,
    )
    .expect_err("invalid b2b");

    assert_eq!(unsupported_spin.code(), "unsupported_spin_rule");
    assert_eq!(unsupported_accuracy.code(), "unsupported_accuracy_level");
    assert_eq!(invalid_combo.code(), "invalid_combo_setting");
    assert_eq!(invalid_b2b.code(), "invalid_b2b_setting");
}
