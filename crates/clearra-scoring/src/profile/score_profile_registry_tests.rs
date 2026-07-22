use super::*;
use crate::{
    model::{AttackModelRegistry, ScoreModelRegistry},
    spin::SpinClassifierRegistry,
};

#[test]
fn builtin_profiles_disclose_basic_approximation_accuracy() {
    let registry = ScoreProfileRegistry::builtins();

    for profile in registry.profiles() {
        assert_eq!(profile.accuracy_level().as_str(), "basic-approximation");
        assert!(!profile.profile_specific_exact());
        assert!(profile.accuracy_reason().contains("configurable spin"));
    }
}

#[test]
fn tetrio_default_and_all_spin_options_are_selectable() {
    let registry = ScoreProfileRegistry::builtins();
    let default = registry.get("tetrio").expect("default tetrio");
    let all_spin = registry.get("tetrio-all-spin").expect("all-spin tetrio");
    let all_spin_plus = registry
        .get("tetrio-all-spin-plus")
        .expect("all-spin-plus tetrio");
    let all_mini = registry.get("tetrio-all-mini").expect("all-mini tetrio");
    let all_mini_plus = registry
        .get("tetrio-all-mini-plus")
        .expect("all-mini-plus tetrio");

    assert_eq!(default.spin_award_policy().as_str(), "t-spins-only");
    assert_eq!(default.all_spin_score_mapping().as_str(), "disabled");
    assert_eq!(
        default.drop_score_policy().as_str(),
        "hard-drop-2-soft-drop-1"
    );
    assert_eq!(
        all_spin.spin_award_policy().as_str(),
        "all-spin-as-t-spin-mini"
    );
    assert_eq!(
        all_spin_plus.spin_award_policy().as_str(),
        "all-spin-as-t-spin-mini"
    );
    assert_eq!(all_mini.spin_award_policy().as_str(), "all-mini");
    assert_eq!(all_mini_plus.spin_award_policy().as_str(), "all-mini");
    for profile in [all_spin, all_spin_plus, all_mini, all_mini_plus] {
        assert_eq!(
            profile.all_spin_score_mapping().as_str(),
            "use-t-spin-mini-table"
        );
    }
}

#[test]
fn tetrio_profile_disables_all_spin_by_default() {
    let registry = ScoreProfileRegistry::builtins();
    let default = registry.get("tetrio").expect("default tetrio");

    assert_eq!(default.spin_award_policy().as_str(), "t-spins-only");
    assert_eq!(default.all_spin_score_mapping().as_str(), "disabled");
}

#[test]
fn all_spin_policy_not_enabled_in_default_profile() {
    let registry = ScoreProfileRegistry::builtins();
    let default = registry.get("tetrio").expect("default tetrio");

    assert!(!default.spin_award_policy().allows_all_spins());
    assert_eq!(default.all_spin_score_mapping().as_str(), "disabled");
}

#[test]
fn score_profile_registry_is_composed_from_model_and_classifier_registries() {
    let registry = ScoreProfileRegistry::builtins();

    for profile in registry.profiles() {
        assert!(ScoreModelRegistry::get(profile.score_model()).is_some());
        assert!(AttackModelRegistry::get(profile.attack_model()).is_some());
        assert!(SpinClassifierRegistry::get(profile.spin_rule().as_str()).is_some());
    }
}
