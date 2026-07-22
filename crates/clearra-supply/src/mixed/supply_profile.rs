use super::{CustomBagProfile, SupplyProvenance};
use clearra_core_domain::ids::ExtensionId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SupplyProfileKind {
    Standard7Bag,
    FixedSequence,
    ObservedWindow,
    MaterializedPatternUniverse,
    UnsupportedExtension(ExtensionId),
}

impl SupplyProfileKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Standard7Bag => "standard-7-bag",
            Self::FixedSequence => "fixed-sequence",
            Self::ObservedWindow => "observed-window",
            Self::MaterializedPatternUniverse => "materialized-pattern-universe",
            Self::UnsupportedExtension(_) => "unsupported-extension",
        }
    }

    pub fn extension_id(&self) -> Option<&ExtensionId> {
        match self {
            Self::UnsupportedExtension(extension_id) => Some(extension_id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplyProfile {
    kind: SupplyProfileKind,
    provenance: SupplyProvenance,
    runtime_guard_reason: Option<&'static str>,
}

impl SupplyProfile {
    pub fn standard_7_bag_path_unchanged() -> Self {
        Self {
            kind: SupplyProfileKind::Standard7Bag,
            provenance: SupplyProvenance::standard_7_bag(),
            runtime_guard_reason: None,
        }
    }
}
impl SupplyProfile {
    pub fn fixed_sequence(provenance: SupplyProvenance) -> Self {
        Self {
            kind: SupplyProfileKind::FixedSequence,
            provenance,
            runtime_guard_reason: None,
        }
    }
}
impl SupplyProfile {
    pub fn observed_window(provenance: SupplyProvenance) -> Self {
        Self {
            kind: SupplyProfileKind::ObservedWindow,
            provenance,
            runtime_guard_reason: None,
        }
    }
}
impl SupplyProfile {
    pub fn materialized_pattern_universe(provenance: SupplyProvenance) -> Self {
        Self {
            kind: SupplyProfileKind::MaterializedPatternUniverse,
            provenance,
            runtime_guard_reason: None,
        }
    }
}
impl SupplyProfile {
    pub fn unsupported_extension(
        extension_id: ExtensionId,
        provenance: SupplyProvenance,
        disabled_reason: &'static str,
    ) -> Self {
        Self {
            kind: SupplyProfileKind::UnsupportedExtension(extension_id),
            provenance,
            runtime_guard_reason: Some(disabled_reason),
        }
    }
}
impl SupplyProfile {
    pub fn mixed_bag_profile(provenance: SupplyProvenance) -> Self {
        Self::unsupported_extension(
            ExtensionId::new("mixed-bag-profile"),
            provenance,
            "custom_bag_runtime_not_connected",
        )
    }
}
impl SupplyProfile {
    pub fn custom_bag_profile(profile: &CustomBagProfile, provenance: SupplyProvenance) -> Self {
        Self::unsupported_extension(
            ExtensionId::new(profile.bag_profile_id()),
            provenance,
            profile.runtime_guard_reason(),
        )
    }
}
impl SupplyProfile {
    pub fn kind(&self) -> &SupplyProfileKind {
        &self.kind
    }
}
impl SupplyProfile {
    pub fn provenance(&self) -> &SupplyProvenance {
        &self.provenance
    }
}
impl SupplyProfile {
    pub fn runtime_guard_reason(&self) -> Option<&'static str> {
        self.runtime_guard_reason
    }
}

#[cfg(test)]
#[path = "supply_profile_tests.rs"]
mod tests;
