#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScoreObjectiveMode {
    #[default]
    Disabled,
    Summary,
}

impl ScoreObjectiveMode {
    pub const fn requested(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Summary => "summary",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoreProfileSelection {
    Tetrio,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SpinProfileSelection {
    #[default]
    TSpins,
    TSpinsPlus,
    AllSpin,
    AllSpinPlus,
    AllMini,
    AllMiniPlus,
}

impl SpinProfileSelection {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "t-spin" | "t-spins" => Some(Self::TSpins),
            "t-spin-plus" | "t-spins-plus" => Some(Self::TSpinsPlus),
            "all-spin" | "all-spins" => Some(Self::AllSpin),
            "all-spin-plus" | "all-spins-plus" | "all-plus" => Some(Self::AllSpinPlus),
            "all-mini" => Some(Self::AllMini),
            "all-mini-plus" | "srs-plus-all-mini" => Some(Self::AllMiniPlus),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TSpins => "t-spins",
            Self::TSpinsPlus => "t-spins-plus",
            Self::AllSpin => "all-spin",
            Self::AllSpinPlus => "all-spin-plus",
            Self::AllMini => "all-mini",
            Self::AllMiniPlus => "all-mini-plus",
        }
    }
}

impl ScoreProfileSelection {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "tetrio" | "tetrio-score" => Some(Self::Tetrio),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tetrio => "tetrio",
        }
    }
}

impl Default for ScoreProfileSelection {
    fn default() -> Self {
        Self::Tetrio
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScoreObjectivePolicy {
    mode: ScoreObjectiveMode,
    profile: ScoreProfileSelection,
    spin_profile: SpinProfileSelection,
    initial_b2b: u32,
}

impl ScoreObjectivePolicy {
    pub const DISABLED: Self = Self {
        mode: ScoreObjectiveMode::Disabled,
        profile: ScoreProfileSelection::Tetrio,
        spin_profile: SpinProfileSelection::TSpins,
        initial_b2b: 0,
    };

    pub const fn summary() -> Self {
        Self::new(ScoreObjectiveMode::Summary)
    }

    pub const fn new(mode: ScoreObjectiveMode) -> Self {
        Self {
            mode,
            profile: ScoreProfileSelection::Tetrio,
            spin_profile: SpinProfileSelection::TSpins,
            initial_b2b: 0,
        }
    }

    pub const fn with_initial_b2b(mut self, initial_b2b: u32) -> Self {
        self.initial_b2b = initial_b2b;
        self
    }

    pub const fn with_profile(mut self, profile: ScoreProfileSelection) -> Self {
        self.profile = profile;
        self
    }

    pub const fn with_spin_profile(mut self, spin_profile: SpinProfileSelection) -> Self {
        self.spin_profile = spin_profile;
        self
    }

    pub const fn mode(self) -> ScoreObjectiveMode {
        self.mode
    }

    pub const fn profile(self) -> ScoreProfileSelection {
        self.profile
    }

    pub const fn spin_profile(self) -> SpinProfileSelection {
        self.spin_profile
    }

    pub const fn initial_b2b(self) -> u32 {
        self.initial_b2b
    }

    pub const fn requested(self) -> bool {
        self.mode.requested()
    }
}

impl Default for ScoreObjectivePolicy {
    fn default() -> Self {
        Self::DISABLED
    }
}
