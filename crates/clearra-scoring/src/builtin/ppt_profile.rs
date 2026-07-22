use crate::profile::{AttackModelId, B2BPolicy, ComboPolicy, ScoreProfile, SpinRuleId};

pub fn ppt_profile() -> ScoreProfile {
    ScoreProfile::new("ppt", "Puyo Puyo Tetris")
        .with_attack_model(AttackModelId::Ppt)
        .with_spin_rule(SpinRuleId::TSpinCornerBased)
        .with_combo_policy(ComboPolicy::linear(0, 1))
        .with_b2b_policy(B2BPolicy::standard(0, 1))
}
