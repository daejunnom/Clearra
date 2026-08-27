use clearra_scoring::profile::ScoreProfileRegistry;

use super::ScoreProfileEditorSchema;

#[test]
fn score_profile_editor_uses_canonical_registry_profiles() {
    let schema = ScoreProfileEditorSchema::mvp2();
    let registry = ScoreProfileRegistry::builtins();

    assert!(schema.enabled());
    assert_eq!(
        schema
            .profiles()
            .iter()
            .map(|profile| profile.value())
            .collect::<Vec<_>>(),
        registry
            .profiles()
            .iter()
            .map(|profile| profile.id())
            .collect::<Vec<_>>()
    );
}

#[test]
fn score_profile_editor_exposes_profile_attack_spin_combo_b2b_fields() {
    let schema = ScoreProfileEditorSchema::mvp2();

    assert!(schema.fields().iter().any(|field| field.key() == "id"));
    assert!(schema
        .fields()
        .iter()
        .any(|field| field.key() == "accuracy_level"));
    assert!(schema
        .fields()
        .iter()
        .any(|field| field.key() == "profile_specific_exact"));
    assert!(schema
        .fields()
        .iter()
        .any(|field| field.key() == "accuracy_reason"));
    assert!(schema
        .score_fields()
        .iter()
        .any(|field| field.key() == "score_model"));
    assert!(schema
        .score_fields()
        .iter()
        .any(|field| field.key() == "drop_score_policy"));
    assert!(schema
        .score_fields()
        .iter()
        .any(|field| field.key() == "level_policy"));
    assert!(schema
        .score_fields()
        .iter()
        .any(|field| field.key() == "pc_bonus_policy"));
    assert!(schema
        .attack_fields()
        .iter()
        .any(|field| field.key() == "attack_model"));
    assert!(schema
        .spin_fields()
        .iter()
        .any(|field| field.key() == "spin_rule"));
    assert!(schema
        .spin_fields()
        .iter()
        .any(|field| field.key() == "spin_award_policy"));
    assert!(schema.spin_fields()[0]
        .options()
        .iter()
        .any(|option| option == "t-spins"));
    assert!(schema
        .combo_fields()
        .iter()
        .any(|field| field.key() == "combo.attack_bonus_per_combo"));
    assert!(schema
        .b2b_fields()
        .iter()
        .any(|field| field.key() == "b2b.attack_bonus"));
    assert!(schema.import_export().import_json_enabled());
    assert!(schema.import_export().export_json_enabled());
    assert!(schema
        .result_contract_fields()
        .iter()
        .any(|field| field == "score_evaluation_basis"));
    assert!(schema
        .result_contract_fields()
        .iter()
        .any(|field| field == "score_accuracy_level"));
    assert!(schema
        .result_contract_fields()
        .iter()
        .any(|field| field == "score_event_basis"));
    assert!(schema
        .result_contract_fields()
        .iter()
        .any(|field| field == "score_evaluation_scope"));
    assert!(schema
        .result_contract_fields()
        .iter()
        .any(|field| field == "score_pattern_optimal_count"));
    assert!(!schema
        .result_contract_fields()
        .iter()
        .any(|field| field == "objective_best_score_by_pattern_count"));
}
