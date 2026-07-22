#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "kebab-case"))]
pub enum AppCommandKind {
    Pc,
    Path,
    Percent,
    Setup,
    BuildProbability,
    Damage,
    SpinFinder,
    Cover,
    Continue,
    Rules,
    Scoring,
    Convert,
    InspectUnsupported,
    Verify,
    VerifyKicks,
}

impl AppCommandKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pc => "pc",
            Self::Path => "path",
            Self::Percent => "percent",
            Self::Setup => "setup",
            Self::BuildProbability => "build-probability",
            Self::Damage => "damage",
            Self::SpinFinder => "spin-finder",
            Self::Cover => "cover",
            Self::Continue => "continue",
            Self::Rules => "rules",
            Self::Scoring => "scoring",
            Self::Convert => "convert",
            Self::InspectUnsupported => "inspect-unsupported",
            Self::Verify => "verify",
            Self::VerifyKicks => "verify-kicks",
        }
    }
}
