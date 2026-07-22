use super::special_spin_case_id::SpecialSpinCaseId;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpinClassifierCapability {
    #[default]
    Disabled,
    DescriptorOnly,
    ExactWithKickEvidence,
    SourcePinnedExact,
}

impl SpinClassifierCapability {
    pub fn supports_exact(self) -> bool {
        matches!(self, Self::ExactWithKickEvidence | Self::SourcePinnedExact)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedSpecialSpinProfile {
    id: String,
    base_kick_profile: String,
    special_cases: Vec<SpecialSpinCaseId>,
    fixture_set_id: String,
    spin_classifier_capability: SpinClassifierCapability,
}

impl VerifiedSpecialSpinProfile {
    pub fn new(
        id: impl Into<String>,
        base_kick_profile: impl Into<String>,
        fixture_set_id: impl Into<String>,
        capability: SpinClassifierCapability,
    ) -> Self {
        Self {
            id: id.into(),
            base_kick_profile: base_kick_profile.into(),
            special_cases: Vec::new(),
            fixture_set_id: fixture_set_id.into(),
            spin_classifier_capability: capability,
        }
    }
}
impl VerifiedSpecialSpinProfile {
    pub fn with_special_case(mut self, special_case: SpecialSpinCaseId) -> Self {
        self.special_cases.push(special_case);
        self
    }
}
impl VerifiedSpecialSpinProfile {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl VerifiedSpecialSpinProfile {
    pub fn base_kick_profile(&self) -> &str {
        &self.base_kick_profile
    }
}
impl VerifiedSpecialSpinProfile {
    pub fn special_cases(&self) -> &[SpecialSpinCaseId] {
        &self.special_cases
    }
}
impl VerifiedSpecialSpinProfile {
    pub fn fixture_set_id(&self) -> &str {
        &self.fixture_set_id
    }
}
impl VerifiedSpecialSpinProfile {
    pub fn spin_classifier_capability(&self) -> SpinClassifierCapability {
        self.spin_classifier_capability
    }
}
impl VerifiedSpecialSpinProfile {
    pub fn search_backend_supported(&self) -> bool {
        self.spin_classifier_capability.supports_exact() && !self.special_cases.is_empty()
    }
}
impl VerifiedSpecialSpinProfile {
    pub fn unsupported_reason(&self) -> Option<&'static str> {
        if self.special_cases.is_empty() {
            Some("verified_special_spin_profile_missing_cases")
        } else if !self.spin_classifier_capability.supports_exact() {
            Some("special_spin_profile_unverified")
        } else {
            None
        }
    }
}
