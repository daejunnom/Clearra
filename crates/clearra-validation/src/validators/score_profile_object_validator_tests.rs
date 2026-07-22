use clearra_scoring::{
    profile::{AllSpinScoreMapping, DropScorePolicy, ScoringAccuracyLevel, SpinAwardPolicy},
    spin::TraceCompleteness,
};

use crate::{
    diagnostic::diagnostic_code::DiagnosticCode,
    validators::score_profile_object_validator::{
        validate_score_profile_object, ScoreProfileObjectDescriptor,
    },
};

#[test]
fn score_profile_object_validator_rejects_all_spin_in_default_tetrio_profile() {
    let object = ScoreProfileObjectDescriptor::new("tetrio")
        .with_score_model_id("tetrio")
        .with_attack_model_id("tetrio")
        .with_spin_classifier_id("all-spin")
        .with_spin_award_policy(SpinAwardPolicy::AllSpins);

    let report = validate_score_profile_object(&object);

    assert!(report.contains_code(DiagnosticCode::EScoreProfileSpinPolicyIncompatible));
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "reason"
            && evidence.value() == "all_spin_default_forbidden_for_tetrio_score")));
}

#[test]
fn score_profile_object_validator_requires_trace_completeness_for_drop_score() {
    let object = ScoreProfileObjectDescriptor::new("custom-drop-score")
        .with_score_model_id("guideline")
        .with_attack_model_id("guideline")
        .with_spin_classifier_id("t-spin-corner-based")
        .with_drop_score_policy(DropScorePolicy::HardDrop2SoftDrop1)
        .with_trace_completeness(TraceCompleteness::RetainedSample);

    let report = validate_score_profile_object(&object);

    assert!(report.contains_code(DiagnosticCode::EScoreProfileSpinPolicyIncompatible));
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "reason"
            && evidence.value() == "drop_score_requires_trace_completeness")));
}

#[test]
fn score_profile_object_validator_rejects_unknown_field() {
    let object = ScoreProfileObjectDescriptor::new("custom")
        .with_score_model_id("guideline")
        .with_attack_model_id("guideline")
        .with_spin_classifier_id("t-spin-simple")
        .with_unknown_field("surprise");

    let report = validate_score_profile_object(&object);

    assert!(report.contains_code(DiagnosticCode::EScoreProfileInvalid));
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "unknown_field" && evidence.value() == "surprise")));
}

#[test]
fn score_profile_object_validator_rejects_unknown_score_model() {
    let object = ScoreProfileObjectDescriptor::new("custom")
        .with_score_model_id("mystery-score-model")
        .with_attack_model_id("guideline")
        .with_spin_classifier_id("t-spin-simple");

    let report = validate_score_profile_object(&object);

    assert!(report.contains_code(DiagnosticCode::EScoreProfileInvalid));
    assert!(report
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic
            .evidence()
            .iter()
            .any(|evidence| evidence.key() == "reason"
                && evidence.value() == "unknown_score_model_id")));
}

#[test]
fn score_profile_object_validator_rejects_exact_profile_with_basic_evaluator() {
    let object = ScoreProfileObjectDescriptor::new("custom-exact")
        .with_score_model_id("tetrio")
        .with_attack_model_id("tetrio")
        .with_spin_classifier_id("t-spin-corner-based")
        .with_accuracy_level(ScoringAccuracyLevel::ProfileSpecificExact)
        .with_exact_score_table_pinned(false)
        .with_exact_spin_classifier_available(false)
        .with_trace_completeness(TraceCompleteness::Full);

    let report = validate_score_profile_object(&object);

    assert!(report.contains_code(DiagnosticCode::EScoreProfileInvalid));
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "reason"
            && evidence.value() == "profile_specific_exact_requires_exact_basis")));
}

#[test]
fn tetrio_profile_reports_basic_approximation_until_exact() {
    let object = ScoreProfileObjectDescriptor::new("tetrio")
        .with_score_model_id("tetrio")
        .with_attack_model_id("tetrio")
        .with_spin_classifier_id("t-spin-corner-based")
        .with_accuracy_level(ScoringAccuracyLevel::BasicApproximation);

    let report = validate_score_profile_object(&object);

    assert!(!report.contains_code(DiagnosticCode::EScoreProfileInvalid));
}

#[test]
fn score_profile_object_validator_allows_custom_all_spin_options_with_classifier() {
    let all_spin = ScoreProfileObjectDescriptor::new("custom-all-spin")
        .with_score_model_id("tetrio")
        .with_attack_model_id("tetrio")
        .with_spin_classifier_id("all-spin")
        .with_spin_award_policy(SpinAwardPolicy::AllSpins)
        .with_all_spin_score_mapping(AllSpinScoreMapping::NativeAllSpinTable);
    let all_mini = ScoreProfileObjectDescriptor::new("custom-all-mini")
        .with_score_model_id("tetrio")
        .with_attack_model_id("tetrio")
        .with_spin_classifier_id("all-mini")
        .with_spin_award_policy(SpinAwardPolicy::AllMini);
    let all_spin_as_mini = ScoreProfileObjectDescriptor::new("custom-all-spin-as-t-spin-mini")
        .with_score_model_id("tetrio")
        .with_attack_model_id("tetrio")
        .with_spin_classifier_id("all-piece-spin")
        .with_spin_award_policy(SpinAwardPolicy::AllSpinAsTSpinMini)
        .with_all_spin_score_mapping(AllSpinScoreMapping::UseTSpinMiniTable);

    assert!(!validate_score_profile_object(&all_spin).has_errors());
    assert!(!validate_score_profile_object(&all_mini).has_errors());
    assert!(!validate_score_profile_object(&all_spin_as_mini).has_errors());
}

#[test]
fn score_profile_object_validator_rejects_all_spin_without_all_piece_classifier() {
    let object = ScoreProfileObjectDescriptor::new("custom-all-spin")
        .with_score_model_id("tetrio")
        .with_attack_model_id("tetrio")
        .with_spin_classifier_id("t-spin-corner-based")
        .with_spin_award_policy(SpinAwardPolicy::AllSpins);

    let report = validate_score_profile_object(&object);

    assert!(report.contains_code(DiagnosticCode::EScoreProfileSpinPolicyIncompatible));
    assert!(report.diagnostics().iter().any(|diagnostic| diagnostic
        .evidence()
        .iter()
        .any(|evidence| evidence.key() == "reason"
            && evidence.value() == "all_spin_requires_all_piece_classifier")));
}
