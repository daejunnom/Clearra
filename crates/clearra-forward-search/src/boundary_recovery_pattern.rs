//! Pattern-wide recovery over the same complete supply identities and weights.
//! Only complete seven-piece bag diagrams have an unambiguous role mapping.

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_supply::{
    pattern_universe::pattern_universe_materializer::PatternUniverseMaterializer,
    queue::queue_pattern_expression::QueuePatternExpression,
};

use crate::{
    search_boundary_recovery_population, BoundaryRecoveryBagRoleError, BoundaryRecoveryBagRolePlan,
    BoundaryRecoveryPopulationError, BoundaryRecoveryPopulationLimits,
    BoundaryRecoveryPopulationReport, BoundaryRecoveryQuery,
};

const MAX_PATTERN_IDENTITIES: usize = 100_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundaryRecoveryPatternQuery {
    pub reference: BoundaryRecoveryQuery,
    pub queue_pattern: String,
    pub max_pattern_evaluations: usize,
    pub max_total_states: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundaryRecoveryPatternError {
    InvalidPattern,
    PatternLengthMismatch,
    InvalidRolePlan(BoundaryRecoveryBagRoleError),
    InvalidUniverse,
    Population(BoundaryRecoveryPopulationError),
}

impl BoundaryRecoveryPatternQuery {
    pub fn search(
        &self,
        control: &ExecutionControl,
    ) -> Result<BoundaryRecoveryPopulationReport, BoundaryRecoveryPatternError> {
        let roles = BoundaryRecoveryBagRolePlan::new(self.reference.clone())
            .map_err(BoundaryRecoveryPatternError::InvalidRolePlan)?;
        let expression = QueuePatternExpression::parse(&self.queue_pattern, MAX_PATTERN_IDENTITIES)
            .map_err(|_| BoundaryRecoveryPatternError::InvalidPattern)?;
        if expression.sequence_len() != self.reference.queue.len() {
            return Err(BoundaryRecoveryPatternError::PatternLengthMismatch);
        }
        let universe = PatternUniverseMaterializer::queue_pattern_expression(&expression, 0)
            .map_err(|_| BoundaryRecoveryPatternError::InvalidUniverse)?;
        search_boundary_recovery_population(
            &universe,
            control,
            BoundaryRecoveryPopulationLimits {
                max_pattern_evaluations: self.max_pattern_evaluations,
                max_total_states: self.max_total_states,
            },
            |_, sequence| Ok(roles.query_for_sequence(sequence)),
        )
        .map_err(BoundaryRecoveryPatternError::Population)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::{
        board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionCancellationToken,
        piece::piece_kind::PieceKind,
    };
    use clearra_rules::profile::rule_profile::RuleProfileId;
    use clearra_scoring::profile::SpinProfileId;

    #[test]
    fn one_full_adjacent_bag_uses_canonical_5040_supply_universe_without_false_failure() {
        let bag = [
            PieceKind::I,
            PieceKind::J,
            PieceKind::L,
            PieceKind::O,
            PieceKind::S,
            PieceKind::T,
            PieceKind::Z,
        ];
        let reference = BoundaryRecoveryQuery {
            initial_board: Board256Mask::EMPTY,
            final_board: Board256Mask::EMPTY,
            height: 8,
            queue: bag.into_iter().chain(bag).collect(),
            stage_one_queue_len: 7,
            required_placements: 14,
            placement_role_masks: vec![Board256Mask::from_words([0xf, 0, 0, 0]); 14],
            placement_role_pieces: Vec::new(),
            max_early_placements: 0,
            borrow_role_index: 0,
            borrow_placement_mask: Board256Mask::EMPTY,
            hold_enabled: false,
            rule_profile: RuleProfileId::SrsPlus,
            spin_profile: SpinProfileId::AllSpinPlus,
            preserve_b2b_by_stage: [false; 2],
            preserve_b2b_bag_mask: 0,
            initial_b2b: true,
            max_states: 1,
        };
        let query = BoundaryRecoveryPatternQuery {
            reference,
            queue_pattern: "IJLOSTZP7".to_owned(),
            max_pattern_evaluations: 1,
            max_total_states: 1,
        };
        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let report = query.search(&control).unwrap();
        assert_eq!(report.total_possible_pattern_count, 5040);
        assert_eq!(report.evaluated_pattern_count, 1);
        assert!(!report.complete);
        assert_eq!(report.incomplete_count, 1);
        assert_eq!(report.no_path_count, 0);
        assert!(report.unknown_probability.get() > 0.99);
    }
}
