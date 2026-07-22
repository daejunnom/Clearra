#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScoreAccuracy {
    Exact,
    PatternComplete,
    TraceSampleOnly,
    PlacementOnlyEstimate,
    KickSensitiveUnavailable,
    #[default]
    Incomplete,
}

impl ScoreAccuracy {
    pub fn is_exact(self) -> bool {
        matches!(self, Self::Exact | Self::PatternComplete)
    }
}
