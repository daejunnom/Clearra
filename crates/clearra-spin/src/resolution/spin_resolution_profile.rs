#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinResolutionProfileId(String);

impl SpinResolutionProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl SpinResolutionProfileId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinResolutionProfile {
    pub id: SpinResolutionProfileId,
    pub preserve_multiple_legal_interpretations: bool,
}

impl SpinResolutionProfile {
    pub fn preserve_all_legal() -> Self {
        Self {
            id: SpinResolutionProfileId::new("preserve-all-legal-spin-interpretations"),
            preserve_multiple_legal_interpretations: true,
        }
    }
}
