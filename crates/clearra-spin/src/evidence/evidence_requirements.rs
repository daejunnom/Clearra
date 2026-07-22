#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EvidenceRequirements {
    requires_last_action: bool,
    requires_corner: bool,
    requires_kick: bool,
    requires_immobile: bool,
    requires_special_case: bool,
}

impl EvidenceRequirements {
    pub const fn t_spin_corner() -> Self {
        Self {
            requires_last_action: true,
            requires_corner: true,
            requires_kick: false,
            requires_immobile: false,
            requires_special_case: false,
        }
    }
}
impl EvidenceRequirements {
    pub const fn kick_sensitive_special() -> Self {
        Self {
            requires_last_action: true,
            requires_corner: false,
            requires_kick: true,
            requires_immobile: false,
            requires_special_case: true,
        }
    }
}
impl EvidenceRequirements {
    pub const fn requires_kick(self) -> bool {
        self.requires_kick
    }
}
impl EvidenceRequirements {
    pub const fn requires_special_case(self) -> bool {
        self.requires_special_case
    }
}
