use clearra_scoring::profile::{
    DropScorePolicy, ScoreAccuracy, ScoreProfile, ScoreProfileObjectValidatorBasis,
    ScoreProfileObjectValidatorError, ScoringAccuracyLevel, SpinAwardPolicy,
};

#[test]
fn validator_rejects_exact_profile_with_basic_evaluator() {
    let profile = ScoreProfile::new("exact", "Exact").with_accuracy(
        ScoringAccuracyLevel::ProfileSpecificExact,
        "exact requested",
    );
    let validator = ScoreProfileObjectValidatorBasis::new(ScoreAccuracy::PlacementOnlyEstimate);

    assert_eq!(
        validator.validate(&profile),
        Err(ScoreProfileObjectValidatorError::ExactProfileWithBasicEvaluator)
    );
}

#[test]
fn validator_requires_trace_completeness_for_drop_score() {
    let profile = ScoreProfile::new("drop", "Drop");
    let validator = ScoreProfileObjectValidatorBasis::new(ScoreAccuracy::PatternComplete)
        .with_drop_score_policy(DropScorePolicy::HardDrop2SoftDrop1);

    assert_eq!(
        validator.validate(&profile),
        Err(ScoreProfileObjectValidatorError::DropScoreRequiresTraceCompleteness)
    );
}

#[test]
fn validator_rejects_all_spin_policy_without_classifier_basis() {
    let profile = ScoreProfile::new("all-spin", "All Spin");
    let validator = ScoreProfileObjectValidatorBasis::new(ScoreAccuracy::Incomplete)
        .with_spin_award_policy(SpinAwardPolicy::AllSpins)
        .requiring_trace_completeness(true);

    assert_eq!(
        validator.validate(&profile),
        Err(ScoreProfileObjectValidatorError::AllSpinPolicyWithoutClassifier)
    );
}

#[test]
fn score_profile_object_validator() {
    let exact_profile = ScoreProfile::new("exact", "Exact").with_accuracy(
        ScoringAccuracyLevel::ProfileSpecificExact,
        "exact requested",
    );
    let basic_validator =
        ScoreProfileObjectValidatorBasis::new(ScoreAccuracy::PlacementOnlyEstimate);

    assert_eq!(
        basic_validator.validate(&exact_profile),
        Err(ScoreProfileObjectValidatorError::ExactProfileWithBasicEvaluator)
    );

    let drop_score_validator =
        ScoreProfileObjectValidatorBasis::new(ScoreAccuracy::PatternComplete)
            .with_drop_score_policy(DropScorePolicy::HardDrop2SoftDrop1);

    assert_eq!(
        drop_score_validator.validate(&ScoreProfile::new("drop", "Drop")),
        Err(ScoreProfileObjectValidatorError::DropScoreRequiresTraceCompleteness)
    );

    let all_spin_validator = ScoreProfileObjectValidatorBasis::new(ScoreAccuracy::Incomplete)
        .with_spin_award_policy(SpinAwardPolicy::AllSpins)
        .requiring_trace_completeness(true);

    assert_eq!(
        all_spin_validator.validate(&ScoreProfile::new("all-spin", "All Spin")),
        Err(ScoreProfileObjectValidatorError::AllSpinPolicyWithoutClassifier)
    );
}
