use super::score_objective_policy::SpinProfileSelection;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionConstraintPolicy {
    preserve_back_to_back: bool,
    spin_profile: SpinProfileSelection,
}

impl ExecutionConstraintPolicy {
    pub const NONE: Self = Self {
        preserve_back_to_back: false,
        spin_profile: SpinProfileSelection::TSpins,
    };

    pub const fn preserve_back_to_back(spin_profile: SpinProfileSelection) -> Self {
        Self {
            preserve_back_to_back: true,
            spin_profile,
        }
    }

    pub const fn requested(self) -> bool {
        self.preserve_back_to_back
    }

    pub const fn preserves_back_to_back(self) -> bool {
        self.preserve_back_to_back
    }

    pub const fn spin_profile(self) -> SpinProfileSelection {
        self.spin_profile
    }
}

impl Default for ExecutionConstraintPolicy {
    fn default() -> Self {
        Self::NONE
    }
}
