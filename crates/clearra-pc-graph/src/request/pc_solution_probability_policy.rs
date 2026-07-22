#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PcSolutionProbabilityPolicy {
    #[default]
    Omit,
    Include,
}

impl PcSolutionProbabilityPolicy {
    pub const fn requested(self) -> bool {
        matches!(self, Self::Include)
    }
}
