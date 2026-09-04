// SRP rationale: this module has one behavior-level change reason: representing
// one normalized solution field's score over the complete materialized universe.

use clearra_core_domain::solution::normalized_tiling_solution::{
    NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
};

pub const PC_SCORE_SOLUTION_FIELD_CONTRACT: &str = "pc-score-solution-field-average.v1";
pub const PC_SCORE_SOLUTION_FIELD_ORDERING: &str = "normalized-solution-field-order";
pub const PC_SCORE_SOLUTION_FIELD_AVERAGE_BASIS: &str =
    "whole-materialized-pattern-universe-failed-pc-zero";
pub const PC_SCORE_OVERALL_SCORE_BASIS: &str = "all-materialized-patterns-failed-pc-zero";

/// One row in the ordinary `pc.score` solution-field gallery.
///
/// Every normalized solution field is retained exactly once. `average_score`
/// is that field's weighted score over the complete materialized pattern
/// universe; patterns that the field cannot solve contribute zero. Candidate
/// IDs, trace/attack selectors, and portfolio membership are intentionally not
/// part of this type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcScoreSolutionFieldAverageV1 {
    field_identity: StandardBoard64TilingIdentity,
    average_score_bits: u64,
    covered_pattern_count: usize,
    pattern_count: usize,
    score_complete: bool,
}

impl PcScoreSolutionFieldAverageV1 {
    pub(crate) fn empty(
        field_identity: StandardBoard64TilingIdentity,
        pattern_count: usize,
        score_complete: bool,
    ) -> Option<Self> {
        (pattern_count > 0).then_some(Self {
            field_identity,
            average_score_bits: 0.0_f64.to_bits(),
            covered_pattern_count: 0,
            pattern_count,
            score_complete,
        })
    }

    pub(crate) fn add_pattern_score(&mut self, pattern_weight: f64, score: u64) -> bool {
        if !pattern_weight.is_finite() || !(0.0..=1.0).contains(&pattern_weight) {
            return false;
        }
        let Some(covered_pattern_count) = self.covered_pattern_count.checked_add(1) else {
            return false;
        };
        if covered_pattern_count > self.pattern_count {
            return false;
        }
        let average_score = self.average_score() + pattern_weight * score as f64;
        if !average_score.is_finite() || average_score < 0.0 {
            return false;
        }
        self.average_score_bits = average_score.to_bits();
        self.covered_pattern_count = covered_pattern_count;
        true
    }

    pub const fn contract_id(&self) -> &'static str {
        PC_SCORE_SOLUTION_FIELD_CONTRACT
    }

    pub const fn field_identity(&self) -> StandardBoard64TilingIdentity {
        self.field_identity
    }

    pub fn normalized_field_key(&self) -> NormalizedTilingSolutionKey {
        NormalizedTilingSolutionKey::from_standard_board64_identity(self.field_identity)
    }

    pub const fn average_score_bits(&self) -> u64 {
        self.average_score_bits
    }

    pub const fn average_score(&self) -> f64 {
        f64::from_bits(self.average_score_bits)
    }

    pub const fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }

    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub const fn score_complete(&self) -> bool {
        self.score_complete
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind, solution::normalized_tiling_solution::PiecePlacementMask,
    };

    use super::*;

    #[test]
    fn unsolved_patterns_contribute_zero_to_one_normalized_field_average() {
        let identity = StandardBoard64TilingIdentity::from_placements(
            0,
            [PiecePlacementMask::new(PieceKind::I, 0xf)],
        )
        .expect("one normalized solution field");
        let mut row = PcScoreSolutionFieldAverageV1::empty(identity, 2, true)
            .expect("non-empty materialized universe");

        assert!(row.add_pattern_score(0.25, 100));
        assert_eq!(
            row.normalized_field_key(),
            NormalizedTilingSolutionKey::from_standard_board64_identity(identity)
        );
        assert_eq!(row.average_score(), 25.0);
        assert_eq!(row.covered_pattern_count(), 1);
        assert_eq!(row.pattern_count(), 2);
        assert!(row.score_complete());
    }
}
