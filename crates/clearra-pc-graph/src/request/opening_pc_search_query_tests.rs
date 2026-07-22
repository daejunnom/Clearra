use crate::request::{RequestedSearchBackend, SupplyWindowSize};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_profiles::{
    bag::bag_profile::BagProfileId, board::board_profile::BoardProfileId,
    pieces::piece_set_profile::PieceSetProfileId,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::*;

#[test]
fn opening_query_owns_empty_field_pc_search_contract() {
    let query = OpeningPcSearchQuery::standard_mvp(PcTarget::four_lines());

    assert_eq!(query.target(), PcTarget::four_lines());
    assert_eq!(query.board().id(), BoardProfileId::Standard10);
    assert_eq!(
        query.piece_set().id(),
        PieceSetProfileId::StandardTetrominoes
    );
    assert_eq!(query.bag().id(), BagProfileId::Standard7Bag);
    assert_eq!(query.queue().mode(), "observed");
    assert!(query.hold_policy().is_enabled());
    assert_eq!(query.supply_window_size(), None);
    assert!(query.rule().is_two_line_supported());
    assert!(query.verified_kick_profile().is_none());
    assert_eq!(
        query.execution_policy().requested_backend(),
        RequestedSearchBackend::Auto
    );
    assert!(query.execution_policy().deterministic());
    assert_eq!(
        query.objective().kind(),
        clearra_core_domain::objective::objective_kind::ObjectiveKind::All
    );
}

#[test]
fn opening_query_preserves_observed_supply_window_size() {
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_supply_window_size(SupplyWindowSize::new(10));

    assert_eq!(query.supply_window_size(), Some(SupplyWindowSize::new(10)));
}

#[test]
fn opening_query_can_carry_cli_supplied_queue_hold_and_objective() {
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
        ])))
        .with_hold_policy(PcHoldPolicy::Disabled)
        .with_objective(ObjectivePolicy::unique());

    assert_eq!(query.queue().mode(), "fixed");
    assert_eq!(query.queue().len(), 1);
    assert!(!query.hold_policy().is_enabled());
    assert_eq!(
        query.objective().kind(),
        clearra_core_domain::objective::objective_kind::ObjectiveKind::Unique
    );
}

#[test]
fn opening_query_can_carry_execution_policy_without_cli_search_logic() {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_workers(4)
        .with_deterministic(true)
        .with_allow_backend_fallback(false);
    let query =
        OpeningPcSearchQuery::new(PcTarget::two_lines()).with_execution_policy(policy.clone());

    assert_eq!(query.execution_policy(), &policy);
}

#[test]
fn opening_query_can_carry_verified_imported_kick_profile_override() {
    let verified =
        VerifiedKickTableProfile::try_new(clearra_rules::kicks::SrsKicks::srs_plus_profile())
            .expect("verified profile");
    let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
        .with_verified_kick_table_profile(verified.clone());

    assert_eq!(query.rule().id(), verified.profile().source_rule());
    assert_eq!(query.verified_kick_profile(), Some(&verified));
}
