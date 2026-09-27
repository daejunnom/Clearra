use super::*;
use crate::board::{place_and_clear, ForwardBoard};
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

fn query() -> CrossStageCatalogQuery {
    // Same final board, two exact lock-time realizations for O. The elevated
    // O must be locked before I clears its floor. The low O permits normal I→O.
    CrossStageCatalogQuery {
        execution: CrossStageExecution {
            initial_board: mask(0x3f0),
            stage_one_target: Board256Mask::EMPTY,
            final_board: mask(0xc030),
            height: 4,
            stage_one_supply: vec![PieceKind::O],
            stage_two_supply: vec![PieceKind::I, PieceKind::O],
            early_limit: CrossStageEarlyLimit::Auto,
            hold_enabled: true,
            rule_profile: RuleProfileId::SrsPlus,
            spin_profile: SpinProfileId::AllSpinPlus,
            initial_b2b: true,
            preserve_b2b: false,
        },
        stage_one: CrossStageCatalog::new(
            vec![vec![role(PieceKind::I, 0xf)]],
            CrossStageCatalogCompletion::Exhausted,
        ),
        stage_two: CrossStageCatalog::new(
            vec![
                vec![role(PieceKind::O, 0x300c000)],
                vec![role(PieceKind::O, 0xc030)],
            ],
            CrossStageCatalogCompletion::Exhausted,
        ),
    }
}

fn assert_witness(query: &CrossStageCatalogQuery, report: &CrossStageCatalogReport) {
    let witness = report.witness.as_ref().expect("existence has a witness");
    let roles: Vec<_> = query.stage_one.candidates[witness.stage_one_candidate]
        .iter()
        .chain(&query.stage_two.candidates[witness.stage_two_candidate])
        .collect();
    let supply: Vec<_> = query
        .execution
        .stage_one_supply
        .iter()
        .chain(&query.execution.stage_two_supply)
        .collect();
    let mut board = ForwardBoard::from_mask(query.execution.initial_board);
    let mut reach =
        ReachabilityWorkspace::new(query.execution.height, query.execution.rule_profile).unwrap();
    let mut seen_sources = std::collections::HashSet::new();
    let mut seen_roles = std::collections::HashSet::new();
    for step in &witness.path.steps {
        assert!(seen_sources.insert(step.source_queue_index));
        assert!(seen_roles.insert(step.placement_role_index));
        assert_eq!(*supply[step.source_queue_index], step.piece);
        assert_eq!(roles[step.placement_role_index].piece, step.piece);
        assert_eq!(
            roles[step.placement_role_index].lock_mask.words(),
            step.placement_mask
        );
        assert!(reach
            .reachable_locks(board, step.piece, true, true)
            .iter()
            .any(|lock| lock.mask.words() == step.placement_mask));
        let (next, rows, lines) = place_and_clear(
            10,
            query.execution.height,
            board.union_for_height(
                ForwardBoard::from_mask(Board256Mask::from_words(step.placement_mask)),
                query.execution.height,
            ),
        );
        assert_eq!(
            (next.words(), rows, lines),
            (step.board_after, step.cleared_row_mask, step.cleared_lines)
        );
        board = next;
    }
    assert_eq!(seen_roles.len(), roles.len());
    assert_eq!(board.words(), query.execution.final_board.words());
    assert!(
        !report.declared_scope_exhausted,
        "one witness is not all-path enumeration"
    );
}

#[test]
fn later_normal_candidate_dominates_an_earlier_actual_recovery() {
    let query = query();
    let first = query.execution.pair(
        &query.stage_one.candidates[0],
        &query.stage_two.candidates[0],
    );
    assert_eq!(
        first.search(&ExecutionControl::default()).unwrap().status,
        CrossStagePairStatus::Recovery
    );
    let report = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(report.status, CrossStageCatalogStatus::Normal);
    assert_eq!(report.normal_pairs_checked, 2);
    assert_eq!(report.recovery_pairs_checked, 0);
    assert_eq!(report.witness.as_ref().unwrap().stage_two_candidate, 1);
    assert!(!report.normal_exclusion_proven);
    assert_witness(&query, &report);
}

#[test]
fn incomplete_catalogs_cannot_prove_additional_recovery_or_no_path() {
    let mut query = query();
    query.stage_two.candidates.truncate(1);
    query.stage_two.completion = CrossStageCatalogCompletion::Partial;
    let report = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(
        report.status,
        CrossStageCatalogStatus::RecoveryWithoutNormalExclusion
    );
    assert!(!report.normal_exclusion_proven);
    assert!(!report.catalogs_exhausted);
    assert_witness(&query, &report);
    query.execution.early_limit = CrossStageEarlyLimit::AtMost(0);
    let report = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(report.status, CrossStageCatalogStatus::IncompleteCatalogs);
    assert!(report.witness.is_none());
}

#[test]
fn exhaustive_declared_catalogs_classify_recovery_without_multiplying_probabilities() {
    let mut query = query();
    query.stage_two.candidates.truncate(1);
    let report = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(report.status, CrossStageCatalogStatus::AdditionalRecovery);
    assert!(report.normal_exclusion_proven);
    assert_eq!(
        report
            .witness
            .as_ref()
            .unwrap()
            .path
            .actual_early_placements,
        1
    );
    assert_witness(&query, &report);
    query.execution.early_limit = CrossStageEarlyLimit::AtMost(0);
    let report = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(report.status, CrossStageCatalogStatus::NoPathWithinCatalogs);
    assert!(report.normal_exclusion_proven && report.declared_scope_exhausted);
}

#[test]
fn global_b2b_checks_the_deferred_first_stage_lock_not_just_the_second_stage() {
    let mut query = query();
    query.stage_two.candidates.truncate(1);
    query.execution.preserve_b2b = true;
    let report = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(report.status, CrossStageCatalogStatus::NoPathWithinCatalogs);
    assert!(report.declared_scope_exhausted);
}

#[test]
fn normal_existence_remains_proven_with_a_partial_catalog() {
    let mut query = query();
    query.stage_one.completion = CrossStageCatalogCompletion::Partial;
    query.stage_two.completion = CrossStageCatalogCompletion::Partial;
    let report = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(report.status, CrossStageCatalogStatus::Normal);
    assert!(!report.catalogs_exhausted);
    assert_witness(&query, &report);
}

#[test]
fn cancellation_and_invalid_inputs_do_not_turn_into_negative_search_proofs() {
    let mut query = query();
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    assert_eq!(
        query.search(&ExecutionControl::new(token)),
        Err(CrossStageCatalogError::Execution(
            CrossStageSearchError::Cancelled
        ))
    );
    query.stage_two.candidates[1][0].lock_mask = mask(1);
    assert_eq!(
        query.search(&ExecutionControl::default()),
        Err(CrossStageCatalogError::InvalidCandidate { stage: 2, index: 1 })
    );
    query.stage_two.candidates.clear();
    let complete = query.search(&ExecutionControl::default()).unwrap();
    assert_eq!(
        complete.status,
        CrossStageCatalogStatus::NoPathWithinCatalogs
    );
    assert_eq!(complete.normal_pairs_checked, 0);
    query.stage_two.completion = CrossStageCatalogCompletion::Partial;
    assert_eq!(
        query.search(&ExecutionControl::default()).unwrap().status,
        CrossStageCatalogStatus::IncompleteCatalogs
    );
}
