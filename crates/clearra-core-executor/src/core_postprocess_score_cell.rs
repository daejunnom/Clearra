use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CorePostProcessScoreCell {
    candidate_identity: StandardBoard64TilingIdentity,
    pattern_id: usize,
    trace_identity: String,
    score: u64,
    attack: u32,
}

impl CorePostProcessScoreCell {
    pub fn new(
        candidate_identity: StandardBoard64TilingIdentity,
        pattern_id: usize,
        trace_identity: impl Into<String>,
        score: u64,
        attack: u32,
    ) -> Self {
        Self {
            candidate_identity,
            pattern_id,
            trace_identity: trace_identity.into(),
            score,
            attack,
        }
    }

    pub const fn candidate_identity(&self) -> StandardBoard64TilingIdentity {
        self.candidate_identity
    }

    pub const fn pattern_id(&self) -> usize {
        self.pattern_id
    }

    pub fn trace_identity(&self) -> &str {
        &self.trace_identity
    }

    pub const fn score(&self) -> u64 {
        self.score
    }

    pub const fn attack(&self) -> u32 {
        self.attack
    }
}
