#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LevelPolicy {
    #[default]
    Disabled,
    FixedLevelOne,
}

impl LevelPolicy {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "disabled" | "none" => Some(Self::Disabled),
            "fixed-level-one" | "level-one" => Some(Self::FixedLevelOne),
            _ => None,
        }
    }
}
impl LevelPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::FixedLevelOne => "fixed-level-one",
        }
    }
}
