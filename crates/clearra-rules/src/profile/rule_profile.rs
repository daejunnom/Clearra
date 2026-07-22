#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuleProfileId {
    SrsPlus,
    Srs,
    SrsX,
    Asc,
    Ars,
    NoKick,
    Custom,
}

impl RuleProfileId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SrsPlus => "srs-plus",
            Self::Srs => "srs",
            Self::SrsX => "srs-x",
            Self::Asc => "asc",
            Self::Ars => "ars",
            Self::NoKick => "no-kick",
            Self::Custom => "custom",
        }
    }
}
impl RuleProfileId {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "srs-plus" => Some(Self::SrsPlus),
            "srs" => Some(Self::Srs),
            "srs-x" => Some(Self::SrsX),
            "asc" => Some(Self::Asc),
            "ars" => Some(Self::Ars),
            "no-kick" | "nokick" => Some(Self::NoKick),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuleProfile {
    id: RuleProfileId,
}

impl RuleProfile {
    pub const fn new(id: RuleProfileId) -> Self {
        Self { id }
    }
}
impl RuleProfile {
    pub fn id(self) -> RuleProfileId {
        self.id
    }
}
impl RuleProfile {
    pub fn is_two_line_supported(self) -> bool {
        matches!(
            self.id,
            RuleProfileId::SrsPlus
                | RuleProfileId::Srs
                | RuleProfileId::SrsX
                | RuleProfileId::Asc
                | RuleProfileId::Ars
                | RuleProfileId::NoKick
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_profile_ids_expose_stable_canonical_strings() {
        assert_eq!(RuleProfileId::SrsPlus.as_str(), "srs-plus");
        assert_eq!(RuleProfileId::Srs.as_str(), "srs");
        assert_eq!(RuleProfileId::SrsX.as_str(), "srs-x");
        assert_eq!(RuleProfileId::Asc.as_str(), "asc");
        assert_eq!(RuleProfileId::Ars.as_str(), "ars");
        assert_eq!(RuleProfileId::NoKick.as_str(), "no-kick");
        assert_eq!(RuleProfileId::Custom.as_str(), "custom");
        assert_eq!(RuleProfileId::parse("srs-x"), Some(RuleProfileId::SrsX));
    }
}
