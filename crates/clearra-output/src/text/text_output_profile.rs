#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextOutputProfile {
    #[default]
    HumanSummary,
    Verbose,
    Diagnostics,
}

impl TextOutputProfile {
    pub fn is_verbose(self) -> bool {
        matches!(self, Self::Verbose)
    }
}
