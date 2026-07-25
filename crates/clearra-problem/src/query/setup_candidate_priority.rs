#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SetupCandidatePriority {
    #[default]
    All,
    BuildProbabilityFirst,
    PcProbabilityFirst,
}

impl SetupCandidatePriority {
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::BuildProbabilityFirst => "build",
            Self::PcProbabilityFirst => "pc",
        }
    }

    pub fn from_keyword(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("all") {
            Some(Self::All)
        } else if value.eq_ignore_ascii_case("build") {
            Some(Self::BuildProbabilityFirst)
        } else if value.eq_ignore_ascii_case("pc") {
            Some(Self::PcProbabilityFirst)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SetupCandidatePriority;

    #[test]
    fn keywords_round_trip_case_insensitively() {
        for priority in [
            SetupCandidatePriority::All,
            SetupCandidatePriority::BuildProbabilityFirst,
            SetupCandidatePriority::PcProbabilityFirst,
        ] {
            assert_eq!(
                SetupCandidatePriority::from_keyword(priority.keyword()),
                Some(priority)
            );
            assert_eq!(
                SetupCandidatePriority::from_keyword(&priority.keyword().to_ascii_uppercase()),
                Some(priority)
            );
        }
    }
}
