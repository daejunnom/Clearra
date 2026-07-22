#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpinAccuracy {
    Exact,
    PatternComplete,
    TraceSampleOnly,
    PlacementOnlyEstimate,
    KickSensitiveUnavailable,
    #[default]
    Incomplete,
}

impl SpinAccuracy {
    pub fn is_exact(self) -> bool {
        matches!(self, Self::Exact | Self::PatternComplete)
    }
}
impl SpinAccuracy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::PatternComplete => "pattern-complete",
            Self::TraceSampleOnly => "trace-sample-only",
            Self::PlacementOnlyEstimate => "placement-only-estimate",
            Self::KickSensitiveUnavailable => "kick-sensitive-unavailable",
            Self::Incomplete => "incomplete",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TraceCompleteness {
    Full,
    RetainedSample,
    MissingKickEvidence,
    #[default]
    Incomplete,
}

impl TraceCompleteness {
    pub fn is_full(self) -> bool {
        matches!(self, Self::Full)
    }
}
impl TraceCompleteness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::RetainedSample => "retained-sample",
            Self::MissingKickEvidence => "missing-kick-evidence",
            Self::Incomplete => "incomplete",
        }
    }
}
