use crate::model::{
    MinimalityPolicy, SpinStructureOutcome, SpinStructureReport, SpinStructureStageMetrics,
    StructureOperation,
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct OutcomeKey {
    operations: Vec<StructureOperation>,
    target: StructureOperation,
    target_cleared_rows: u32,
}

pub(crate) fn finalize(mut report: SpinStructureReport) -> SpinStructureReport {
    let minimality = report
        .query
        .as_ref()
        .map_or(MinimalityPolicy::SubsetMinimal, |query| query.minimality);
    let mut outcomes = report
        .regular
        .drain(..)
        .chain(report.mini.drain(..))
        .collect::<Vec<_>>();
    outcomes.sort_by_key(|outcome| (outcome.build.len(), outcome_key(outcome), outcome.mini));
    let before = outcomes.len();
    outcomes.dedup_by(|left, right| outcome_key(left) == outcome_key(right));
    report.stages.exact_outcome_deduplications += (before - outcomes.len()) as u64;

    match minimality {
        MinimalityPolicy::MinimumPieceCount => {
            if let Some(minimum) = outcomes.iter().map(|outcome| outcome.build.len()).min() {
                outcomes.retain(|outcome| outcome.build.len() == minimum);
            }
        }
        MinimalityPolicy::SubsetMinimal => {
            let mut retained: Vec<SpinStructureOutcome> = Vec::new();
            for outcome in outcomes {
                let candidate = operation_keys(&outcome);
                if retained.iter().any(|known| {
                    known.logical_spin == outcome.logical_spin
                        && known.logical_spin_cleared_rows == outcome.logical_spin_cleared_rows
                        && multiset_subset(&operation_keys(known), &candidate)
                }) {
                    report.stages.exact_outcome_deduplications += 1;
                } else {
                    retained.push(outcome);
                }
            }
            outcomes = retained;
        }
    }

    report.minimum_placements = outcomes
        .iter()
        .map(|outcome| outcome.build.len() as u8)
        .min();
    for outcome in outcomes {
        if outcome.mini {
            report.mini.push(outcome);
        } else {
            report.regular.push(outcome);
        }
    }
    report.regular.sort_by_key(outcome_key);
    report.mini.sort_by_key(outcome_key);
    report
}

pub(crate) fn merge_stage_metrics(
    target: &mut SpinStructureStageMetrics,
    source: SpinStructureStageMetrics,
) {
    target.build_states += source.build_states;
    target.fill_checks += source.fill_checks;
    target.support_locks += source.support_locks;
    target.corner_checks += source.corner_checks;
    target.entry_states += source.entry_states;
    target.verification_checks += source.verification_checks;
    target.exact_state_deduplications += source.exact_state_deduplications;
    target.exact_outcome_deduplications += source.exact_outcome_deduplications;
}

fn outcome_key(outcome: &SpinStructureOutcome) -> OutcomeKey {
    OutcomeKey {
        operations: operation_keys(outcome),
        target: outcome.logical_spin.clone(),
        target_cleared_rows: outcome.logical_spin_cleared_rows,
    }
}

fn operation_keys(outcome: &SpinStructureOutcome) -> Vec<StructureOperation> {
    let mut keys = outcome.logical_operations.clone();
    keys.sort();
    keys
}

fn multiset_subset(left: &[StructureOperation], right: &[StructureOperation]) -> bool {
    if left.len() >= right.len() {
        return false;
    }
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        match left[left_index].cmp(&right[right_index]) {
            std::cmp::Ordering::Less => return false,
            std::cmp::Ordering::Equal => {
                left_index += 1;
                right_index += 1;
            }
            std::cmp::Ordering::Greater => right_index += 1,
        }
    }
    left_index == left.len()
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_replay::ScoringLockEvidence;

    use super::*;
    use crate::{StructureBoard, StructurePlacement};

    fn operation(piece: PieceKind, x: i8) -> StructureOperation {
        StructureOperation::new(
            piece,
            RotationState::Zero,
            x,
            0,
            StructureBoard::from_rows(&[0b1111]).expect("mask"),
            0,
        )
    }

    fn outcome(
        operations: Vec<StructureOperation>,
        target: StructureOperation,
        target_cleared_rows: u32,
        mini: bool,
    ) -> SpinStructureOutcome {
        let build = operations
            .iter()
            .map(|operation| StructurePlacement {
                piece: operation.piece(),
                rotation: operation.rotation(),
                x: operation.x(),
                y: operation.y(),
                mask_before_clear: operation.mask(),
                cleared_rows: 0,
                cleared_lines: 0,
                evidence: ScoringLockEvidence::no_rotation(operation.rotation()),
            })
            .collect();
        SpinStructureOutcome {
            board_before_spin: StructureBoard::EMPTY,
            final_board: StructureBoard::EMPTY,
            spin: StructurePlacement {
                piece: target.piece(),
                rotation: target.rotation(),
                x: target.x(),
                y: target.y(),
                mask_before_clear: target.mask(),
                cleared_rows: 0,
                cleared_lines: target_cleared_rows.count_ones() as u8,
                evidence: ScoringLockEvidence::no_rotation(target.rotation()),
            },
            build,
            mini,
            logical_operations: operations,
            logical_spin: target,
            logical_spin_cleared_rows: target_cleared_rows,
        }
    }

    fn report_with(
        subset: SpinStructureOutcome,
        superset: SpinStructureOutcome,
    ) -> SpinStructureReport {
        SpinStructureReport {
            regular: vec![superset],
            mini: vec![subset],
            complete: true,
            ..SpinStructureReport::default()
        }
    }

    #[test]
    fn multiset_subset_is_strict_and_multiplicity_preserving() {
        let a = operation(PieceKind::I, 0);
        let b = operation(PieceKind::T, 1);
        assert!(multiset_subset(
            std::slice::from_ref(&a),
            &[a.clone(), b.clone()]
        ));
        assert!(!multiset_subset(&[a.clone(), a], &[b.clone(), b]));
    }

    #[test]
    fn subset_minimality_is_independent_of_regular_or_mini_classification() {
        let target = operation(PieceKind::T, 1);
        let roof = operation(PieceKind::I, 0);
        let subset = outcome(vec![target], target, 1 << 0, true);
        let superset = outcome(vec![target, roof], target, 1 << 0, false);

        let report = finalize(report_with(subset, superset));

        assert_eq!(report.outcome_count(), 1);
        assert_eq!(report.mini.len(), 1);
        assert!(report.regular.is_empty());
    }

    #[test]
    fn target_cleared_row_signature_preserves_a_double_from_a_single_subset() {
        let target = operation(PieceKind::T, 1);
        let second_line_piece = operation(PieceKind::I, 0);
        let single = outcome(vec![target], target, 1 << 0, true);
        let double = outcome(
            vec![target, second_line_piece],
            target,
            (1 << 0) | (1 << 1),
            false,
        );

        let report = finalize(report_with(single, double));

        assert_eq!(report.outcome_count(), 2);
        assert_eq!(report.mini.len(), 1);
        assert_eq!(report.regular.len(), 1);
        assert_eq!(report.mini[0].logical_spin_cleared_rows(), 1 << 0);
        assert_eq!(
            report.regular[0].logical_spin_cleared_rows(),
            (1 << 0) | (1 << 1)
        );
    }

    #[test]
    fn target_cleared_row_signature_is_part_of_exact_outcome_identity() {
        let target = operation(PieceKind::T, 1);
        let first_row = outcome(vec![target], target, 1 << 0, true);
        let second_row = outcome(vec![target], target, 1 << 1, false);

        let report = finalize(report_with(first_row, second_row));

        assert_eq!(report.outcome_count(), 2);
        assert_eq!(
            report
                .outcomes()
                .map(SpinStructureOutcome::logical_spin_cleared_rows)
                .collect::<std::collections::BTreeSet<_>>(),
            [1 << 0, 1 << 1].into_iter().collect()
        );
    }
}
