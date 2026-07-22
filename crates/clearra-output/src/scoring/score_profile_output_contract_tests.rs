use super::*;

#[test]
fn tetrio_profile_reports_basic_approximation_until_exact() {
    let contract = ScoreProfileOutputContract::basic_approximation(
        "tetrio",
        "tetrio",
        "tetrio",
        "t-spins",
        "profile-specific exact fixtures are not complete",
    );

    assert_eq!(contract.score_profile_id(), "tetrio");
    assert_eq!(contract.score_model_id(), "tetrio");
    assert_eq!(contract.attack_model_id(), "tetrio");
    assert_eq!(contract.spin_rule_id(), "t-spins");
    assert_eq!(contract.accuracy_level(), "basic-approximation");
    assert!(!contract.profile_specific_exact());
    assert!(!contract.exact_claim_allowed());
}

#[test]
fn score_profile_reports_accuracy_level() {
    let contract = ScoreProfileOutputContract::basic_approximation(
        "tetrio",
        "tetrio",
        "tetrio",
        "t-spins",
        "profile-specific exact fixtures are not complete",
    );

    assert_eq!(contract.accuracy_level(), "basic-approximation");
    assert_eq!(
        contract.accuracy_reason(),
        "profile-specific exact fixtures are not complete"
    );
    assert!(!contract.profile_specific_exact());
}

#[test]
fn tetrio_not_profile_specific_exact_until_exact_supported() {
    tetrio_profile_reports_basic_approximation_until_exact();
}

#[test]
fn score_profile_output_contract_exposes_object_policy_fields() {
    let contract = ScoreProfileOutputContract::new(
        "custom",
        "guideline",
        "guideline",
        "t-spin-simple",
        "t-spins-only",
        "hard-drop-2-soft-drop-1",
        "fixed-level-one",
        "linear",
        "standard",
        "disabled",
        "profile-specific-exact",
        "exact fixtures passed",
        "full-drop-trace",
        true,
    );

    assert_eq!(contract.spin_award_policy(), "t-spins-only");
    assert_eq!(contract.drop_score_policy(), "hard-drop-2-soft-drop-1");
    assert_eq!(contract.level_policy(), "fixed-level-one");
    assert_eq!(contract.combo_policy(), "linear");
    assert_eq!(contract.b2b_policy(), "standard");
    assert_eq!(contract.pc_bonus_policy(), "disabled");
    assert_eq!(contract.trace_requirement(), "full-drop-trace");
    assert_eq!(contract.accuracy_reason(), "exact fixtures passed");
    assert!(contract.exact_claim_allowed());
}
