#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum QueueObservationPolicy {
    #[default]
    FullQueueOracle,
    VisibleSeven,
}

impl QueueObservationPolicy {
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::FullQueueOracle => "oracle",
            Self::VisibleSeven => "visible-7",
        }
    }

    pub const fn coverage_semantics(self) -> &'static str {
        match self {
            Self::FullQueueOracle => "full-future-oracle",
            Self::VisibleSeven => "visible-seven-policy",
        }
    }

    pub const fn visible_piece_count(self) -> Option<u8> {
        match self {
            Self::FullQueueOracle => None,
            Self::VisibleSeven => Some(7),
        }
    }

    pub fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "oracle" | "full-future" => Some(Self::FullQueueOracle),
            "visible-7" | "seven-visible" | "online" => Some(Self::VisibleSeven),
            _ => None,
        }
    }

    pub const fn requires_observation_policy(self) -> bool {
        matches!(self, Self::VisibleSeven)
    }
}

#[cfg(test)]
mod tests {
    use super::QueueObservationPolicy;

    #[test]
    fn keywords_keep_oracle_and_visible_seven_distinct() {
        assert_eq!(
            QueueObservationPolicy::from_keyword("oracle"),
            Some(QueueObservationPolicy::FullQueueOracle)
        );
        assert_eq!(
            QueueObservationPolicy::from_keyword("visible-7"),
            Some(QueueObservationPolicy::VisibleSeven)
        );
        assert_eq!(
            QueueObservationPolicy::VisibleSeven.visible_piece_count(),
            Some(7)
        );
    }
}
