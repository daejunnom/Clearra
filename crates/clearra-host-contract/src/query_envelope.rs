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
    Ren,
    SpinStructure,
    BuildCoverage,
    ContinueToken,
    Rules,
    Scoring,
    Convert,
    InspectUnsupported,
    Verify,
    VerifyKicks,
    UtilitySequence,
    UtilitySequenceDependencies,
    UtilityParity,
    UtilityFumen,
    UtilityRender,
    UtilityToGray,
    UtilityMirror,
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
            Self::Ren => "ren",
            Self::SpinStructure => "spin-structure",
            Self::BuildCoverage => "build-coverage",
            Self::ContinueToken => "continue-token",
            Self::Rules => "rules",
            Self::Scoring => "scoring",
            Self::Convert => "convert",
            Self::InspectUnsupported => "inspect-unsupported",
            Self::Verify => "verify",
            Self::VerifyKicks => "verify-kicks",
            Self::UtilitySequence => "utility-sequence",
            Self::UtilitySequenceDependencies => "utility-sequence-dependencies",
            Self::UtilityParity => "utility-parity",
            Self::UtilityFumen => "utility-fumen",
            Self::UtilityRender => "utility-render",
            Self::UtilityToGray => "utility-to-gray",
            Self::UtilityMirror => "utility-mirror",
        }
    }
}
