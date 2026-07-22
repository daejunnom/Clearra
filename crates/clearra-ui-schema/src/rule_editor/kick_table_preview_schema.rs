use clearra_rules::kicks::{
    KickProfileCapability, KickProfileDescriptor, KickProfileRegistry, KickProfileSourceKind,
    KickProfileVerificationReport, KickTableProfile, KickTableProfileId,
};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::disabled_reason::UiDisabledReason;

use super::kick_table_verification_schema::KickTableVerificationSchema;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickTablePreviewSchema {
    profile_id: String,
    rule_profile_id: String,
    label: String,
    source_kind: String,
    source_description: String,
    transition_count: usize,
    first_success_order_preserved: bool,
    provenance: String,
    verified: bool,
    supports_180: bool,
    supports_exact_180: bool,
    requires_lock_reachability: bool,
    requires_spawn_reachability: bool,
    search_backend_supported: bool,
    c_compact_descriptor_ready: bool,
    unsupported_backend_reason: String,
    disabled_reason: Option<UiDisabledReason>,
    verification: Option<KickTableVerificationSchema>,
}

impl KickTablePreviewSchema {
    pub fn from_descriptor(descriptor: KickProfileDescriptor) -> Self {
        let capability = descriptor.capability();
        Self {
            profile_id: descriptor.id().as_str().to_owned(),
            rule_profile_id: descriptor.rule_profile_id().as_str().to_owned(),
            label: descriptor.label().to_owned(),
            source_kind: descriptor.source_kind().as_str().to_owned(),
            source_description: descriptor.source_description().to_owned(),
            transition_count: descriptor.transition_count(),
            first_success_order_preserved: descriptor.first_success_order_preserved(),
            provenance: descriptor.provenance().to_owned(),
            verified: descriptor.verified(),
            supports_180: capability.supports_180(),
            supports_exact_180: capability.supports_exact_180(),
            requires_lock_reachability: capability.requires_lock_reachability(),
            requires_spawn_reachability: capability.requires_spawn_reachability(),
            search_backend_supported: capability.search_backend_supported(),
            c_compact_descriptor_ready: capability.c_compact_descriptor_ready(),
            unsupported_backend_reason: capability
                .unsupported_reason()
                .unwrap_or("none")
                .to_owned(),
            disabled_reason: capability
                .unsupported_reason()
                .map(|reason| UiDisabledReason::new(DiagnosticCode::ERuleUnsupportedMvp, reason)),
            verification: None,
        }
    }
}
impl KickTablePreviewSchema {
    pub fn from_profile(profile: &KickTableProfile) -> Self {
        let descriptor = KickProfileRegistry::descriptor(profile.id()).unwrap_or_else(|| {
            KickProfileDescriptor::new(
                profile.id(),
                profile.source_rule(),
                profile.id().as_str(),
                KickProfileSourceKind::ImportedVerified,
                "User imported verified kick table profile",
                KickProfileCapability::imported_verified(
                    profile.source_rule(),
                    profile.supports_180(),
                ),
            )
            .with_profile_contract(
                profile.transition_count(),
                true,
                "user-imported-kick-json",
                true,
            )
        });
        Self::from_descriptor(descriptor).with_verification(
            KickProfileVerificationReport::verify_imported_profile(profile),
        )
    }
}
impl KickTablePreviewSchema {
    pub fn imported_adapter_profile() -> Self {
        Self {
            profile_id: KickTableProfileId::Imported.as_str().to_owned(),
            rule_profile_id: clearra_rules::profile::rule_profile::RuleProfileId::Custom
                .as_str()
                .to_owned(),
            label: "Imported".to_owned(),
            source_kind: KickProfileSourceKind::ImportedVerified.as_str().to_owned(),
            source_description: "Imported kick table profile supplied by adapter".to_owned(),
            transition_count: 84,
            first_success_order_preserved: true,
            provenance: "user-imported-kick-json".to_owned(),
            verified: true,
            supports_180: true,
            supports_exact_180: true,
            requires_lock_reachability: true,
            requires_spawn_reachability: false,
            search_backend_supported: true,
            c_compact_descriptor_ready: true,
            unsupported_backend_reason: "none".to_owned(),
            disabled_reason: None,
            verification: None,
        }
    }
}
impl KickTablePreviewSchema {
    pub fn with_verification(mut self, report: KickProfileVerificationReport) -> Self {
        self.verification = Some(KickTableVerificationSchema::from_report(report));
        self
    }
}
impl KickTablePreviewSchema {
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
}
impl KickTablePreviewSchema {
    pub fn rule_profile_id(&self) -> &str {
        &self.rule_profile_id
    }
}
impl KickTablePreviewSchema {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl KickTablePreviewSchema {
    pub fn source_kind(&self) -> &str {
        &self.source_kind
    }
}
impl KickTablePreviewSchema {
    pub fn source_description(&self) -> &str {
        &self.source_description
    }
}
impl KickTablePreviewSchema {
    pub fn transition_count(&self) -> usize {
        self.transition_count
    }
}
impl KickTablePreviewSchema {
    pub fn first_success_order_preserved(&self) -> bool {
        self.first_success_order_preserved
    }
}
impl KickTablePreviewSchema {
    pub fn provenance(&self) -> &str {
        &self.provenance
    }
}
impl KickTablePreviewSchema {
    pub fn verified(&self) -> bool {
        self.verified
    }
}
impl KickTablePreviewSchema {
    pub fn supports_180(&self) -> bool {
        self.supports_180
    }
}
impl KickTablePreviewSchema {
    pub fn supports_exact_180(&self) -> bool {
        self.supports_exact_180
    }
}
impl KickTablePreviewSchema {
    pub fn requires_lock_reachability(&self) -> bool {
        self.requires_lock_reachability
    }
}
impl KickTablePreviewSchema {
    pub fn requires_spawn_reachability(&self) -> bool {
        self.requires_spawn_reachability
    }
}
impl KickTablePreviewSchema {
    pub fn search_backend_supported(&self) -> bool {
        self.search_backend_supported
    }
}
impl KickTablePreviewSchema {
    pub fn c_compact_descriptor_ready(&self) -> bool {
        self.c_compact_descriptor_ready
    }
}
impl KickTablePreviewSchema {
    pub fn unsupported_backend_reason(&self) -> &str {
        &self.unsupported_backend_reason
    }
}
impl KickTablePreviewSchema {
    pub fn disabled_reason(&self) -> Option<&UiDisabledReason> {
        self.disabled_reason.as_ref()
    }
}
impl KickTablePreviewSchema {
    pub fn verification(&self) -> Option<&KickTableVerificationSchema> {
        self.verification.as_ref()
    }
}
