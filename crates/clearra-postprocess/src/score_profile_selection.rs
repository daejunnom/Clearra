use clearra_objectives::policy::score_objective_policy::{
    ScoreObjectivePolicy, ScoreProfileSelection, SpinProfileSelection,
};
use clearra_scoring::{
    builtin::tetrio_pc_score_with_spin_profile,
    profile::{ScoreProfile, SpinProfileId},
};

pub(crate) fn score_profile(policy: ScoreObjectivePolicy) -> ScoreProfile {
    let spin_profile = match policy.spin_profile() {
        SpinProfileSelection::TSpins => SpinProfileId::TSpins,
        SpinProfileSelection::TSpinsPlus => SpinProfileId::TSpinsPlus,
        SpinProfileSelection::AllSpin => SpinProfileId::AllSpin,
        SpinProfileSelection::AllSpinPlus => SpinProfileId::AllSpinPlus,
        SpinProfileSelection::AllMini => SpinProfileId::AllMini,
        SpinProfileSelection::AllMiniPlus => SpinProfileId::AllMiniPlus,
    };
    match policy.profile() {
        ScoreProfileSelection::Tetrio => tetrio_pc_score_with_spin_profile(spin_profile),
    }
}
