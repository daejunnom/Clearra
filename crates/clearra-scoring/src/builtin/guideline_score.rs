use crate::profile::{
    B2BPolicy, ComboPolicy, DropScorePolicy, LevelPolicy, ScoreModelId, ScoreProfile, SpinProfile,
    SpinProfileId, TraceRequirement,
};

pub fn guideline_score() -> ScoreProfile {
    guideline_score_with_spin_profile(SpinProfileId::TSpins)
}

pub fn guideline_score_with_spin_profile(spin_profile: SpinProfileId) -> ScoreProfile {
    guideline_profile(
        profile_id("guideline", spin_profile),
        format!("Guideline-compatible Level 1 ({})", spin_profile.as_str()),
        spin_profile,
    )
    .with_drop_score_policy(DropScorePolicy::HardDrop2SoftDrop1)
}

pub fn guideline_pc_score_with_spin_profile(spin_profile: SpinProfileId) -> ScoreProfile {
    guideline_profile(
        format!("guideline-pc-{}", spin_profile.as_str()),
        format!(
            "Guideline-compatible Level 1 PC scoring ({})",
            spin_profile.as_str()
        ),
        spin_profile,
    )
    .with_drop_score_policy(DropScorePolicy::Disabled)
    .with_trace_requirement(TraceRequirement::PlacementTrace)
}

fn guideline_profile(
    id: impl Into<String>,
    display_name: impl Into<String>,
    spin_profile: SpinProfileId,
) -> ScoreProfile {
    ScoreProfile::new(id, display_name)
        .with_score_model(ScoreModelId::Guideline)
        .with_spin_profile(SpinProfile::builtin(spin_profile))
        .with_combo_policy(ComboPolicy::linear(50, 0))
        .with_b2b_policy(B2BPolicy::multiplier(3, 2, 0))
        .with_level_policy(LevelPolicy::FixedLevelOne)
}

fn profile_id(base: &str, spin_profile: SpinProfileId) -> String {
    if spin_profile == SpinProfileId::TSpins {
        base.to_owned()
    } else {
        format!("{base}-{}", spin_profile.as_str())
    }
}
