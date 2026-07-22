use super::{SpinProfile, SpinProfileId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinProfileRegistry {
    profiles: Vec<SpinProfile>,
}

impl SpinProfileRegistry {
    pub fn builtins() -> Self {
        Self {
            profiles: vec![
                SpinProfile::builtin(SpinProfileId::TSpins),
                SpinProfile::builtin(SpinProfileId::TSpinsPlus),
                SpinProfile::builtin(SpinProfileId::AllSpin),
                SpinProfile::builtin(SpinProfileId::AllSpinPlus),
                SpinProfile::builtin(SpinProfileId::AllMini),
                SpinProfile::builtin(SpinProfileId::AllMiniPlus),
            ],
        }
    }

    pub fn profiles(&self) -> &[SpinProfile] {
        &self.profiles
    }

    pub fn get(&self, id: SpinProfileId) -> Option<SpinProfile> {
        self.profiles
            .iter()
            .copied()
            .find(|profile| profile.id() == id)
    }
}

impl Default for SpinProfileRegistry {
    fn default() -> Self {
        Self::builtins()
    }
}
