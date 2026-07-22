#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpinAwardPolicy {
    #[default]
    Disabled,
    TSpinsOnly,
    AllSpins,
    AllMini,
    AllSpinAsTSpinMini,
}

impl SpinAwardPolicy {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "disabled" | "none" => Some(Self::Disabled),
            "t-spins-only" | "tspin-only" => Some(Self::TSpinsOnly),
            "all-spins" => Some(Self::AllSpins),
            "all-mini" => Some(Self::AllMini),
            "all-spin-as-t-spin-mini" => Some(Self::AllSpinAsTSpinMini),
            _ => None,
        }
    }
}
impl SpinAwardPolicy {
    pub fn allows_all_spins(self) -> bool {
        matches!(
            self,
            Self::AllSpins | Self::AllMini | Self::AllSpinAsTSpinMini
        )
    }
}
impl SpinAwardPolicy {
    pub fn requires_all_piece_classifier(self) -> bool {
        self.allows_all_spins()
    }
}
impl SpinAwardPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::TSpinsOnly => "t-spins-only",
            Self::AllSpins => "all-spins",
            Self::AllMini => "all-mini",
            Self::AllSpinAsTSpinMini => "all-spin-as-t-spin-mini",
        }
    }
}
