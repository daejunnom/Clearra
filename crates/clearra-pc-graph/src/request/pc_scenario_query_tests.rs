use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::*;
use crate::request::RequestedSearchBackend;

#[test]
fn scenario_query_owns_setup_completion_contract_without_pc_target() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0b1111),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::O])),
        PieceWindow::new(2),
    )
    .with_hold_piece(Some(PieceKind::T))
    .with_count_policy(PcCountPolicy::CountUnique);

    assert_eq!(query.initial_board().width(), 10);
    assert_eq!(query.initial_board().visible_height(), 4);
    assert_eq!(query.initial_board().occupied_mask(), 0b1111);
    assert_eq!(query.remaining_queue().mode(), "fixed");
    assert_eq!(query.hold_state().piece(), Some(PieceKind::T));
    assert_eq!(query.piece_window().max_pieces(), 2);
    assert_eq!(query.exact_pieces(), None);
    assert_eq!(query.min_remaining_queue(), 0);
    assert!(query.allow_hold());
    assert!(!query.requires_180());
    assert_eq!(query.completion_goal(), PcCompletionGoal::ClearToEmpty);
    assert_eq!(query.completion_goal().as_str(), "clear-to-empty");
    assert_eq!(query.count_policy(), PcCountPolicy::CountUnique);
    assert!(query.verified_kick_profile().is_none());
    assert_eq!(
        query.execution_policy().requested_backend(),
        RequestedSearchBackend::Auto
    );
    assert!(query.execution_policy().deterministic());
    assert_eq!(
        query.retained_trace_limit(),
        SearchDefaults::MVP1.scenario_retained_trace_limit()
    );
}

#[test]
fn scenario_query_owns_completion_constraints() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
        ])),
        PieceWindow::new(4),
    )
    .with_exact_pieces(Some(3))
    .with_min_remaining_queue(1)
    .with_allow_hold(false)
    .with_requires_180(true)
    .with_retained_trace_limit(2);

    assert_eq!(query.exact_pieces(), Some(3));
    assert_eq!(query.min_remaining_queue(), 1);
    assert!(!query.allow_hold());
    assert!(query.requires_180());
    assert_eq!(query.retained_trace_limit(), 2);
}

#[test]
fn scenario_query_can_carry_verified_imported_kick_profile_override() {
    let verified =
        VerifiedKickTableProfile::try_new(clearra_rules::kicks::SrsKicks::srs_plus_profile())
            .expect("verified profile");
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_verified_kick_table_profile(verified.clone());

    assert_eq!(query.rule().id(), verified.profile().source_rule());
    assert_eq!(query.verified_kick_profile(), Some(&verified));
}

#[test]
fn scenario_query_can_carry_execution_policy() {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_workers(2)
        .with_max_frontier_states(128);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_execution_policy(policy.clone());

    assert_eq!(query.execution_policy(), &policy);
}
