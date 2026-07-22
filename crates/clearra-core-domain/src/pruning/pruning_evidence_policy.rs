#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum PruningEvidencePolicy {
    #[default]
    BestEffort,
    CompleteRequired,
}

impl PruningEvidencePolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BestEffort => "best-effort",
            Self::CompleteRequired => "complete-required",
        }
    }
}
