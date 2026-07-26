use super::setup_candidate_priority::SetupCandidatePriority;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SetupLengthPreference {
    #[default]
    Auto,
    Longer,
    Shorter,
}

impl SetupLengthPreference {
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Longer => "longer",
            Self::Shorter => "shorter",
        }
    }

    pub fn from_keyword(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("auto") {
            Some(Self::Auto)
        } else if value.eq_ignore_ascii_case("longer") {
            Some(Self::Longer)
        } else if value.eq_ignore_ascii_case("shorter") {
            Some(Self::Shorter)
        } else {
            None
        }
    }

    pub const fn resolve(self, priority: SetupCandidatePriority) -> Self {
        match self {
            Self::Auto => match priority {
                SetupCandidatePriority::All | SetupCandidatePriority::BuildProbabilityFirst => {
                    Self::Longer
                }
                SetupCandidatePriority::PcProbabilityFirst => Self::Shorter,
            },
            explicit => explicit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_round_trip_and_auto_follows_probability_priority() {
        for preference in [
            SetupLengthPreference::Auto,
            SetupLengthPreference::Longer,
            SetupLengthPreference::Shorter,
        ] {
            assert_eq!(
                SetupLengthPreference::from_keyword(preference.keyword()),
                Some(preference)
            );
        }
        assert_eq!(
            SetupLengthPreference::Auto.resolve(SetupCandidatePriority::BuildProbabilityFirst),
            SetupLengthPreference::Longer
        );
        assert_eq!(
            SetupLengthPreference::Auto.resolve(SetupCandidatePriority::PcProbabilityFirst),
            SetupLengthPreference::Shorter
        );
        assert_eq!(
            SetupLengthPreference::Auto.resolve(SetupCandidatePriority::All),
            SetupLengthPreference::Longer
        );
    }
}
