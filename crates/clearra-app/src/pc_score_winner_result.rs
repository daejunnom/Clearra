use clearra_core_domain::solution::normalized_tiling_solution::{
    NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
};

pub const PC_SCORE_PATTERN_WINNER_CONTRACT: &str = "pc-score-pattern-winner.v1";
pub const PC_SCORE_INFORMATIONAL_ATTACK_BASIS: &str = "canonical-equal-score-trace";

/// One candidate in the complete maximum-score family for a materialized
/// supply pattern.
///
/// `score` is the sole ordering and equality authority. The attack value is
/// copied from the canonical trace chosen after an exact score tie and is
/// therefore informational only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcScorePatternWinnerV1 {
    pattern_id: usize,
    candidate_id: u64,
    solution_identity: StandardBoard64TilingIdentity,
    score: u64,
    informational_attack: u32,
}

impl PcScorePatternWinnerV1 {
    pub(crate) const fn new(
        pattern_id: usize,
        candidate_id: u64,
        solution_identity: StandardBoard64TilingIdentity,
        score: u64,
        informational_attack: u32,
    ) -> Self {
        Self {
            pattern_id,
            candidate_id,
            solution_identity,
            score,
            informational_attack,
        }
    }

    pub const fn contract_id(&self) -> &'static str {
        PC_SCORE_PATTERN_WINNER_CONTRACT
    }

    pub const fn pattern_id(&self) -> usize {
        self.pattern_id
    }

    /// Exact numeric candidate identity. JSON surfaces encode this value as a
    /// canonical base-10 string so JavaScript never rounds it.
    pub const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }

    pub const fn solution_identity(&self) -> StandardBoard64TilingIdentity {
        self.solution_identity
    }

    pub fn normalized_solution_key(&self) -> NormalizedTilingSolutionKey {
        NormalizedTilingSolutionKey::from_standard_board64_identity(self.solution_identity)
    }

    pub const fn score(&self) -> u64 {
        self.score
    }

    pub const fn informational_attack(&self) -> u32 {
        self.informational_attack
    }

    pub const fn informational_attack_basis(&self) -> &'static str {
        PC_SCORE_INFORMATIONAL_ATTACK_BASIS
    }
}
