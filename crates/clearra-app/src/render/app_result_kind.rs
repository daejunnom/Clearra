#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppResultKind {
    Pc,
    Scenario,
    Path,
    Percent,
    Setup,
    BuildProbability,
    Damage,
    SpinFinder,
    SpinStructure,
    Cover,
    Rules,
    Scoring,
    Convert,
    Continue,
    Verify,
    VerifyKicks,
}

impl AppResultKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pc => "pc",
            Self::Scenario => "pc-scenario",
            Self::Path => "path",
            Self::Percent => "percent",
            Self::Setup => "setup",
            Self::BuildProbability => "build-probability",
            Self::Damage => "damage",
            Self::SpinFinder => "spin-finder",
            Self::SpinStructure => "spin-structure",
            Self::Cover => "build_coverage",
            Self::Rules => "rules",
            Self::Scoring => "scoring",
            Self::Convert => "convert",
            Self::Continue => "continue",
            Self::Verify => "verify",
            Self::VerifyKicks => "verify-kicks",
        }
    }
}
