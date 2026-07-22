use crate::{
    event::{clear_event::ClearEvent, score_event::ScoreEvent},
    profile::{AttackModelId, ScoreProfile},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AttackModelEvaluator;

impl AttackModelEvaluator {
    pub fn evaluate(clear: ClearEvent) -> u32 {
        Self::line_attack(AttackModelId::Guideline, clear)
    }
}
impl AttackModelEvaluator {
    pub fn evaluate_event(profile: &ScoreProfile, event: ScoreEvent) -> u32 {
        let line_attack = Self::line_attack(profile.attack_model(), event.clear());
        let combo_bonus = combo_attack_bonus(profile, event);
        let b2b_bonus =
            if profile.b2b_policy().enabled() && event.b2b_before() && event.is_difficult_clear() {
                profile.b2b_policy().attack_bonus()
            } else {
                0
            };

        line_attack
            .saturating_add(combo_bonus)
            .saturating_add(b2b_bonus)
    }
}
impl AttackModelEvaluator {
    fn line_attack(model: AttackModelId, clear: ClearEvent) -> u32 {
        if model == AttackModelId::Disabled {
            return 0;
        }

        let line_attack = match clear.lines() {
            0 | 1 => 0,
            2 => 1,
            3 => 2,
            _ => 4,
        };
        if clear.is_perfect_clear() {
            line_attack + 10
        } else {
            line_attack
        }
    }
}

fn combo_attack_bonus(profile: &ScoreProfile, event: ScoreEvent) -> u32 {
    let policy = profile.combo_policy();
    if !policy.enabled() || event.clear().lines() == 0 {
        return 0;
    }
    u32::from(event.combo_after().combo().saturating_sub(1))
        .saturating_mul(policy.attack_bonus_per_combo())
}

#[cfg(test)]
mod tests {
    use crate::profile::{AttackModelId, ComboPolicy, ScoreProfile};
    use crate::state::ComboState;

    use super::*;

    #[test]
    fn attack_model_adds_pc_combo_and_b2b_bonuses_from_score_event() {
        let profile = ScoreProfile::new("test", "Test")
            .with_attack_model(AttackModelId::Guideline)
            .with_combo_policy(ComboPolicy::linear(0, 1))
            .with_b2b_policy(crate::profile::B2BPolicy::standard(0, 1));
        let event = ScoreEvent::new(
            0,
            ClearEvent::new(4, true),
            None,
            ComboState::new(1),
            ComboState::new(2),
            true,
            true,
        );

        assert_eq!(AttackModelEvaluator::evaluate_event(&profile, event), 16);
    }
}
