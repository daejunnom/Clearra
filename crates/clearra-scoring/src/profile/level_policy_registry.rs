use super::level_policy::LevelPolicy;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LevelPolicyDescriptor {
    id: LevelPolicy,
    display_name: &'static str,
}

impl LevelPolicyDescriptor {
    pub const fn new(id: LevelPolicy, display_name: &'static str) -> Self {
        Self { id, display_name }
    }
}
impl LevelPolicyDescriptor {
    pub const fn id(self) -> LevelPolicy {
        self.id
    }
}
impl LevelPolicyDescriptor {
    pub const fn display_name(self) -> &'static str {
        self.display_name
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LevelPolicyRegistry;

impl LevelPolicyRegistry {
    pub fn builtins() -> Vec<LevelPolicyDescriptor> {
        vec![
            LevelPolicyDescriptor::new(LevelPolicy::Disabled, "Disabled"),
            LevelPolicyDescriptor::new(LevelPolicy::FixedLevelOne, "Fixed level 1"),
        ]
    }
}
