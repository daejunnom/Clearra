use crate::{
    builtin::{jstris_ultra, ppt_profile, tetrio_score_with_spin_profile},
    profile::{score_profile::ScoreProfile, SpinProfileRegistry},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreProfileRegistry {
    profiles: Vec<ScoreProfile>,
}

impl ScoreProfileRegistry {
    pub fn new(profiles: Vec<ScoreProfile>) -> Self {
        Self { profiles }
    }
}
impl ScoreProfileRegistry {
    pub fn builtins() -> Self {
        let mut profiles = vec![jstris_ultra(), ppt_profile()];
        profiles.extend(
            SpinProfileRegistry::builtins()
                .profiles()
                .iter()
                .map(|profile| tetrio_score_with_spin_profile(profile.id())),
        );
        Self::new(profiles)
    }
}
impl ScoreProfileRegistry {
    pub fn profiles(&self) -> &[ScoreProfile] {
        &self.profiles
    }
}
impl ScoreProfileRegistry {
    pub fn get(&self, id: &str) -> Option<&ScoreProfile> {
        let canonical = id.trim().to_ascii_lowercase().replace('_', "-");
        self.profiles
            .iter()
            .find(|profile| profile.id() == canonical)
    }
}

impl Default for ScoreProfileRegistry {
    fn default() -> Self {
        Self::builtins()
    }
}

#[cfg(test)]
#[path = "score_profile_registry_tests.rs"]
mod tests;
