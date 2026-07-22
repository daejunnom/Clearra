use super::kick_table::KickTableProfileId;
use crate::profile::rule_profile::RuleProfileId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KickProfileSourceKind {
    BuiltInExact,
    ImportedVerified,
    RegistryDescriptorOnly,
}

impl KickProfileSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BuiltInExact => "built-in-exact",
            Self::ImportedVerified => "imported-verified",
            Self::RegistryDescriptorOnly => "registry-descriptor-only",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KickProfileCapability {
    supports_180: bool,
    supports_exact_180: bool,
    requires_lock_reachability: bool,
    requires_spawn_reachability: bool,
    search_backend_supported: bool,
    unsupported_reason: Option<&'static str>,
}

impl KickProfileCapability {
    pub const fn new(
        supports_180: bool,
        supports_exact_180: bool,
        requires_lock_reachability: bool,
        requires_spawn_reachability: bool,
        search_backend_supported: bool,
        unsupported_reason: Option<&'static str>,
    ) -> Self {
        Self {
            supports_180,
            supports_exact_180,
            requires_lock_reachability,
            requires_spawn_reachability,
            search_backend_supported,
            unsupported_reason,
        }
    }
}
impl KickProfileCapability {
    pub fn imported_verified(source_rule: RuleProfileId, supports_180: bool) -> Self {
        let unsupported_reason = match source_rule {
            RuleProfileId::Asc => Some("asc_profile_requires_spawn_reachability"),
            RuleProfileId::Ars => Some("ars_profile_requires_spawn_reachability"),
            RuleProfileId::Custom => Some("custom_rule_unsupported"),
            RuleProfileId::Srs
            | RuleProfileId::SrsPlus
            | RuleProfileId::SrsX
            | RuleProfileId::NoKick => None,
        };
        Self {
            supports_180,
            supports_exact_180: supports_180,
            requires_lock_reachability: !matches!(source_rule, RuleProfileId::NoKick),
            requires_spawn_reachability: matches!(
                source_rule,
                RuleProfileId::Asc | RuleProfileId::Ars
            ),
            search_backend_supported: unsupported_reason.is_none(),
            unsupported_reason,
        }
    }
}
impl KickProfileCapability {
    pub fn supports_180(self) -> bool {
        self.supports_180
    }
}
impl KickProfileCapability {
    pub fn supports_exact_180(self) -> bool {
        self.supports_exact_180
    }
}
impl KickProfileCapability {
    pub fn requires_lock_reachability(self) -> bool {
        self.requires_lock_reachability
    }
}
impl KickProfileCapability {
    pub fn requires_spawn_reachability(self) -> bool {
        self.requires_spawn_reachability
    }
}
impl KickProfileCapability {
    pub fn search_backend_supported(self) -> bool {
        self.search_backend_supported
    }
}
impl KickProfileCapability {
    pub fn c_compact_descriptor_ready(self) -> bool {
        self.search_backend_supported
    }
}
impl KickProfileCapability {
    pub fn unsupported_reason(self) -> Option<&'static str> {
        self.unsupported_reason
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KickProfileDescriptor {
    id: KickTableProfileId,
    rule_profile_id: RuleProfileId,
    label: &'static str,
    source_kind: KickProfileSourceKind,
    source_description: &'static str,
    transition_count: usize,
    first_success_order_preserved: bool,
    provenance: &'static str,
    verified: bool,
    capability: KickProfileCapability,
}

impl KickProfileDescriptor {
    pub fn new(
        id: KickTableProfileId,
        rule_profile_id: RuleProfileId,
        label: &'static str,
        source_kind: KickProfileSourceKind,
        source_description: &'static str,
        capability: KickProfileCapability,
    ) -> Self {
        Self {
            id,
            rule_profile_id,
            label,
            source_kind,
            source_description,
            transition_count: if capability.supports_180 { 84 } else { 56 },
            first_success_order_preserved: true,
            provenance: source_description,
            verified: matches!(
                source_kind,
                KickProfileSourceKind::BuiltInExact | KickProfileSourceKind::ImportedVerified
            ),
            capability,
        }
    }
}
impl KickProfileDescriptor {
    pub fn with_profile_contract(
        mut self,
        transition_count: usize,
        first_success_order_preserved: bool,
        provenance: &'static str,
        verified: bool,
    ) -> Self {
        self.transition_count = transition_count;
        self.first_success_order_preserved = first_success_order_preserved;
        self.provenance = provenance;
        self.verified = verified;
        self
    }
}
impl KickProfileDescriptor {
    pub fn id(self) -> KickTableProfileId {
        self.id
    }
}
impl KickProfileDescriptor {
    pub fn rule_profile_id(self) -> RuleProfileId {
        self.rule_profile_id
    }
}
impl KickProfileDescriptor {
    pub fn label(self) -> &'static str {
        self.label
    }
}
impl KickProfileDescriptor {
    pub fn source_kind(self) -> KickProfileSourceKind {
        self.source_kind
    }
}
impl KickProfileDescriptor {
    pub fn source_description(self) -> &'static str {
        self.source_description
    }
}
impl KickProfileDescriptor {
    pub fn transition_count(self) -> usize {
        self.transition_count
    }
}
impl KickProfileDescriptor {
    pub fn first_success_order_preserved(self) -> bool {
        self.first_success_order_preserved
    }
}
impl KickProfileDescriptor {
    pub fn provenance(self) -> &'static str {
        self.provenance
    }
}
impl KickProfileDescriptor {
    pub fn verified(self) -> bool {
        self.verified
    }
}
impl KickProfileDescriptor {
    pub fn capability(self) -> KickProfileCapability {
        self.capability
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct KickProfileRegistry;

impl KickProfileRegistry {
    pub fn builtin_profiles() -> Vec<KickProfileDescriptor> {
        vec![
            KickProfileDescriptor::new(
                KickTableProfileId::Srs90,
                RuleProfileId::Srs,
                "SRS 90",
                KickProfileSourceKind::BuiltInExact,
                "Clearra built-in SRS 90 kick table",
                KickProfileCapability::new(false, false, true, false, true, None),
            ),
            KickProfileDescriptor::new(
                KickTableProfileId::SrsPlus,
                RuleProfileId::SrsPlus,
                "SRS+",
                KickProfileSourceKind::BuiltInExact,
                "Clearra built-in TETR.IO SRS+ symmetric I and 180 kick table",
                KickProfileCapability::new(true, true, true, false, true, None),
            )
            .with_profile_contract(
                80,
                true,
                "Clearra built-in TETR.IO SRS+ symmetric I and 180 kick table",
                true,
            ),
            KickProfileDescriptor::new(
                KickTableProfileId::NoKick,
                RuleProfileId::NoKick,
                "No Kick",
                KickProfileSourceKind::BuiltInExact,
                "Clearra built-in no-kick profile",
                KickProfileCapability::new(false, false, false, false, true, None),
            ),
            KickProfileDescriptor::new(
                KickTableProfileId::SrsX,
                RuleProfileId::SrsX,
                "SRS-X",
                KickProfileSourceKind::BuiltInExact,
                "Clearra built-in TETR.IO SRS-X: SRS 90 with Nullpomino/Heboris-style 180 kicks",
                KickProfileCapability::new(true, true, true, false, true, None),
            )
            .with_profile_contract(
                80,
                true,
                "TETR.IO SRS-X contract: SRS 90 with Nullpomino/Heboris-style 180 kicks",
                true,
            ),
            KickProfileDescriptor::new(
                KickTableProfileId::Asc,
                RuleProfileId::Asc,
                "ASC",
                KickProfileSourceKind::RegistryDescriptorOnly,
                "Registry descriptor only; current backend lacks ASC spawn-aware reachability",
                KickProfileCapability::new(
                    true,
                    false,
                    true,
                    true,
                    false,
                    Some("asc_profile_requires_spawn_reachability"),
                ),
            ),
            KickProfileDescriptor::new(
                KickTableProfileId::Ars,
                RuleProfileId::Ars,
                "ARS",
                KickProfileSourceKind::RegistryDescriptorOnly,
                "Registry descriptor only; current backend lacks ARS spawn-aware reachability",
                KickProfileCapability::new(
                    false,
                    false,
                    true,
                    true,
                    false,
                    Some("ars_profile_requires_spawn_reachability"),
                ),
            ),
        ]
    }
}
impl KickProfileRegistry {
    pub fn descriptor(id: KickTableProfileId) -> Option<KickProfileDescriptor> {
        Self::builtin_profiles()
            .into_iter()
            .find(|descriptor| descriptor.id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kick_profile_registry_exposes_mvp2_extension_profile_capabilities() {
        let profiles = KickProfileRegistry::builtin_profiles();
        let srs_x = KickProfileRegistry::descriptor(KickTableProfileId::SrsX).expect("srs-x");
        let asc = KickProfileRegistry::descriptor(KickTableProfileId::Asc).expect("asc");
        let ars = KickProfileRegistry::descriptor(KickTableProfileId::Ars).expect("ars");

        assert!(profiles
            .iter()
            .any(|profile| profile.id() == KickTableProfileId::SrsPlus));
        let srs_plus =
            KickProfileRegistry::descriptor(KickTableProfileId::SrsPlus).expect("srs-plus");
        assert_eq!(srs_plus.source_kind(), KickProfileSourceKind::BuiltInExact);
        assert!(srs_plus.source_description().contains("TETR.IO SRS+"));
        assert_eq!(srs_plus.transition_count(), 80);
        assert!(srs_plus.first_success_order_preserved());
        assert!(srs_plus.provenance().contains("symmetric I"));
        assert!(srs_plus.verified());
        assert!(srs_plus.capability().supports_180());
        assert!(srs_plus.capability().supports_exact_180());
        assert!(srs_x.capability().supports_180());
        assert_eq!(srs_x.transition_count(), 80);
        assert_eq!(srs_x.source_kind(), KickProfileSourceKind::BuiltInExact);
        assert!(srs_x.capability().supports_exact_180());
        assert!(srs_x.capability().search_backend_supported());
        assert!(srs_x.capability().unsupported_reason().is_none());
        assert!(asc.capability().requires_spawn_reachability());
        for descriptor in [asc, ars] {
            assert!(!descriptor.capability().search_backend_supported());
            assert!(descriptor.capability().unsupported_reason().is_some());
        }
    }
}
