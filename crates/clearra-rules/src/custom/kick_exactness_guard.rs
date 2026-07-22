use crate::kicks::KickProfileSourceKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustomKickProfileKind {
    ImportedVerified,
    UnverifiedCustom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CustomKickExactnessGuard {
    profile_kind: CustomKickProfileKind,
    source_kind: KickProfileSourceKind,
    verified: bool,
    supports_180: bool,
}

impl CustomKickExactnessGuard {
    pub const fn imported_verified(supports_180: bool) -> Self {
        Self {
            profile_kind: CustomKickProfileKind::ImportedVerified,
            source_kind: KickProfileSourceKind::ImportedVerified,
            verified: true,
            supports_180,
        }
    }
}
impl CustomKickExactnessGuard {
    pub const fn unverified_custom() -> Self {
        Self {
            profile_kind: CustomKickProfileKind::UnverifiedCustom,
            source_kind: KickProfileSourceKind::RegistryDescriptorOnly,
            verified: false,
            supports_180: false,
        }
    }
}
impl CustomKickExactnessGuard {
    pub const fn profile_kind(self) -> CustomKickProfileKind {
        self.profile_kind
    }
}
impl CustomKickExactnessGuard {
    pub const fn source_kind(self) -> KickProfileSourceKind {
        self.source_kind
    }
}
impl CustomKickExactnessGuard {
    pub const fn verified(self) -> bool {
        self.verified
    }
}
impl CustomKickExactnessGuard {
    pub const fn supports_180(self) -> bool {
        self.supports_180
    }
}
impl CustomKickExactnessGuard {
    pub const fn supports_exact_180(self) -> bool {
        self.verified && self.supports_180
    }
}
impl CustomKickExactnessGuard {
    pub const fn c_execution_allowed(self) -> bool {
        !matches!(self.profile_kind, CustomKickProfileKind::UnverifiedCustom)
    }
}
impl CustomKickExactnessGuard {
    pub const fn disabled_reason(self) -> Option<&'static str> {
        if matches!(self.profile_kind, CustomKickProfileKind::UnverifiedCustom) {
            Some("unverified_custom_kick_rejected_before_c_execution")
        } else {
            None
        }
    }
}
impl CustomKickExactnessGuard {
    pub fn validate_before_c_execution(self) -> Result<(), CustomKickExecutionError> {
        if self.c_execution_allowed() {
            Ok(())
        } else {
            Err(CustomKickExecutionError::UnverifiedCustomKickRejectedBeforeCExecution)
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustomKickExecutionError {
    UnverifiedCustomKickRejectedBeforeCExecution,
}

#[cfg(test)]
#[path = "kick_exactness_guard_tests.rs"]
mod tests;
