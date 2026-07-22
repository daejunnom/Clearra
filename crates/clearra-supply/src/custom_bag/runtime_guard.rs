use crate::mixed::{CustomBagProfile, SupplyProfile, SupplyProfileKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomBagRuntimeGuard {
    bag_profile_id: String,
    piece_set_id: String,
    disabled_reason: &'static str,
    standard_fallback_forbidden: bool,
}

impl CustomBagRuntimeGuard {
    pub fn from_profile(profile: &CustomBagProfile) -> Self {
        Self {
            bag_profile_id: profile.bag_profile_id().to_owned(),
            piece_set_id: profile.piece_set_id().to_owned(),
            disabled_reason: profile.runtime_guard_reason(),
            standard_fallback_forbidden: true,
        }
    }
}
impl CustomBagRuntimeGuard {
    pub fn from_supply_profile(profile: &SupplyProfile) -> Option<Self> {
        if !matches!(profile.kind(), SupplyProfileKind::UnsupportedExtension(_)) {
            return None;
        }
        Some(Self {
            bag_profile_id: profile.provenance().bag_profile_id().to_owned(),
            piece_set_id: profile.provenance().piece_set_id().to_owned(),
            disabled_reason: profile
                .runtime_guard_reason()
                .unwrap_or("custom_bag_runtime_not_connected"),
            standard_fallback_forbidden: true,
        })
    }
}
impl CustomBagRuntimeGuard {
    pub fn bag_profile_id(&self) -> &str {
        &self.bag_profile_id
    }
}
impl CustomBagRuntimeGuard {
    pub fn piece_set_id(&self) -> &str {
        &self.piece_set_id
    }
}
impl CustomBagRuntimeGuard {
    pub const fn disabled_reason(&self) -> &'static str {
        self.disabled_reason
    }
}
impl CustomBagRuntimeGuard {
    pub const fn standard_fallback_forbidden(&self) -> bool {
        self.standard_fallback_forbidden
    }
}
impl CustomBagRuntimeGuard {
    pub fn custom_bag_not_silent_standard_fallback(&self) -> bool {
        self.standard_fallback_forbidden
            && self.disabled_reason == "custom_bag_runtime_not_connected"
    }
}
impl CustomBagRuntimeGuard {
    pub fn validate_execution(&self) -> Result<(), CustomBagExecutionError> {
        if !self.standard_fallback_forbidden {
            return Err(CustomBagExecutionError::SilentStandardFallbackForbidden);
        }
        Err(CustomBagExecutionError::RuntimeNotConnected {
            disabled_reason: self.disabled_reason,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomBagExecutionError {
    RuntimeNotConnected { disabled_reason: &'static str },
    SilentStandardFallbackForbidden,
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::ids::piece_id::PieceDefinitionId;

    use crate::mixed::{CustomBagEntry, CustomBagProfile};

    use super::*;

    #[test]
    fn custom_bag_not_silent_standard_fallback() {
        let profile = CustomBagProfile::new(
            "tri-bag",
            "mixed-standard-tri",
            vec![CustomBagEntry::new(
                PieceDefinitionId::new("custom:tri-v1"),
                1,
                1,
            )],
        )
        .expect("custom bag");
        let guard = CustomBagRuntimeGuard::from_profile(&profile);

        assert!(guard.custom_bag_not_silent_standard_fallback());
        assert_eq!(
            guard.validate_execution(),
            Err(CustomBagExecutionError::RuntimeNotConnected {
                disabled_reason: "custom_bag_runtime_not_connected"
            })
        );
    }
}
