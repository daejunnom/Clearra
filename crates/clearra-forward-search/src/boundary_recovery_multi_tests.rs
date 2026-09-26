//! Concrete multi-role witnesses, not tests of a validator against itself.
use super::*;

fn mask(value: u64) -> Board256Mask {
    Board256Mask::from_words([value, 0, 0, 0])
}

fn o_at(y: usize) -> Board256Mask {
    let mut words = [0_u64; 4];
    for index in [10 * y, 10 * y + 1, 10 * (y + 1), 10 * (y + 1) + 1] {
        words[index / 64] |= 1_u64 << (index % 64);
    }
    Board256Mask::from_words(words)
}

fn tower(early: usize) -> BoundaryRecoveryQuery {
    let top = o_at(2 * early);
    let mut roles = vec![top];
    roles.extend((0..early).map(|index| o_at(2 * index)));
    let mut all = [0_u64; 4];
    for role in &roles {
        for (word, value) in all.iter_mut().zip(role.words()) {
            *word |= value;
        }
    }
    BoundaryRecoveryQuery {
        initial_board: Board256Mask::EMPTY,
        stage_one_target: top,
        final_board: Board256Mask::from_words(all),
        height: (2 * (early + 1)) as u8,
        queue: vec![PieceKind::O; early + 1],
        stage_one_queue_len: 1,
        required_placements: Some(early + 1),
        placement_role_masks: roles,
        placement_role_pieces: Vec::new(),
        max_early_placements: early as u8,
        early_placement: EarlyPlacementPolicy::AnyStageTwoRole,
        hold_enabled: false,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::TSpins,
        preserve_b2b_by_stage: [false; 2],
        preserve_b2b_bag_mask: 0,
        initial_b2b: true,
        max_states: None,
    }
}

fn assert_witness(query: &BoundaryRecoveryQuery, report: &BoundaryRecoveryReport) {
    assert!(matches!(
        report.status,
        BoundaryRecoveryStatus::NonPcRecovery | BoundaryRecoveryStatus::PcPreservingRecovery
    ));
    let mut board = ForwardBoard::from_mask(query.initial_board);
    let mut source = 0_u64;
    let mut roles = 0_u64;
    let mut reach = ReachabilityWorkspace::new(query.height, query.rule_profile).unwrap();
    let mut early = 0;
    let first_mask = (1_u64 << query.stage_one_queue_len) - 1;
    for step in &report.steps {
        assert_eq!(
            source & (1_u64 << step.source_queue_index),
            0,
            "a source token may lock only once"
        );
        assert_eq!(
            roles & (1_u64 << step.placement_role_index),
            0,
            "a role may be fulfilled only once"
        );
        assert_eq!(query.queue[step.source_queue_index], step.piece);
        assert!(
            reach
                .reachable_locks(board, step.piece, true, true)
                .iter()
                .any(|lock| lock.mask.words() == step.placement_mask),
            "every returned lock must be reachable on the actual board"
        );
        if roles & first_mask != first_mask
            && step.placement_role_index >= query.stage_one_queue_len
        {
            early += 1;
        }
        source |= 1_u64 << step.source_queue_index;
        roles |= 1_u64 << step.placement_role_index;
        let (next, rows, lines) = place_and_clear(
            10,
            query.height,
            board.union_for_height(
                ForwardBoard::from_mask(Board256Mask::from_words(step.placement_mask)),
                query.height,
            ),
        );
        assert_eq!(next.words(), step.board_after);
        assert_eq!((rows, lines), (step.cleared_row_mask, step.cleared_lines));
        board = next;
    }
    assert_eq!(early, report.borrowed_stage_two_count);
    assert!(early <= usize::from(query.max_early_placements));
    assert_eq!(source.count_ones() as usize, report.steps.len());
    assert_eq!(roles.count_ones() as usize, report.steps.len());
    assert_eq!(board.words(), query.final_board.words());
}

#[test]
fn two_and_three_early_roles_are_real_execution_capabilities() {
    // Each O above the first requires all supports below. Identical source
    // pieces fill different stage roles without future-token fabrication.
    for required in [2, 3] {
        let mut query = tower(required);
        for cap in 0..required {
            query.max_early_placements = cap as u8;
            let failed = query.search(&ExecutionControl::default()).unwrap();
            assert_eq!(failed.status, BoundaryRecoveryStatus::NoPath);
        }
        query.max_early_placements = required as u8;
        for automatic in [false, true] {
            if automatic {
                query.required_placements = None;
            }
            let result = query.search(&ExecutionControl::default()).unwrap();
            assert_witness(&query, &result);
            assert_eq!(result.borrowed_stage_two_count, required);
            assert_eq!(result.stage_one_checkpoint_step, Some(required + 1));
            assert_eq!(result.steps.last().unwrap().placement_role_index, 0);
            assert_eq!(result.steps.first().unwrap().source_queue_index, 0);
            assert!(
                result.normal_states > 0,
                "normal failure must be proved first"
            );
        }
    }
}

#[test]
fn clear_events_do_not_refund_early_placement_count() {
    let mut query = tower(2);
    query.height = 6;
    query.initial_board = mask((0..4).fold(0, |board, row| board | (0x3fc_u64 << (10 * row))));
    query.stage_one_target = mask(0xf);
    query.final_board = mask(0xf);
    query.queue = vec![PieceKind::O, PieceKind::O, PieceKind::I];
    query.placement_role_pieces = vec![PieceKind::I, PieceKind::O, PieceKind::O];
    query.placement_role_masks = vec![mask(0xf), mask(0xc03), mask(0xc03)];
    let result = query.search(&ExecutionControl::default()).unwrap();
    assert_witness(&query, &result);
    assert_eq!(result.borrowed_stage_two_count, 2);
    assert_eq!(
        result.steps[..2]
            .iter()
            .map(|step| step.cleared_lines)
            .sum::<u8>(),
        4
    );
    assert_eq!(result.steps.last().unwrap().board_after, [0xf, 0, 0, 0]);
    query.max_early_placements = 1;
    assert_eq!(
        query.search(&ExecutionControl::default()).unwrap().status,
        BoundaryRecoveryStatus::NoPath
    );
}

#[test]
fn deferred_first_stage_clear_still_requires_actual_b2b_preservation() {
    let mut query = tower(2);
    query.height = 4;
    query.initial_board = mask(0x3f0 | (1 << 19));
    query.stage_one_target = mask(1 << 9);
    let left = (0x30_u64 << 10) | (0x30 << 20);
    let right = (0xc0_u64 << 10) | (0xc0 << 20);
    query.placement_role_masks = vec![mask(0xf), mask(left), mask(right)];
    query.placement_role_pieces = vec![PieceKind::I, PieceKind::O, PieceKind::O];
    query.queue = vec![PieceKind::O, PieceKind::O, PieceKind::I];
    query.final_board = mask((left | right) >> 10 | (1 << 9));
    let unrestricted = query.search(&ExecutionControl::default()).unwrap();
    assert_witness(&query, &unrestricted);
    let late = unrestricted.steps.last().unwrap();
    assert_eq!(late.placement_role_index, 0);
    assert_eq!(late.cleared_lines, 1);
    assert!(!late.recognized_spin);
    assert!(!late.b2b_active_after);
    // This is replay policy on the real interleaved board, not a condition on
    // an isolated stage-one candidate's old scoring evidence.
    query.preserve_b2b_by_stage = [true, true];
    assert_eq!(
        query.search(&ExecutionControl::default()).unwrap().status,
        BoundaryRecoveryStatus::NoPath
    );
}

#[test]
fn larger_cap_does_not_force_extra_early_locks_or_discard_normal_paths() {
    let mut query = tower(2);
    query.stage_one_target = mask(0xc03);
    query.placement_role_masks.swap(0, 1);
    let result = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.status, BoundaryRecoveryStatus::Normal);
    assert_eq!(result.borrowed_stage_two_count, 0);
    assert_eq!(result.recovery_states, 0);
}

#[test]
fn invalid_quotas_and_ambiguous_single_role_compatibility_fail_explicitly() {
    let mut query = tower(2);
    query.max_early_placements = 3;
    assert_eq!(
        query.search(&ExecutionControl::default()),
        Err(BoundaryRecoveryError::InvalidEarlyPlacementLimit)
    );
    query.max_early_placements = 2;
    query.early_placement = EarlyPlacementPolicy::SelectedRole {
        index: 1,
        placement: query.placement_role_masks[1],
    };
    assert_eq!(
        query.search(&ExecutionControl::default()),
        Err(BoundaryRecoveryError::InvalidBorrowRole)
    );
    query.early_placement = EarlyPlacementPolicy::AnyStageTwoRole;
    query.max_states = Some(1);
    assert_eq!(
        query.search(&ExecutionControl::default()).unwrap().status,
        BoundaryRecoveryStatus::Incomplete
    );
}

#[test]
fn one_slot_hold_can_defer_first_stage_while_two_other_tokens_lock_early() {
    let mut query = tower(2);
    query.stage_one_target = mask(0xf_u64 << 40);
    query.placement_role_masks[0] = query.stage_one_target;
    query.final_board = mask((0xf_u64 << 40) | 0xc03 | (0xc03_u64 << 20));
    query.queue = vec![PieceKind::I, PieceKind::O, PieceKind::O, PieceKind::T];
    query.hold_enabled = true;
    let result = query.search(&ExecutionControl::default()).unwrap();
    assert_witness(&query, &result);
    assert_eq!(
        result
            .steps
            .iter()
            .map(|step| step.source_queue_index)
            .collect::<Vec<_>>(),
        vec![1, 2, 0]
    );
    assert_eq!(
        result
            .steps
            .iter()
            .map(|step| step.hold_decision)
            .collect::<Vec<_>>(),
        vec!["store", "none", "swap"]
    );
    assert_eq!(result.borrowed_stage_two_count, 2);
    assert_eq!(result.steps.last().unwrap().piece, PieceKind::I);
    query.hold_enabled = false;
    assert_eq!(
        query.search(&ExecutionControl::default()).unwrap().status,
        BoundaryRecoveryStatus::NoPath
    );
}

#[test]
fn early_supports_can_turn_an_isolated_b2b_break_into_a_real_tetris_save() {
    let mut query = tower(2);
    let i_vertical = (0..4).fold(0_u64, |value, row| value | (1_u64 << (10 * row)));
    let lower_o = 0xc03_u64 << 11;
    let upper_o = 0xc03_u64 << 31;
    query.height = 6;
    query.initial_board =
        mask(0x3fe | (1..4).fold(0, |value, row| value | (0x3f8_u64 << (10 * row))));
    query.stage_one_target = Board256Mask::EMPTY;
    query.final_board = mask(6);
    query.queue = vec![PieceKind::I, PieceKind::O, PieceKind::O, PieceKind::T];
    query.placement_role_masks = vec![mask(i_vertical), mask(lower_o), mask(upper_o)];
    query.hold_enabled = true;
    query.preserve_b2b_by_stage = [true, true];
    // An isolated first-stage I clears just one row and leaves garbage behind.
    // That old evidence must not prune the geometry of the combined replay.
    let (isolated, _, lines) = place_and_clear(
        10,
        query.height,
        ForwardBoard::from_mask(query.initial_board)
            .union_for_height(ForwardBoard::from_mask(mask(i_vertical)), query.height),
    );
    assert_eq!(lines, 1);
    assert!(!isolated.is_empty());
    let result = query.search(&ExecutionControl::default()).unwrap();
    assert_witness(&query, &result);
    assert_eq!(result.borrowed_stage_two_count, 2);
    assert!(result.steps.iter().all(|step| step.b2b_active_after));
    let late = result.steps.last().unwrap();
    assert_eq!(late.placement_role_index, 0);
    assert_eq!(late.cleared_lines, 4);
    assert!(late.b2b_active_after);
}
