use clearra_objectives::policy::score_objective_policy::{
    ScoreObjectivePolicy, ScoreProfileSelection, SpinProfileSelection,
};
use clearra_scoring::{
    builtin::{
        guideline_pc_score_with_spin_profile, jstris_ultra_pc_score_with_spin_profile,
        tetrio_pc_score_with_spin_profile,
    },
    profile::{ScoreProfile, SpinProfileId},
};

pub(crate) fn score_profile(policy: ScoreObjectivePolicy) -> ScoreProfile {
    let spin_profile = spin_profile_id(policy.spin_profile());
    match policy.profile() {
        ScoreProfileSelection::Guideline => guideline_pc_score_with_spin_profile(spin_profile),
        ScoreProfileSelection::JstrisUltra => jstris_ultra_pc_score_with_spin_profile(spin_profile),
        ScoreProfileSelection::Tetrio => tetrio_pc_score_with_spin_profile(spin_profile),
    }
}

pub(crate) const fn spin_profile_id(profile: SpinProfileSelection) -> SpinProfileId {
    match profile {
        SpinProfileSelection::TSpins => SpinProfileId::TSpins,
        SpinProfileSelection::TSpinsPlus => SpinProfileId::TSpinsPlus,
        SpinProfileSelection::AllSpin => SpinProfileId::AllSpin,
        SpinProfileSelection::AllSpinPlus => SpinProfileId::AllSpinPlus,
        SpinProfileSelection::AllMini => SpinProfileId::AllMini,
        SpinProfileSelection::AllMiniPlus => SpinProfileId::AllMiniPlus,
    }
}
