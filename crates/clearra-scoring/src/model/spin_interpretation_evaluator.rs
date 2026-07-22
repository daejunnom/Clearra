use clearra_spin::{SpinAwardProfile, SpinInterpretationSet};

use crate::{
    model::{score_table::ScoreModelTable, spin_interpretation_score::SpinInterpretationScore},
    profile::{AttackProfile, ScoreProfile},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpinInterpretationEvaluator;

impl SpinInterpretationEvaluator {
    pub fn evaluate(
        profile: &ScoreProfile,
        attack_profile: &AttackProfile,
        interpretations: &SpinInterpretationSet,
        cleared_lines: u8,
        perfect_clear: bool,
    ) -> SpinInterpretationScore {
        let award_profile = profile.spin_profile().award_profile();
        Self::evaluate_with_award_profile(
            profile,
            attack_profile,
            &award_profile,
            interpretations,
            cleared_lines,
            perfect_clear,
        )
    }
}
impl SpinInterpretationEvaluator {
    pub fn evaluate_with_award_profile(
        profile: &ScoreProfile,
        attack_profile: &AttackProfile,
        award_profile: &SpinAwardProfile,
        interpretations: &SpinInterpretationSet,
        cleared_lines: u8,
        perfect_clear: bool,
    ) -> SpinInterpretationScore {
        let score_table = ScoreModelTable::for_model(profile.score_model());
        let probability_complete = !interpretations.contains_unknown();
        let mut best = SpinInterpretationScore::new(0, 0, probability_complete);

        for interpretation in &interpretations.interpretations {
            let award_class = award_profile.award_class(interpretation);
            let score = score_table.map_or(0, |table| {
                table.score_award_class(award_class, cleared_lines)
            });
            let attack = attack_profile.attack_for_award(award_class, cleared_lines, perfect_clear);
            if score > best.score() || (score == best.score() && attack > best.attack()) {
                best = SpinInterpretationScore::new(score, attack, probability_complete);
            }
        }

        best
    }
}

#[cfg(test)]
#[path = "spin_interpretation_evaluator_tests.rs"]
mod tests;
