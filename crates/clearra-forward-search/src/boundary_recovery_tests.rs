use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;

use super::*;

fn control() -> ExecutionControl {
    ExecutionControl::new(ExecutionCancellationToken::new())
}

fn two_stage_query() -> BoundaryRecoveryQuery {
    BoundaryRecoveryQuery {
        initial_board: Board256Mask::from_words([0x3f0, 0, 0, 0]),
        stage_one_target: Board256Mask::EMPTY,
        final_board: Board256Mask::from_words([0xc030, 0, 0, 0]),
        height: 4,
        queue: vec![PieceKind::I, PieceKind::O],
        stage_one_queue_len: 1,
        required_placements: 2,
        placement_role_masks: Vec::new(),
        placement_role_pieces: Vec::new(),
        max_early_placements: 1,
        borrow_role_index: 1,
        borrow_placement_mask: Board256Mask::from_words([0x300c000, 0, 0, 0]),
        hold_enabled: false,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
        preserve_b2b_by_stage: [false, false],
        preserve_b2b_bag_mask: 0,
        initial_b2b: true,
        max_states: 10_000,
    }
}

#[test]
fn normal_checkpoint_reuses_the_same_board_and_supply() {
    let report = two_stage_query().search(&control()).unwrap();
    assert_eq!(report.status, BoundaryRecoveryStatus::Normal);
    assert_eq!(report.stage_one_checkpoint_step, Some(1));
    assert_eq!(report.checkpoint_is_pc, Some(true));
    assert_eq!(report.borrowed_stage_two_count, 0);
    assert_eq!(report.steps.len(), 2);
    assert_eq!(report.steps[0].source_queue_index, 0);
    assert_eq!(report.steps[1].source_queue_index, 1);
    assert_eq!(report.steps[1].board_after, [0xc030, 0, 0, 0]);
}

#[test]
fn borrowed_stage_two_token_is_not_consumed_twice() {
    let mut query = two_stage_query();
    query.queue.push(PieceKind::T);
    query.hold_enabled = true;
    let (result, states) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
    let PassResult::Found {
        steps,
        checkpoint_is_pc,
        borrowed_count,
        ..
    } = result
    else {
        panic!("the second-stage O should be borrowable before the I clear: {result:?}, states={states}");
    };
    assert!(!checkpoint_is_pc);
    assert_eq!(borrowed_count, 1);
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].source_queue_index, 1);
    assert_eq!(steps[0].hold_decision, "store");
    assert_eq!(steps[1].source_queue_index, 0);
    assert_eq!(steps[1].hold_decision, "swap");
    assert_eq!(steps[1].board_after, [0xc030, 0, 0, 0]);
}

#[test]
fn recovery_rejects_a_different_early_placement_for_the_same_piece() {
    let mut query = two_stage_query();
    query.queue.push(PieceKind::T);
    query.hold_enabled = true;
    query.borrow_placement_mask = Board256Mask::from_words([0x600c000, 0, 0, 0]);
    let (result, _) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
    assert!(matches!(result, PassResult::NoPath));
}

#[test]
fn exhausted_state_budget_is_not_reported_as_impossibility() {
    let mut query = two_stage_query();
    query.max_states = 1;
    let report = query.search(&control()).unwrap();
    assert_eq!(report.status, BoundaryRecoveryStatus::Incomplete);
}

#[test]
fn zero_early_placements_runs_only_the_normal_connection() {
    let mut query = two_stage_query();
    query.max_early_placements = 0;
    query.borrow_placement_mask = Board256Mask::EMPTY;
    let report = query.search(&control()).unwrap();
    assert_eq!(report.status, BoundaryRecoveryStatus::Normal);
    assert_eq!(report.recovery_states, 0);
}

#[test]
fn lookahead_token_cannot_replace_a_required_second_stage_token() {
    let mut query = two_stage_query();
    query.queue.push(PieceKind::T);
    query.hold_enabled = true;
    query.max_early_placements = 0;
    // After I clears the initial row, T could make this board, but the
    // declared second-stage token is O and T is only queue lookahead.
    query.final_board = Board256Mask::from_words([0x807, 0, 0, 0]);
    let report = query.search(&control()).unwrap();
    assert_eq!(report.status, BoundaryRecoveryStatus::NoPath);

    query.max_early_placements = 1;
    query.borrow_role_index = 2;
    assert_eq!(
        query.search(&control()),
        Err(BoundaryRecoveryError::InvalidBorrowRole)
    );
}

#[test]
fn exact_stage_roles_follow_source_tokens_through_the_same_search() {
    let mut query = two_stage_query();
    query.max_early_placements = 0;
    let ordinary = query.search(&control()).unwrap();
    query.placement_role_masks = ordinary
        .steps
        .iter()
        .map(|step| Board256Mask::from_words(step.placement_mask))
        .collect();
    assert_eq!(
        query.search(&control()).unwrap().status,
        BoundaryRecoveryStatus::Normal
    );

    query.placement_role_masks[1] = Board256Mask::from_words([0xf, 0, 0, 0]);
    assert_eq!(
        query.search(&control()).unwrap().status,
        BoundaryRecoveryStatus::NoPath
    );
    query.placement_role_masks.pop();
    assert_eq!(
        query.search(&control()),
        Err(BoundaryRecoveryError::InvalidPlacementRoles)
    );
}

#[test]
fn b2b_preservation_never_accepts_reestablishing_a_broken_chain() {
    let mut query = two_stage_query();
    query.initial_b2b = false;
    query.preserve_b2b_by_stage = [true, false];
    let report = query.search(&control()).unwrap();
    assert_eq!(report.status, BoundaryRecoveryStatus::NoPath);
}

#[test]
fn bag_policy_checks_the_locked_source_bag_independently() {
    let mut query = two_stage_query();
    query.initial_b2b = false;
    query.preserve_b2b_bag_mask = 1;
    assert_eq!(
        query.search(&control()).unwrap().status,
        BoundaryRecoveryStatus::NoPath
    );
    query.preserve_b2b_bag_mask = 2;
    assert_eq!(
        query.search(&control()).unwrap().status,
        BoundaryRecoveryStatus::Normal
    );
    query.preserve_b2b_bag_mask = 4;
    assert_eq!(
        query.search(&control()),
        Err(BoundaryRecoveryError::InvalidBagPolicy)
    );
}

#[test]
fn borrowed_token_cannot_break_an_active_selected_bag() {
    let mut query = two_stage_query();
    query.queue.push(PieceKind::T);
    query.hold_enabled = true;
    query.initial_b2b = false;
    let (unrestricted, _) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
    assert!(matches!(unrestricted, PassResult::Found { .. }));
    query.preserve_b2b_bag_mask = 1;
    let (protected, _) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
    assert!(matches!(protected, PassResult::NoPath));
}

#[test]
fn complete_and_partial_stages_keep_distinct_bag_indices() {
    let mut query = two_stage_query();
    query.stage_one_queue_len = 8;
    query.required_placements = 15;
    assert_eq!(query.bag_count(), 3);
    assert_eq!(query.bag_index(0), 0);
    assert_eq!(query.bag_index(7), 1);
    assert_eq!(query.bag_index(8), 2);
    assert_eq!(query.bag_index(14), 2);
}

#[test]
fn five_stage_one_bags_and_one_adjacent_bag_fit_without_bitmask_wraparound() {
    let mut query = two_stage_query();
    query.queue = (0..42)
        .map(|index| {
            [
                PieceKind::I,
                PieceKind::J,
                PieceKind::L,
                PieceKind::O,
                PieceKind::S,
                PieceKind::T,
                PieceKind::Z,
            ][index % 7]
        })
        .collect();
    query.stage_one_queue_len = 35;
    query.required_placements = 42;
    query.max_early_placements = 0;
    query.max_states = 1;
    assert_eq!(
        query.search(&control()).unwrap().status,
        BoundaryRecoveryStatus::Incomplete
    );

    query.queue.push(PieceKind::I);
    assert_eq!(
        query.search(&control()),
        Err(BoundaryRecoveryError::QueueTooLong)
    );
}

#[test]
fn bag_roles_remain_fixed_while_supply_tokens_permute() {
    let mut reference = two_stage_query();
    reference.height = 8;
    reference.initial_board = Board256Mask::EMPTY;
    reference.final_board = Board256Mask::EMPTY;
    reference.queue = vec![
        PieceKind::I,
        PieceKind::J,
        PieceKind::L,
        PieceKind::O,
        PieceKind::S,
        PieceKind::T,
        PieceKind::Z,
        PieceKind::Z,
        PieceKind::T,
        PieceKind::S,
        PieceKind::O,
        PieceKind::L,
        PieceKind::J,
        PieceKind::I,
    ];
    reference.stage_one_queue_len = 7;
    reference.required_placements = 14;
    reference.borrow_role_index = 10;
    reference.placement_role_masks = (0..14)
        .map(|index| Board256Mask::from_words([0xf_u64 << (index * 4), 0, 0, 0]))
        .collect();
    reference.borrow_placement_mask = reference.placement_role_masks[10];
    let plan = BoundaryRecoveryBagRolePlan::new(reference.clone()).unwrap();
    let mut sequence = reference.queue.clone();
    sequence[..7].reverse();
    sequence[7..].rotate_left(3);
    let projected = plan.query_for_sequence(&sequence).unwrap();
    assert_eq!(projected.queue, sequence);
    assert_eq!(projected.placement_role_pieces, reference.queue);
    assert_eq!(
        projected.placement_role_masks,
        reference.placement_role_masks
    );
    assert_eq!(projected.borrow_role_index, reference.borrow_role_index);
    assert_eq!(
        projected.borrow_placement_mask,
        reference.borrow_placement_mask
    );

    sequence[7] = PieceKind::I;
    assert!(plan.query_for_sequence(&sequence).is_none());
}

#[test]
fn identical_supply_pieces_can_fill_roles_across_the_stage_boundary() {
    let mut query = two_stage_query();
    query.initial_board = Board256Mask::from_words([0xff3fc, 0, 0, 0]);
    query.queue = vec![PieceKind::O, PieceKind::O];
    query.placement_role_masks = vec![
        Board256Mask::from_words([0xc03, 0, 0, 0]),
        Board256Mask::from_words([0xc03000000, 0, 0, 0]),
    ];
    query.placement_role_pieces = query.queue.clone();
    query.borrow_placement_mask = query.placement_role_masks[1];

    let (result, states) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
    let PassResult::Found {
        steps,
        checkpoint_is_pc,
        borrowed_count,
        ..
    } = result
    else {
        panic!("same-piece roles should cross the boundary: {result:?}, states={states}");
    };
    assert_eq!(steps.len(), 2);
    assert_eq!(borrowed_count, 1);
    assert!(!checkpoint_is_pc);
    assert_eq!(steps[0].source_queue_index, 0);
    assert_eq!(steps[0].placement_role_index, 1);
    assert_eq!(steps[1].source_queue_index, 1);
    assert_eq!(steps[1].placement_role_index, 0);
    assert_eq!(steps[1].board_after, query.final_board.words());
}

#[test]
fn exact_early_roles_can_prove_recovery_only_after_normal_failure() {
    let mut query = two_stage_query();
    query.queue.push(PieceKind::T);
    query.hold_enabled = true;
    let (PassResult::Found { steps, .. }, _) =
        Pass::new(&query, &control(), 1).unwrap().run().unwrap()
    else {
        panic!("expected an early-placement witness");
    };
    query.placement_role_masks = vec![Board256Mask::EMPTY; query.required_placements];
    for step in steps {
        query.placement_role_masks[step.source_queue_index] =
            Board256Mask::from_words(step.placement_mask);
    }
    query.borrow_placement_mask = query.placement_role_masks[query.borrow_role_index];
    let report = query.search(&control()).unwrap();
    assert_eq!(report.status, BoundaryRecoveryStatus::NonPcRecovery);
    assert!(report.normal_states > 0);
    assert!(report.recovery_states > 0);

    query.max_states = report.normal_states;
    let bounded = query.search(&control()).unwrap();
    assert_eq!(bounded.status, BoundaryRecoveryStatus::Incomplete);
    assert_eq!(bounded.recovery_states, 0);
    assert!(bounded.normal_states <= query.max_states);
}

#[test]
fn nonempty_first_stage_goal_is_checked_without_requiring_a_pc() {
    let mut query = two_stage_query();
    query.initial_board = Board256Mask::EMPTY;
    query.stage_one_target = Board256Mask::from_words([0xf, 0, 0, 0]);
    query.final_board = Board256Mask::from_words([0xc03f, 0, 0, 0]);
    query.max_early_placements = 0;
    query.placement_role_masks = vec![
        Board256Mask::from_words([0xf, 0, 0, 0]),
        Board256Mask::from_words([0xc030, 0, 0, 0]),
    ];
    let report = query.search(&control()).unwrap();
    assert_eq!(report.status, BoundaryRecoveryStatus::Normal);
    assert_eq!(report.stage_one_checkpoint_step, Some(1));
    assert_eq!(report.checkpoint_is_pc, Some(false));
    assert_eq!(report.steps[0].board_after, [0xf, 0, 0, 0]);
    query.stage_one_target = Board256Mask::from_words([0x1e, 0, 0, 0]);
    assert_eq!(
        query.search(&control()).unwrap().status,
        BoundaryRecoveryStatus::NoPath
    );
}

#[test]
fn first_stage_goal_uses_post_clear_coordinates_and_retains_initial_cells() {
    let mut query = two_stage_query();
    query.initial_board = Board256Mask::from_words([0x803f0, 0, 0, 0]);
    query.stage_one_target = Board256Mask::from_words([0x200, 0, 0, 0]);
    query.final_board = Board256Mask::from_words([0xe03, 0, 0, 0]);
    query.max_early_placements = 0;
    query.placement_role_masks = vec![
        Board256Mask::from_words([0xf, 0, 0, 0]),
        Board256Mask::from_words([0xc03, 0, 0, 0]),
    ];
    let report = query.search(&control()).unwrap();
    assert_eq!(report.status, BoundaryRecoveryStatus::Normal);
    assert_eq!(report.steps[0].board_after, [0x200, 0, 0, 0]);
    assert_eq!(report.checkpoint_is_pc, Some(false));
    query.stage_one_target = Board256Mask::from_words([0x80000, 0, 0, 0]);
    assert_eq!(
        query.search(&control()).unwrap().status,
        BoundaryRecoveryStatus::NoPath
    );
}

#[test]
fn early_second_stage_cells_cannot_impersonate_the_first_stage_target() {
    let mut query = two_stage_query();
    query.queue.push(PieceKind::T);
    query.hold_enabled = true;
    query.stage_one_target = query.final_board;
    let (result, _) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
    assert!(matches!(result, PassResult::NoPath));
}

#[test]
fn nonempty_first_stage_provenance_allows_a_real_early_second_stage_piece() {
    let mut query = two_stage_query();
    query.initial_board = Board256Mask::from_words([0x803f0, 0, 0, 0]);
    query.stage_one_target = Board256Mask::from_words([0x200, 0, 0, 0]);
    query.final_board = Board256Mask::from_words([0xc230, 0, 0, 0]);
    query.queue.push(PieceKind::T);
    query.hold_enabled = true;
    let (result, states) = Pass::new(&query, &control(), 1).unwrap().run().unwrap();
    let PassResult::Found {
        steps,
        checkpoint_is_pc,
        borrowed_count,
        ..
    } = result
    else {
        panic!("expected continuous early placement: {result:?}, states={states}");
    };
    assert_eq!(borrowed_count, 1);
    assert!(!checkpoint_is_pc);
    assert_eq!(steps[1].board_after, [0xc230, 0, 0, 0]);
}

#[test]
fn first_stage_target_cannot_extend_outside_the_declared_field() {
    let mut query = two_stage_query();
    query.stage_one_target = Board256Mask::from_words([1_u64 << 40, 0, 0, 0]);
    assert_eq!(
        query.search(&control()),
        Err(BoundaryRecoveryError::BoardOutsideField)
    );
}
