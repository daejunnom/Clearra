use clearra_core_domain::solution::normalized_tiling_solution::{
    NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
};

pub const PC_SCORE_PATTERN_WINNER_CONTRACT: &str = "pc-score-pattern-winner.v1";
pub const PC_SCORE_INFORMATIONAL_ATTACK_BASIS: &str = "canonical-equal-score-trace";
pub const PC_SCORE_CANONICAL_SELECTION: &str = "smallest-canonical-candidate-id";

pub(crate) fn canonical_score_winner(
    winners: &[PcScorePatternWinnerV1],
) -> Option<PcScorePatternWinnerV1> {
    winners
        .iter()
        .copied()
        .min_by_key(PcScorePatternWinnerV1::candidate_id)
}

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

#[cfg(test)]
mod tests {
    use super::{canonical_score_winner, PcScorePatternWinnerV1};
    use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;

    #[test]
    fn canonical_score_winner_uses_numeric_candidate_id_and_never_attack() {
        let identity = StandardBoard64TilingIdentity::from_placements(0, std::iter::empty())
            .expect("empty identity");
        let winners = [
            PcScorePatternWinnerV1::new(0, 10, identity, 1_200, 0),
            PcScorePatternWinnerV1::new(1, 2, identity, 1_200, 999),
            PcScorePatternWinnerV1::new(2, 3, identity, 1_200, 1),
        ];

        assert_eq!(
            canonical_score_winner(&winners).map(|winner| winner.candidate_id()),
            Some(2)
        );
    }
}
