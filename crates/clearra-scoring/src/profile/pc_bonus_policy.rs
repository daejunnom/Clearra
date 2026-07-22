#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PcBonusPolicy {
    #[default]
    Disabled,
    FixedBonus(u64),
}

impl PcBonusPolicy {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "disabled" | "none" => Some(Self::Disabled),
            _ => None,
        }
    }
}
impl PcBonusPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::FixedBonus(_) => "fixed-bonus",
        }
    }
}
impl PcBonusPolicy {
    pub fn bonus(self) -> u64 {
        match self {
            Self::Disabled => 0,
            Self::FixedBonus(value) => value,
        }
    }
}
