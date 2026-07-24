use crate::profile::{
    B2BPolicy, ComboPolicy, DropScorePolicy, ScoreModelId, ScoreProfile, SpinProfile,
    SpinProfileId, TraceRequirement,
};

pub fn jstris_ultra() -> ScoreProfile {
    jstris_ultra_with_spin_profile(SpinProfileId::TSpins)
}

pub fn jstris_ultra_with_spin_profile(spin_profile: SpinProfileId) -> ScoreProfile {
    jstris_profile(
        profile_id("jstris-ultra", spin_profile),
        format!("Jstris Ultra ({})", spin_profile.as_str()),
        spin_profile,
    )
}

pub fn jstris_ultra_pc_score_with_spin_profile(spin_profile: SpinProfileId) -> ScoreProfile {
    jstris_profile(
        format!("jstris-ultra-pc-{}", spin_profile.as_str()),
        format!("Jstris Ultra PC scoring ({})", spin_profile.as_str()),
        spin_profile,
    )
    .with_drop_score_policy(DropScorePolicy::Disabled)
    .with_trace_requirement(TraceRequirement::PlacementTrace)
}

fn jstris_profile(
    id: impl Into<String>,
    display_name: impl Into<String>,
    spin_profile: SpinProfileId,
) -> ScoreProfile {
    ScoreProfile::new(id, display_name)
        .with_score_model(ScoreModelId::JstrisUltra)
        .with_spin_profile(SpinProfile::builtin(spin_profile))
        .with_combo_policy(ComboPolicy::linear(50, 0))
        .with_b2b_policy(B2BPolicy::multiplier(3, 2, 0))
}

fn profile_id(base: &str, spin_profile: SpinProfileId) -> String {
    if spin_profile == SpinProfileId::TSpins {
        base.to_owned()
    } else {
        format!("{base}-{}", spin_profile.as_str())
    }
}
