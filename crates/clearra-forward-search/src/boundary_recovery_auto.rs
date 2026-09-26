//! Automatic terminal horizons. Cell conservation is only a necessary test;
//! every retained horizon still passes the unchanged legal Build/hold search.
use super::*;

fn candidates(query: &BoundaryRecoveryQuery) -> Vec<usize> {
    let (initial, _, _) = place_and_clear(
        10,
        query.height,
        ForwardBoard::from_mask(query.initial_board),
    );
    let occupied = initial
        .words()
        .iter()
        .map(|word| word.count_ones())
        .sum::<u32>();
    let target = query
        .final_board
        .words()
        .iter()
        .map(|word| word.count_ones())
        .sum::<u32>();
    let horizon = query.placement_horizon();
    let first = if query.placement_role_masks.is_empty() {
        query.stage_one_queue_len + 1
    } else {
        horizon
    };
    (first..=horizon)
        .filter(|count| {
            // Four cells enter at every lock and ten leave per cleared row.
            let available = occupied + 4 * (*count as u32);
            available >= target && (available - target) % 10 == 0
        })
        .collect()
}

pub(super) fn search(
    source: &BoundaryRecoveryQuery,
    control: &ExecutionControl,
) -> Result<BoundaryRecoveryReport, BoundaryRecoveryError> {
    if control.is_cancelled() {
        return Err(BoundaryRecoveryError::Cancelled);
    }
    let horizons = candidates(source);
    let mut normal_states = 0_usize;
    let mut recovery_states = 0_usize;
    // Never classify recovery from an early horizon before ruling out a
    // normal connection at every other horizon. Unknown stays unknown.
    for borrowed in 0..=source.max_early_placements {
        for &count in &horizons {
            if control.is_cancelled() {
                return Err(BoundaryRecoveryError::Cancelled);
            }
            if borrowed > 0 && source.borrow_role_index >= count {
                continue;
            }
            let mut query = source.clone();
            query.required_placements = Some(count);
            query.max_early_placements = borrowed;
            // A policy on a future, unconsumed bag does not require consuming it.
            query.preserve_b2b_bag_mask &= (1_u64 << query.bag_count()) - 1;
            let used = normal_states.saturating_add(recovery_states);
            query.max_states = source.max_states.map(|limit| limit.saturating_sub(used));
            if query.max_states == Some(0) {
                return Ok(report(
                    PassResult::Incomplete,
                    normal_states,
                    recovery_states,
                    borrowed > 0,
                ));
            }
            let (outcome, states) = Pass::new(&query, control, borrowed)?.run()?;
            if borrowed == 0 {
                normal_states = normal_states.saturating_add(states);
            } else {
                recovery_states = recovery_states.saturating_add(states);
            }
            if !matches!(outcome, PassResult::NoPath) {
                return Ok(report(
                    outcome,
                    normal_states,
                    recovery_states,
                    borrowed > 0,
                ));
            }
        }
    }
    Ok(report(
        PassResult::NoPath,
        normal_states,
        recovery_states,
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;

    fn query(queue: &[PieceKind], initial: u64, target: u64) -> BoundaryRecoveryQuery {
        BoundaryRecoveryQuery {
            initial_board: Board256Mask::from_words([initial, 0, 0, 0]),
            stage_one_target: Board256Mask::from_words([0xf, 0, 0, 0]),
            final_board: Board256Mask::from_words([target, 0, 0, 0]),
            height: 4,
            queue: queue.to_vec(),
            stage_one_queue_len: 1,
            required_placements: None,
            placement_role_masks: Vec::new(),
            placement_role_pieces: Vec::new(),
            max_early_placements: 0,
            borrow_role_index: 1,
            borrow_placement_mask: Board256Mask::EMPTY,
            hold_enabled: false,
            rule_profile: RuleProfileId::SrsPlus,
            spin_profile: SpinProfileId::AllSpinPlus,
            preserve_b2b_by_stage: [false; 2],
            preserve_b2b_bag_mask: 0,
            initial_b2b: false,
            max_states: None,
        }
    }

    #[test]
    fn automatic_count_preserves_lookahead_instead_of_consuming_the_whole_queue() {
        let query = query(&[PieceKind::I, PieceKind::O, PieceKind::T], 0, 0xc03f);
        let result = query.search(&ExecutionControl::default()).unwrap();
        assert_eq!(result.status, BoundaryRecoveryStatus::Normal);
        assert_eq!(result.steps.len(), 2);
        assert_eq!(result.steps.last().unwrap().board_after, [0xc03f, 0, 0, 0]);
        assert!(result.steps.iter().all(|step| step.source_queue_index < 2));
    }

    #[test]
    fn automatic_count_includes_real_line_clears_and_nonempty_checkpoints() {
        let mut query = query(&[PieceKind::I, PieceKind::O, PieceKind::T], 0x3f0, 0xc030);
        query.stage_one_target = Board256Mask::EMPTY;
        let result = query.search(&ExecutionControl::default()).unwrap();
        assert_eq!(result.status, BoundaryRecoveryStatus::Normal);
        assert_eq!(result.steps.len(), 2);
        assert_eq!(result.steps[0].cleared_lines, 1);
        assert_eq!(result.steps.last().unwrap().board_after, [0xc030, 0, 0, 0]);
    }

    #[test]
    fn automatic_mode_retains_all_feasible_horizons_and_respects_explicit_limits() {
        let query = query(&[PieceKind::I; 7], 0, 8);
        // No geometric claim: 2 and 7 locks have the same area residue.
        assert!(candidates(&query).is_empty()); // one target cell has no compatible residue
        let mut valid = query.clone();
        valid.final_board = Board256Mask::from_words([0xff, 0, 0, 0]);
        assert_eq!(candidates(&valid), vec![2, 7]);
        valid.max_states = Some(1);
        assert_eq!(
            valid.search(&ExecutionControl::default()).unwrap().status,
            BoundaryRecoveryStatus::Incomplete
        );
        valid.max_states = Some(0);
        assert_eq!(
            valid.search(&ExecutionControl::default()),
            Err(BoundaryRecoveryError::InvalidStateLimit)
        );
    }

    #[test]
    fn automatic_mode_does_not_choose_only_the_smallest_area_compatible_count() {
        // Two I pieces cannot make the final I+O field. Four I pieces and an
        // O can clear two rows first; the final I and O then reach the target.
        let mut input = query(
            &[
                PieceKind::I,
                PieceKind::I,
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::I,
                PieceKind::O,
            ],
            0,
            0xc03f,
        );
        input.height = 2;
        assert_eq!(candidates(&input), vec![2, 7]);
        let mut exact = input.clone();
        exact.required_placements = Some(2);
        assert_eq!(
            exact.search(&ExecutionControl::default()).unwrap().status,
            BoundaryRecoveryStatus::NoPath
        );
        exact.required_placements = Some(7);
        let expected = exact.search(&ExecutionControl::default()).unwrap();
        assert_eq!(expected.status, BoundaryRecoveryStatus::Normal);
        assert_eq!(expected.steps.len(), 7);
        let actual = input.search(&ExecutionControl::default()).unwrap();
        assert_eq!(actual.status, expected.status);
        assert_eq!(actual.steps, expected.steps);
    }

    #[test]
    fn unlimited_state_budget_does_not_disable_cancellation() {
        let query = query(&[PieceKind::I, PieceKind::O], 0, 0xc03f);
        let token = ExecutionCancellationToken::new();
        token.handle().cancel();
        let control = ExecutionControl::new(token);
        assert_eq!(
            query.search(&control),
            Err(BoundaryRecoveryError::Cancelled)
        );
    }
}
