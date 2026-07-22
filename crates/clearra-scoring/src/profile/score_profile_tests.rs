use super::*;

#[test]
fn profile_ids_and_models_are_stable_contracts() {
    let profile = ScoreProfile::new("custom", "Custom")
        .with_score_model(ScoreModelId::Tetrio)
        .with_attack_model(AttackModelId::Tetrio)
        .with_spin_rule(SpinRuleId::TSpinCornerBased)
        .with_combo_policy(ComboPolicy::linear(50, 1))
        .with_b2b_policy(B2BPolicy::standard(200, 1))
        .with_spin_award_policy(SpinAwardPolicy::TSpinsOnly)
        .with_all_spin_score_mapping(AllSpinScoreMapping::Disabled)
        .with_drop_score_policy(DropScorePolicy::HardDrop2SoftDrop1);

    assert_eq!(profile.id(), "custom");
    assert!(profile.score_enabled());
    assert!(profile.attack_enabled());
    assert_eq!(profile.score_model().as_str(), "tetrio");
    assert_eq!(profile.score_model_id().as_str(), "tetrio");
    assert_eq!(profile.attack_model().as_str(), "tetrio");
    assert_eq!(profile.attack_model_id().as_str(), "tetrio");
    assert_eq!(profile.spin_rule().as_str(), "t-spins");
    assert_eq!(profile.spin_rule_id().as_str(), "t-spins");
    assert_eq!(profile.accuracy_level().as_str(), "basic-approximation");
    assert!(!profile.profile_specific_exact());
    assert!(profile.accuracy_reason().contains("configurable spin"));
    assert_eq!(profile.combo_policy().score_bonus_per_combo(), 50);
    assert_eq!(profile.b2b_policy().attack_bonus(), 1);
    assert_eq!(profile.spin_award_policy(), SpinAwardPolicy::TSpinsOnly);
    assert_eq!(
        profile.all_spin_score_mapping(),
        AllSpinScoreMapping::Disabled
    );
    assert_eq!(
        profile.drop_score_policy(),
        DropScorePolicy::HardDrop2SoftDrop1
    );
    assert_eq!(profile.level_policy(), LevelPolicy::Disabled);
    assert_eq!(profile.pc_bonus_policy(), PcBonusPolicy::Disabled);
    assert_eq!(profile.trace_requirement(), TraceRequirement::FullDropTrace);
}

#[test]
fn scoring_accuracy_level_parses_stable_contract_strings() {
    assert_eq!(
        ScoringAccuracyLevel::parse("basic-approximation"),
        Some(ScoringAccuracyLevel::BasicApproximation)
    );
    assert_eq!(
        ScoringAccuracyLevel::parse("profile-specific-exact"),
        Some(ScoringAccuracyLevel::ProfileSpecificExact)
    );
    assert_eq!(
        ScoringAccuracyLevel::parse("unsupported"),
        Some(ScoringAccuracyLevel::Unsupported)
    );
    assert_eq!(
        ScoringAccuracyLevel::parse("insufficient-trace"),
        Some(ScoringAccuracyLevel::InsufficientTrace)
    );
    assert_eq!(
        SpinRuleId::parse("t-spin-3-corner"),
        Some(SpinRuleId::TSpinCornerBased)
    );
}
