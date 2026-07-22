use crate::profile::{B2BPolicy, ComboPolicy, ScoreModelId, ScoreProfile, SpinRuleId};

pub fn jstris_ultra() -> ScoreProfile {
    ScoreProfile::new("jstris-ultra", "Jstris Ultra")
        .with_score_model(ScoreModelId::JstrisUltra)
        .with_spin_rule(SpinRuleId::TSpinCornerBased)
        .with_combo_policy(ComboPolicy::linear(50, 0))
        .with_b2b_policy(B2BPolicy::standard(200, 0))
}
