use super::*;
use clearra_core_domain::execution_cancellation::ExecutionCancellationToken;

fn mask(value: u64) -> Board256Mask {
    Board256Mask::from_words([value, 0, 0, 0])
}
fn role(piece: PieceKind, value: u64) -> CrossStageRole {
    CrossStageRole {
        piece,
        lock_mask: mask(value),
    }
}
fn square(x: usize, y: usize) -> u64 {
    (3_u64 << (10 * y + x)) | (3_u64 << (10 * (y + 1) + x))
}

// The first-stage roles are I placements, while the first-stage supply is O.
// Every supplied O therefore has to fill a second-stage base before later I
// tokens can complete stage one. These are fixed-role fixtures, not a claim
// that standalone Build Probability generates these pairs.
fn towers(xs: &[usize]) -> CrossStageRecoveryQuery {
    let first: Vec<_> = xs
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let y = if xs.len() == 3 && i == 1 { 3 } else { 2 };
            role(PieceKind::I, 0xf << (10 * y + *x))
        })
        .collect();
    let second: Vec<_> = xs
        .iter()
        .map(|x| role(PieceKind::O, square(*x, 0)))
        .collect();
    let first_mask = first
        .iter()
        .fold(0, |sum, role| sum | role.lock_mask.words()[0]);
    let final_mask = xs.iter().fold(first_mask, |sum, x| sum | square(*x, 0));
    CrossStageRecoveryQuery {
        initial_board: Board256Mask::EMPTY,
        stage_one_target: mask(first_mask),
        final_board: mask(final_mask),
        height: 4,
        stage_one_supply: vec![PieceKind::O; xs.len()],
        stage_two_supply: vec![PieceKind::I; xs.len()],
        stage_one_roles: first,
        stage_two_roles: second,
        early_limit: CrossStageEarlyLimit::Auto,
        hold_enabled: false,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
        initial_b2b: true,
        preserve_b2b: false,
    }
}

fn verify_trace(query: &CrossStageRecoveryQuery, result: &CrossStagePairReport) {
    assert_eq!(result.steps.len(), query.role_count().unwrap());
    let mut source_indices = HashSet::new();
    let mut role_indices = HashSet::new();
    let mut early = 0;
    let mut checkpoint = false;
    let mut board = ForwardBoard::from_mask(query.initial_board);
    let mut reachability = ReachabilityWorkspace::new(query.height, query.rule_profile).unwrap();
    for step in &result.steps {
        assert!(
            reachability
                .reachable_locks(board, step.piece, true, true)
                .iter()
                .any(|lock| lock.mask.words() == step.placement_mask),
            "witness lock is reachable"
        );
        let (next, rows, lines) = place_and_clear(
            10,
            query.height,
            board.union_for_height(
                ForwardBoard::from_mask(Board256Mask::from_words(step.placement_mask)),
                query.height,
            ),
        );
        assert_eq!(
            (next.words(), rows, lines),
            (step.board_after, step.cleared_row_mask, step.cleared_lines)
        );
        board = next;
        assert!(source_indices.insert(step.source_queue_index));
        assert!(role_indices.insert(step.placement_role_index));
        assert_eq!(
            query.token(step.source_queue_index).unwrap().piece,
            step.piece
        );
        if !checkpoint
            && step.source_queue_index < query.stage_one_supply.len()
            && step.placement_role_index >= query.stage_one_roles.len()
        {
            early += 1;
        }
        checkpoint = step.stage_one_complete_after;
    }
    assert_eq!(early, result.actual_early_placements);
    assert!(early <= result.effective_max_early);
    assert_eq!(
        result.steps.last().unwrap().board_after,
        query.final_board.words()
    );
}

#[test]
fn two_and_three_early_roles_require_their_actual_count_and_auto_finds_them() {
    for xs in [vec![0, 4], vec![0, 3, 6]] {
        let mut query = towers(&xs);
        for limit in 0..xs.len() {
            query.early_limit = CrossStageEarlyLimit::AtMost(limit);
            let result = query.search(&ExecutionControl::default()).unwrap();
            assert_eq!(result.status, CrossStagePairStatus::NoPath);
            assert!(result.pair_exhausted);
        }
        for limit in [
            CrossStageEarlyLimit::AtMost(xs.len()),
            CrossStageEarlyLimit::AtMost(xs.len() + 5),
            CrossStageEarlyLimit::Auto,
        ] {
            query.early_limit = limit;
            let result = query.search(&ExecutionControl::default()).unwrap();
            assert_eq!(result.status, CrossStagePairStatus::Recovery);
            assert_eq!(result.actual_early_placements, xs.len());
            assert!(
                !result.pair_exhausted,
                "one witness is not all-path enumeration"
            );
            verify_trace(&query, &result);
        }
    }
}

#[test]
fn late_stage_one_clear_does_not_escape_global_b2b_policy() {
    // The stage-one source O fills a second-stage role. The stage-two source
    // I then completes stage one with a non-PC single under the existing O.
    let mut query = towers(&[0]);
    query.initial_board = mask(0x3f0);
    query.stage_one_target = Board256Mask::EMPTY;
    query.final_board = mask(0xc030);
    query.stage_one_supply = vec![PieceKind::O];
    query.stage_two_supply = vec![PieceKind::I];
    query.stage_one_roles = vec![role(PieceKind::I, 0xf)];
    query.stage_two_roles = vec![role(PieceKind::O, 0x300c000)];
    let result = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.status, CrossStagePairStatus::Recovery);
    verify_trace(&query, &result);
    assert_eq!(result.steps.last().unwrap().cleared_lines, 1);
    assert!(!result.steps.last().unwrap().b2b_active_after);
    query.preserve_b2b = true;
    assert_eq!(
        query.search(&ExecutionControl::default()).unwrap().status,
        CrossStagePairStatus::NoPath
    );
}

#[test]
fn actual_global_b2b_keeps_zero_clear_crossings_legal() {
    let mut query = towers(&[0, 4]);
    query.preserve_b2b = true;
    let result = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.status, CrossStagePairStatus::Recovery);
    assert!(result
        .steps
        .iter()
        .all(|step| step.cleared_lines == 0 && step.b2b_active_after));
    verify_trace(&query, &result);
}

#[test]
fn finite_supply_releases_the_last_held_token_without_reusing_it() {
    let mut query = towers(&[0]);
    // Normal exact path needs O before I, but the source order is I O.
    query.stage_one_supply = vec![PieceKind::I];
    query.stage_two_supply = vec![PieceKind::O];
    query.stage_one_roles = vec![role(PieceKind::O, square(0, 0))];
    query.stage_two_roles = vec![role(PieceKind::I, 0xf << 20)];
    query.stage_one_target = mask(square(0, 0));
    query.final_board = mask(square(0, 0) | (0xf << 20));
    query.hold_enabled = true;
    let result = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.status, CrossStagePairStatus::Normal);
    assert_eq!(result.steps[0].hold_decision, "store");
    assert_eq!(result.steps[1].hold_decision, "release-held-at-terminal");
    verify_trace(&query, &result);
}

#[test]
fn no_fixed_42_token_or_single_word_role_limit_is_introduced() {
    let mut query = towers(&[0]);
    query.stage_one_supply.extend(vec![PieceKind::O; 80]);
    query
        .stage_two_roles
        .extend(vec![role(PieceKind::O, square(4, 0)); 80]);
    assert_eq!(
        query
            .early_limit
            .effective_max(query.stage_one_supply.len(), query.stage_two_roles.len()),
        81
    );
    // Cancellation prevents the deliberately impossible large search while
    // verifying that its dimensions are not rejected as a 42/64-item limit.
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    assert_eq!(
        query.search(&ExecutionControl::new(token)),
        Err(CrossStageSearchError::Cancelled)
    );
    let mut roles = vec![0_u64; 3];
    for index in [0, 63, 64, 127, 128] {
        roles[index / 64] |= 1 << (index % 64);
    }
    assert!([0, 63, 64, 127, 128]
        .into_iter()
        .all(|index| bit(&roles, index)));
    assert!([1, 62, 65, 126, 129]
        .into_iter()
        .all(|index| !bit(&roles, index)));
}

#[test]
fn invalid_role_is_not_downgraded_to_no_path() {
    let mut query = towers(&[0]);
    query.stage_two_roles[0].lock_mask = mask(1);
    assert_eq!(
        query.search(&ExecutionControl::default()),
        Err(CrossStageSearchError::InvalidRole)
    );
}

#[test]
fn global_preservation_does_not_reject_zero_clear_moves_before_chain_start() {
    let mut query = towers(&[0, 4]);
    query.preserve_b2b = true;
    query.initial_b2b = false;
    let result = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.status, CrossStagePairStatus::Recovery);
    assert!(result
        .steps
        .iter()
        .all(|step| step.cleared_lines == 0 && !step.b2b_active_after));
    verify_trace(&query, &result);
}

#[test]
fn first_supply_supports_make_late_first_role_a_real_b2b_save() {
    let mut query = towers(&[0]);
    let vertical_i = (0..4).fold(0_u64, |value, row| value | (1_u64 << (10 * row)));
    query.height = 6;
    query.initial_board =
        mask(0x3fe | (1..4).fold(0, |value, row| value | (0x3f8_u64 << (10 * row))));
    query.stage_one_target = Board256Mask::EMPTY;
    query.final_board = mask(6);
    query.stage_one_supply = vec![PieceKind::O, PieceKind::O];
    query.stage_two_supply = vec![PieceKind::I];
    query.stage_one_roles = vec![role(PieceKind::I, vertical_i)];
    query.stage_two_roles = vec![
        role(PieceKind::O, 0xc03 << 11),
        role(PieceKind::O, 0xc03 << 31),
    ];
    query.preserve_b2b = true;
    let (_, _, isolated_lines) = place_and_clear(
        10,
        query.height,
        ForwardBoard::from_mask(query.initial_board)
            .union_for_height(ForwardBoard::from_mask(mask(vertical_i)), query.height),
    );
    assert_eq!(
        isolated_lines, 1,
        "isolated evidence is NOT combined execution evidence"
    );
    let result = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.status, CrossStagePairStatus::Recovery);
    assert_eq!(result.actual_early_placements, 2);
    assert_eq!(result.steps.last().unwrap().cleared_lines, 4);
    assert!(result.steps.iter().all(|step| step.b2b_active_after));
    verify_trace(&query, &result);
}
