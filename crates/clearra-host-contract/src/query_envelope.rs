#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum QueryEnvelope {
    PcOpening,
    PcScenario,
    PathOpening,
    PercentScenario,
    SetupSearch,
    BuildProbability,
    Damage,
    SpinFinder,
    BuildCoverage,
    ContinueToken,
    Rules,
    Scoring,
    Convert,
    InspectUnsupported,
    Verify,
    VerifyKicks,
}

impl QueryEnvelope {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::PcOpening => "pc-opening",
            Self::PcScenario => "pc-scenario",
            Self::PathOpening => "path-opening",
            Self::PercentScenario => "percent-scenario",
            Self::SetupSearch => "setup-search",
            Self::BuildProbability => "build-probability",
            Self::Damage => "damage",
            Self::SpinFinder => "spin-finder",
            Self::BuildCoverage => "build-coverage",
            Self::ContinueToken => "continue-token",
            Self::Rules => "rules",
            Self::Scoring => "scoring",
            Self::Convert => "convert",
            Self::InspectUnsupported => "inspect-unsupported",
            Self::Verify => "verify",
            Self::VerifyKicks => "verify-kicks",
        }
    }
}
