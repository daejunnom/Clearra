use crate::profile::{
    AttackModelId, B2BPolicy, ComboPolicy, DropScorePolicy, ScoreModelId, ScoreProfile,
    SpinProfile, SpinProfileId, TraceRequirement,
};

pub fn tetrio_score() -> ScoreProfile {
    tetrio_score_with_spin_profile(SpinProfileId::TSpins)
}

pub fn tetrio_pc_score() -> ScoreProfile {
    tetrio_pc_score_with_spin_profile(SpinProfileId::TSpins)
}

pub fn tetrio_score_with_spin_profile(spin_profile: SpinProfileId) -> ScoreProfile {
    let suffix = spin_profile.as_str();
    let id = if spin_profile == SpinProfileId::TSpins {
        "tetrio".to_owned()
    } else {
        format!("tetrio-{suffix}")
    };
    tetrio_profile(id, format!("TETR.IO ({suffix})"))
        .with_spin_profile(SpinProfile::builtin(spin_profile))
}

pub fn tetrio_pc_score_with_spin_profile(spin_profile: SpinProfileId) -> ScoreProfile {
    let suffix = spin_profile.as_str();
    tetrio_profile(
        format!("tetrio-pc-{suffix}"),
        format!("TETR.IO PC scoring ({suffix})"),
    )
    .with_spin_profile(SpinProfile::builtin(spin_profile))
    .with_drop_score_policy(DropScorePolicy::Disabled)
    .with_trace_requirement(TraceRequirement::PlacementTrace)
    .with_attack_model(AttackModelId::Disabled)
}

fn tetrio_profile(id: impl Into<String>, display_name: impl Into<String>) -> ScoreProfile {
    ScoreProfile::new(id, display_name)
        .with_score_model(ScoreModelId::Tetrio)
        .with_attack_model(AttackModelId::Tetrio)
        .with_spin_profile(SpinProfile::builtin(SpinProfileId::TSpins))
        .with_combo_policy(ComboPolicy::linear(50, 1))
        .with_b2b_policy(B2BPolicy::multiplier(3, 2, 1))
        .with_drop_score_policy(DropScorePolicy::HardDrop2SoftDrop1)
}
