#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum KickEvidenceRequirement {
    #[default]
    NotRequired,
    RequiredForExact,
    Required,
}

impl KickEvidenceRequirement {
    pub fn requires_evidence(self) -> bool {
        matches!(self, Self::RequiredForExact | Self::Required)
    }
}
