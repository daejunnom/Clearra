use crate::line_clear::LineClearPolicy;

use super::verified_custom_rule_profile::VerifiedCustomRuleProfile;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomRuleSearchCapabilityReport {
    search_backend_supported: bool,
    unsupported_reason: Option<&'static str>,
    supports_180: bool,
    requires_lock_reachability: bool,
    requires_spawn_reachability: bool,
    line_clear_policy: LineClearPolicy,
    c_compact_descriptor_ready: bool,
}

impl CustomRuleSearchCapabilityReport {
    pub fn from_verified_profile(profile: &VerifiedCustomRuleProfile) -> Self {
        Self {
            search_backend_supported: false,
            unsupported_reason: Some("custom_rule_search_backend_not_connected"),
            supports_180: profile.verified_kick_profile().profile().supports_180(),
            requires_lock_reachability: profile
                .lock_reachability_policy()
                .requires_lock_reachability(),
            requires_spawn_reachability: profile
                .lock_reachability_policy()
                .requires_spawn_reachability(),
            line_clear_policy: profile.line_clear_policy(),
            c_compact_descriptor_ready: profile.can_compile_to_c_descriptor(),
        }
    }
}
impl CustomRuleSearchCapabilityReport {
    pub fn search_backend_supported(&self) -> bool {
        self.search_backend_supported
    }
}
impl CustomRuleSearchCapabilityReport {
    pub fn unsupported_reason(&self) -> Option<&'static str> {
        self.unsupported_reason
    }
}
impl CustomRuleSearchCapabilityReport {
    pub fn supports_180(&self) -> bool {
        self.supports_180
    }
}
impl CustomRuleSearchCapabilityReport {
    pub fn requires_lock_reachability(&self) -> bool {
        self.requires_lock_reachability
    }
}
impl CustomRuleSearchCapabilityReport {
    pub fn requires_spawn_reachability(&self) -> bool {
        self.requires_spawn_reachability
    }
}
impl CustomRuleSearchCapabilityReport {
    pub fn line_clear_policy(&self) -> LineClearPolicy {
        self.line_clear_policy
    }
}
impl CustomRuleSearchCapabilityReport {
    pub fn c_compact_descriptor_ready(&self) -> bool {
        self.c_compact_descriptor_ready
    }
}
