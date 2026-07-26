use super::rule_profile::{RuleProfile, RuleProfileId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuleKickModel {
    Srs90,
    SrsPlus180,
    SrsX,
    Jstris180,
    Asc,
    Ars,
    NoKick,
    UnsupportedCustom,
}

impl RuleKickModel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Srs90 => "srs-90",
            Self::SrsPlus180 => "srs-plus-180",
            Self::SrsX => "srs-x",
            Self::Jstris180 => "jstris-180",
            Self::Asc => "asc",
            Self::Ars => "ars",
            Self::NoKick => "no-kick",
            Self::UnsupportedCustom => "unsupported-custom",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuleCapability {
    two_line_supported: bool,
    kick_model: RuleKickModel,
    supports_180: bool,
    supports_exact_180: bool,
    requires_lock_reachability: bool,
    requires_spawn_reachability: bool,
    search_backend_supported: bool,
    srs_plus_extensions_disabled: bool,
    unsupported_reason: Option<&'static str>,
}

impl RuleCapability {
    pub fn from_rule(rule: RuleProfile) -> Self {
        let kick_model = match rule.id() {
            RuleProfileId::Srs => RuleKickModel::Srs90,
            RuleProfileId::SrsPlus => RuleKickModel::SrsPlus180,
            RuleProfileId::SrsX => RuleKickModel::SrsX,
            RuleProfileId::Jstris180 => RuleKickModel::Jstris180,
            RuleProfileId::Asc => RuleKickModel::Asc,
            RuleProfileId::Ars => RuleKickModel::Ars,
            RuleProfileId::NoKick => RuleKickModel::NoKick,
            RuleProfileId::Custom => RuleKickModel::UnsupportedCustom,
        };
        let supports_180 = matches!(
            rule.id(),
            RuleProfileId::SrsPlus
                | RuleProfileId::SrsX
                | RuleProfileId::Jstris180
                | RuleProfileId::Asc
        );
        let requires_spawn_reachability =
            matches!(rule.id(), RuleProfileId::Asc | RuleProfileId::Ars);
        let search_backend_supported = matches!(
            rule.id(),
            RuleProfileId::Srs
                | RuleProfileId::SrsPlus
                | RuleProfileId::SrsX
                | RuleProfileId::Jstris180
                | RuleProfileId::NoKick
        );
        Self {
            two_line_supported: rule.is_two_line_supported(),
            kick_model,
            supports_180,
            supports_exact_180: matches!(
                rule.id(),
                RuleProfileId::SrsPlus | RuleProfileId::SrsX | RuleProfileId::Jstris180
            ),
            requires_lock_reachability: !matches!(rule.id(), RuleProfileId::NoKick),
            requires_spawn_reachability,
            search_backend_supported,
            srs_plus_extensions_disabled: false,
            unsupported_reason: (!search_backend_supported).then_some(match rule.id() {
                RuleProfileId::Custom => "custom_rule_unsupported",
                RuleProfileId::Asc => "asc_profile_requires_spawn_reachability",
                RuleProfileId::Ars => "ars_profile_requires_spawn_reachability",
                _ => "rule_profile_unsupported",
            }),
        }
    }
}
impl RuleCapability {
    pub fn two_line_supported(self) -> bool {
        self.two_line_supported
    }
}
impl RuleCapability {
    pub fn kick_model(self) -> RuleKickModel {
        self.kick_model
    }
}
impl RuleCapability {
    pub fn supports_180(self) -> bool {
        self.supports_180
    }
}
impl RuleCapability {
    pub fn supports_exact_180(self) -> bool {
        self.supports_exact_180
    }
}
impl RuleCapability {
    pub fn requires_lock_reachability(self) -> bool {
        self.requires_lock_reachability
    }
}
impl RuleCapability {
    pub fn requires_spawn_reachability(self) -> bool {
        self.requires_spawn_reachability
    }
}
impl RuleCapability {
    pub fn search_backend_supported(self) -> bool {
        self.search_backend_supported
    }
}
impl RuleCapability {
    pub fn srs_plus_extensions_disabled(self) -> bool {
        self.srs_plus_extensions_disabled
    }
}
impl RuleCapability {
    pub fn extension_disabled_reason(self) -> Option<&'static str> {
        self.srs_plus_extensions_disabled
            .then_some("srs_plus_extensions_disabled")
    }
}
impl RuleCapability {
    pub fn unsupported_reason(self) -> Option<&'static str> {
        self.unsupported_reason
    }
}

#[cfg(test)]
#[path = "rule_capability_tests.rs"]
mod tests;
