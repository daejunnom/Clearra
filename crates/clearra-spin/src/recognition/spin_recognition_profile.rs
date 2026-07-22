use crate::{
    evidence::EvidenceRequirements,
    recognition::{SpinRecognizerId, UnknownSpinPolicy},
    special::SpecialSpinCaseRegistryId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinRecognitionProfileId(String);

impl SpinRecognitionProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl SpinRecognitionProfileId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinRecognitionProfile {
    pub id: SpinRecognitionProfileId,
    pub recognizer_set: Vec<SpinRecognizerId>,
    pub evidence_requirements: EvidenceRequirements,
    pub special_case_registry: SpecialSpinCaseRegistryId,
    pub unknown_policy: UnknownSpinPolicy,
}

impl SpinRecognitionProfile {
    pub fn t_spin_corner() -> Self {
        Self {
            id: SpinRecognitionProfileId::new("t-spin-corner-recognition"),
            recognizer_set: vec![SpinRecognizerId::CornerTSpinRecognizer],
            evidence_requirements: EvidenceRequirements::t_spin_corner(),
            special_case_registry: SpecialSpinCaseRegistryId::new("none"),
            unknown_policy: UnknownSpinPolicy::PreserveUnknown,
        }
    }
}
impl SpinRecognitionProfile {
    pub fn kick_sensitive_special(registry_id: impl Into<String>) -> Self {
        Self {
            id: SpinRecognitionProfileId::new("kick-sensitive-special-recognition"),
            recognizer_set: vec![
                SpinRecognizerId::KickSensitiveRecognizer,
                SpinRecognizerId::SpecialSpinCaseRecognizer,
            ],
            evidence_requirements: EvidenceRequirements::kick_sensitive_special(),
            special_case_registry: SpecialSpinCaseRegistryId::new(registry_id),
            unknown_policy: UnknownSpinPolicy::PreserveUnknown,
        }
    }
}
