use super::pc_bonus_policy::PcBonusPolicy;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcBonusPolicyDescriptor {
    id: PcBonusPolicy,
    display_name: &'static str,
}

impl PcBonusPolicyDescriptor {
    pub const fn new(id: PcBonusPolicy, display_name: &'static str) -> Self {
        Self { id, display_name }
    }
}
impl PcBonusPolicyDescriptor {
    pub const fn id(self) -> PcBonusPolicy {
        self.id
    }
}
impl PcBonusPolicyDescriptor {
    pub const fn display_name(self) -> &'static str {
        self.display_name
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PcBonusPolicyRegistry;

impl PcBonusPolicyRegistry {
    pub fn builtins() -> Vec<PcBonusPolicyDescriptor> {
        vec![
            PcBonusPolicyDescriptor::new(PcBonusPolicy::Disabled, "Disabled"),
            PcBonusPolicyDescriptor::new(PcBonusPolicy::FixedBonus(3500), "Fixed PC bonus"),
        ]
    }
}
