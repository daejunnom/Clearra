use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_spin::{
    LastActionEvidence, SpinAwardProfile, SpinEvidence, SpinInterpretation, SpinInterpretationSet,
};

use crate::profile::{AttackModelId, AttackProfile, ScoreModelId, ScoreProfile, SpinAwardPolicy};

use super::*;

#[test]
fn clearra_scoring_consumes_spin_interpretation_set() {
    let evidence = SpinEvidence::new(LastActionEvidence::new(PieceKind::T, true));
    let interpretations = SpinInterpretationSet::new([SpinInterpretation::RegularTSpin], evidence);
    let score_profile =
        ScoreProfile::new("score-only", "Score Only").with_score_model(ScoreModelId::Guideline);
    let attack_profile = AttackProfile::guideline();

    let score = SpinInterpretationEvaluator::evaluate(
        &score_profile,
        &attack_profile,
        &interpretations,
        2,
        false,
    );

    assert_eq!(score.score(), 1200);
    assert_eq!(score.attack(), 4);
    assert!(score.probability_complete());
}

#[test]
fn attack_profile_separate_from_score_profile() {
    let score_profile =
        ScoreProfile::new("score-only", "Score Only").with_score_model(ScoreModelId::Guideline);
    let attack_profile = AttackProfile::guideline();

    assert_eq!(score_profile.attack_model(), AttackModelId::Disabled);
    assert_eq!(attack_profile.attack_model(), AttackModelId::Guideline);
}

#[test]
fn spin_award_profile_separate_from_score_profile() {
    let evidence = SpinEvidence::new(LastActionEvidence::new(PieceKind::T, true));
    let interpretations = SpinInterpretationSet::new([SpinInterpretation::RegularTSpin], evidence);
    let score_profile = ScoreProfile::new("score-only", "Score Only")
        .with_score_model(ScoreModelId::Guideline)
        .with_spin_award_policy(SpinAwardPolicy::AllSpins);
    let attack_profile = AttackProfile::guideline();
    let award_profile = SpinAwardProfile::standard();

    let score = SpinInterpretationEvaluator::evaluate_with_award_profile(
        &score_profile,
        &attack_profile,
        &award_profile,
        &interpretations,
        2,
        false,
    );

    assert_eq!(score_profile.spin_award_policy(), SpinAwardPolicy::AllSpins);
    assert_eq!(award_profile.id().as_str(), "standard-spin-award");
    assert_eq!(score.score(), 1200);
}
