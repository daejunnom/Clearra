//! Weighted outcomes over one canonical supply universe.
//!
//! The fixed-queue proof classifies each complete supply identity once. A
//! recovery is additional only after the normal pass for that same queue has
//! completed without a path. Unsearched and state-limited queues retain
//! unknown probability; neither becomes a proof of failure.

use clearra_core_domain::{
    execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind,
    probability::probability_value::ProbabilityValue,
};
use clearra_supply::pattern_universe::materialized_pattern_universe::MaterializedPatternUniverse;

use crate::{
    BoundaryRecoveryError, BoundaryRecoveryQuery, BoundaryRecoveryReport, BoundaryRecoveryStatus,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundaryRecoveryPopulationLimits {
    pub max_pattern_evaluations: usize,
    pub max_total_states: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundaryRecoveryPopulationError {
    InvalidLimits,
    InvalidPatternCount,
    SequenceIdentityMismatch {
        pattern_index: usize,
    },
    InvalidQuery {
        pattern_index: usize,
        cause: BoundaryRecoveryError,
    },
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct BoundaryRecoveryPopulationReport {
    pub materialized_pattern_count: usize,
    pub total_possible_pattern_count: u128,
    pub evaluated_pattern_count: usize,
    pub state_count: usize,
    pub complete: bool,
    pub normal_count: usize,
    pub pc_preserving_recovery_count: usize,
    pub non_pc_recovery_count: usize,
    pub no_path_count: usize,
    pub incomplete_count: usize,
    pub diagram_unavailable_count: usize,
    pub normal_probability: ProbabilityValue,
    pub pc_preserving_recovery_probability: ProbabilityValue,
    pub non_pc_recovery_probability: ProbabilityValue,
    pub additional_recovery_probability: ProbabilityValue,
    pub total_response_probability: ProbabilityValue,
    pub no_path_probability: ProbabilityValue,
    /// Materialized incomplete/unevaluated outcomes plus any unmaterialized
    /// universe tail. This is not counted as failure.
    pub unknown_probability: ProbabilityValue,
    pub normal_example: Option<(usize, Vec<PieceKind>, BoundaryRecoveryReport)>,
    pub recovery_example: Option<(usize, Vec<PieceKind>, BoundaryRecoveryReport)>,
}

/// The caller supplies a query for each *complete* sequence. A role-mapping
/// adapter must bind diagram roles to that sequence before this function is
/// called; reusing fixed source-position roles after a pattern permutation
/// would silently change the user's diagram. `None` is a proven role mismatch
/// for this sequence and counts as no path without starting the search.
pub fn search_boundary_recovery_population(
    universe: &MaterializedPatternUniverse,
    control: &ExecutionControl,
    limits: BoundaryRecoveryPopulationLimits,
    mut query_for_sequence: impl FnMut(
        usize,
        &[PieceKind],
    )
        -> Result<Option<BoundaryRecoveryQuery>, BoundaryRecoveryError>,
) -> Result<BoundaryRecoveryPopulationReport, BoundaryRecoveryPopulationError> {
    if limits.max_pattern_evaluations == 0 || limits.max_total_states == 0 {
        return Err(BoundaryRecoveryPopulationError::InvalidLimits);
    }
    let pattern_count = universe.pattern_count();
    if pattern_count == 0 || pattern_count > u32::MAX as usize {
        return Err(BoundaryRecoveryPopulationError::InvalidPatternCount);
    }

    let mut normal = 0.0_f64;
    let mut pc_recovery = 0.0_f64;
    let mut non_pc_recovery = 0.0_f64;
    let mut no_path = 0.0_f64;
    let mut counts = [0_usize; 5];
    let mut diagram_unavailable = 0_usize;
    let mut evaluated = 0_usize;
    let mut states = 0_usize;
    let mut normal_example = None;
    let mut recovery_example = None;
    while evaluated < pattern_count
        && evaluated < limits.max_pattern_evaluations
        && states < limits.max_total_states
    {
        if control.is_cancelled() {
            return Err(BoundaryRecoveryPopulationError::Cancelled);
        }
        let sequence = universe.sequence_at(evaluated);
        let query = query_for_sequence(evaluated, &sequence).map_err(|cause| {
            BoundaryRecoveryPopulationError::InvalidQuery {
                pattern_index: evaluated,
                cause,
            }
        })?;
        let weight = universe.weight_at(evaluated).get();
        let Some(mut query) = query else {
            counts[3] += 1;
            diagram_unavailable += 1;
            no_path += weight;
            evaluated += 1;
            continue;
        };
        // The adapter may not substitute a different queue after seeing its
        // pattern ID. Supply identity and probability must remain paired.
        if query.queue != sequence.as_ref() {
            return Err(BoundaryRecoveryPopulationError::SequenceIdentityMismatch {
                pattern_index: evaluated,
            });
        }
        query.max_states = query
            .max_states
            .min(limits.max_total_states.saturating_sub(states));
        let report = query.search(control).map_err(|cause| match cause {
            BoundaryRecoveryError::Cancelled => BoundaryRecoveryPopulationError::Cancelled,
            cause => BoundaryRecoveryPopulationError::InvalidQuery {
                pattern_index: evaluated,
                cause,
            },
        })?;
        states = states.saturating_add(report.normal_states.saturating_add(report.recovery_states));
        match report.status {
            BoundaryRecoveryStatus::Normal => {
                counts[0] += 1;
                normal += weight;
                if normal_example.is_none() {
                    normal_example = Some((evaluated, sequence.to_vec(), report));
                }
            }
            BoundaryRecoveryStatus::PcPreservingRecovery => {
                counts[1] += 1;
                pc_recovery += weight;
                if recovery_example.is_none() {
                    recovery_example = Some((evaluated, sequence.to_vec(), report));
                }
            }
            BoundaryRecoveryStatus::NonPcRecovery => {
                counts[2] += 1;
                non_pc_recovery += weight;
                if recovery_example.is_none() {
                    recovery_example = Some((evaluated, sequence.to_vec(), report));
                }
            }
            BoundaryRecoveryStatus::NoPath => {
                counts[3] += 1;
                no_path += weight;
            }
            BoundaryRecoveryStatus::Incomplete => counts[4] += 1,
        }
        evaluated += 1;
    }
    let known = normal + pc_recovery + non_pc_recovery + no_path;
    let probability = |value: f64| {
        ProbabilityValue::new(value.clamp(0.0, 1.0))
            .expect("validated universe weights remain finite")
    };
    let complete = universe.complete() && evaluated == pattern_count && counts[4] == 0;
    Ok(BoundaryRecoveryPopulationReport {
        materialized_pattern_count: pattern_count,
        total_possible_pattern_count: universe.total_possible_pattern_count(),
        evaluated_pattern_count: evaluated,
        state_count: states,
        complete,
        normal_count: counts[0],
        pc_preserving_recovery_count: counts[1],
        non_pc_recovery_count: counts[2],
        no_path_count: counts[3],
        incomplete_count: counts[4],
        diagram_unavailable_count: diagram_unavailable,
        normal_probability: probability(normal),
        pc_preserving_recovery_probability: probability(pc_recovery),
        non_pc_recovery_probability: probability(non_pc_recovery),
        additional_recovery_probability: probability(pc_recovery + non_pc_recovery),
        total_response_probability: probability(normal + pc_recovery + non_pc_recovery),
        no_path_probability: probability(no_path),
        unknown_probability: if complete {
            ProbabilityValue::ZERO
        } else {
            probability(1.0 - known)
        },
        normal_example,
        recovery_example,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::{
        board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionCancellationToken,
    };
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };
    use clearra_rules::profile::rule_profile::RuleProfileId;
    use clearra_scoring::profile::SpinProfileId;

    #[test]
    fn weighted_queue_union_counts_normal_success_once_and_keeps_failure_separate() {
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(1),
            PatternWeightModelId::new(2),
            vec![
                vec![PieceKind::I, PieceKind::O],
                vec![PieceKind::O, PieceKind::I],
            ],
            vec![
                ProbabilityValue::new(0.2).unwrap(),
                ProbabilityValue::new(0.8).unwrap(),
            ],
            2,
            true,
            None,
        )
        .unwrap();
        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let result = search_boundary_recovery_population(
            &universe,
            &control,
            BoundaryRecoveryPopulationLimits {
                max_pattern_evaluations: 2,
                max_total_states: 20_000,
            },
            |_, sequence| {
                Ok(Some(BoundaryRecoveryQuery {
                    queue: sequence.to_vec(),
                    ..query_for_test()
                }))
            },
        )
        .unwrap();
        assert!(result.complete);
        assert_eq!((result.normal_count, result.no_path_count), (1, 1));
        assert!((result.normal_probability.get() - 0.2).abs() < 1e-12);
        assert!((result.total_response_probability.get() - 0.2).abs() < 1e-12);
        assert!((result.no_path_probability.get() - 0.8).abs() < 1e-12);
        assert_eq!(result.unknown_probability, ProbabilityValue::ZERO);

        let truncated = search_boundary_recovery_population(
            &universe,
            &control,
            BoundaryRecoveryPopulationLimits {
                max_pattern_evaluations: 1,
                max_total_states: 20_000,
            },
            |_, sequence| {
                Ok(Some(BoundaryRecoveryQuery {
                    queue: sequence.to_vec(),
                    ..query_for_test()
                }))
            },
        )
        .unwrap();
        assert!(!truncated.complete);
        assert_eq!(truncated.no_path_count, 0);
        assert!((truncated.unknown_probability.get() - 0.8).abs() < 1e-12);
    }

    #[test]
    fn non_pc_recovery_adds_only_its_own_queue_weight_after_proven_normal_failure() {
        let universe = MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(3),
            PatternWeightModelId::new(4),
            vec![vec![PieceKind::I, PieceKind::O, PieceKind::T]],
            vec![ProbabilityValue::ONE],
            1,
            true,
            None,
        )
        .unwrap();
        let control = ExecutionControl::new(ExecutionCancellationToken::new());
        let result = search_boundary_recovery_population(
            &universe,
            &control,
            BoundaryRecoveryPopulationLimits {
                max_pattern_evaluations: 1,
                max_total_states: 20_000,
            },
            |_, sequence| {
                let mut query = query_for_test();
                query.queue = sequence.to_vec();
                query.placement_role_masks = vec![
                    Board256Mask::from_words([0xf, 0, 0, 0]),
                    Board256Mask::from_words([0x300c000, 0, 0, 0]),
                ];
                query.max_early_placements = 1;
                query.borrow_role_index = 1;
                query.borrow_placement_mask = query.placement_role_masks[1];
                query.hold_enabled = true;
                Ok(Some(query))
            },
        )
        .unwrap();
        assert!(result.complete);
        assert_eq!((result.normal_count, result.non_pc_recovery_count), (0, 1));
        assert_eq!(result.normal_probability, ProbabilityValue::ZERO);
        assert_eq!(
            result.additional_recovery_probability,
            ProbabilityValue::ONE
        );
        assert_eq!(result.total_response_probability, ProbabilityValue::ONE);
        assert_eq!(result.unknown_probability, ProbabilityValue::ZERO);
    }

    fn query_for_test() -> BoundaryRecoveryQuery {
        BoundaryRecoveryQuery {
            initial_board: Board256Mask::from_words([0x3f0, 0, 0, 0]),
            final_board: Board256Mask::from_words([0xc030, 0, 0, 0]),
            height: 4,
            queue: Vec::new(),
            stage_one_queue_len: 1,
            required_placements: 2,
            placement_role_masks: Vec::new(),
            placement_role_pieces: Vec::new(),
            max_early_placements: 0,
            borrow_role_index: 0,
            borrow_placement_mask: Board256Mask::EMPTY,
            hold_enabled: false,
            rule_profile: RuleProfileId::SrsPlus,
            spin_profile: SpinProfileId::AllSpinPlus,
            preserve_b2b_by_stage: [false, false],
            preserve_b2b_bag_mask: 0,
            initial_b2b: true,
            max_states: 10_000,
        }
    }
}
