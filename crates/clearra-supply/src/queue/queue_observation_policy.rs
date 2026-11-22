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

    /// Whether the ordinary observation window contains every remaining
    /// position of this finite source. `source_len` counts source draws,
    /// including any projected lookahead, rather than remaining locks.
    /// An empty-hold lock can consume two source draws.
    ///
    /// The caller retains the exact source and its end-of-source contract.
    /// This is a sufficient observation certificate, not a complete test
    /// for oracle equivalence: a source-domain owner can additionally prove
    /// that an unobserved suffix is unique under its pattern or bag rules.
    /// An invalid cursor never authorizes such a certificate.
    pub const fn fully_observes_source(self, source_len: usize, source_cursor: usize) -> bool {
        if source_cursor > source_len {
            return false;
        }
        match self.visible_piece_count() {
            None => true,
            Some(visible) => source_len - source_cursor <= visible as usize,
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

    #[test]
    fn full_observation_counts_source_draws_and_preserves_the_p7p4_boundary() {
        let policy = QueueObservationPolicy::VisibleSeven;
        assert!(policy.fully_observes_source(7, 0));
        // Seven locks with an initially empty hold can still use eight
        // draws. The lock count cannot replace source_len here.
        assert!(!policy.fully_observes_source(8, 0));
        assert!(policy.fully_observes_source(8, 1));
        assert!(!policy.fully_observes_source(11, 3));
        assert!(policy.fully_observes_source(11, 4));
        assert!(policy.fully_observes_source(11, 11));
        assert!(policy.fully_observes_source(0, 0));
    }

    #[test]
    fn full_observation_checks_cursor_validity_without_addition_overflow() {
        let policy = QueueObservationPolicy::VisibleSeven;
        assert!(policy.fully_observes_source(usize::MAX, usize::MAX - 7));
        assert!(!policy.fully_observes_source(usize::MAX, usize::MAX - 8));
        assert!(!policy.fully_observes_source(7, 8));
        assert!(QueueObservationPolicy::FullQueueOracle.fully_observes_source(11, 0));
        assert!(!QueueObservationPolicy::FullQueueOracle.fully_observes_source(7, 8));
    }
}
