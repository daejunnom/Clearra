use super::*;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_supply::{
    execution_automaton::SupplyObservationIdentity,
    hold::hold_policy::HoldPolicy,
    hold_automaton::{HoldAutomatonState, SupplyProvenanceId},
    piece_source::{PieceSourceId, PieceSourceKind},
    QueueObservationPolicy,
};

fn hold_state() -> HoldAutomatonState {
    HoldAutomatonState::new(
        PieceSourceId::new(11),
        3,
        Some(PieceKind::T),
        2,
        0xfeed,
        SupplyProvenanceId(77),
    )
}

fn memo_key(hold: HoldAutomatonState) -> BuildUpMemoKey {
    BuildUpMemoKey::new(
        CacheIdentity(0x1234),
        0b111,
        0x30,
        DeletedLineState::default(),
        hold,
        0x8080,
        0,
    )
}

#[test]
fn buildup_memo_key_differs_by_deleted_line_state() {
    let left = memo_key(hold_state());
    let mut right = left;
    right.deleted_line_state = DeletedLineState {
        deleted_row_mask: 1 << 3,
        deleted_count: 1,
    };

    assert_ne!(left.stable_hash(), right.stable_hash());
}

#[test]
fn buildup_memo_key_includes_deleted_line_state() {
    buildup_memo_key_differs_by_deleted_line_state();
}

#[test]
fn buildup_memo_key_includes_hold_automaton_state() {
    let left = memo_key(hold_state());
    let mut changed = hold_state();
    changed.hold_piece = Some(PieceKind::I);
    changed.hold_empty = false;
    let right = memo_key(changed);

    assert_ne!(left.hold_automaton_state, right.hold_automaton_state);
    assert_ne!(left.stable_hash(), right.stable_hash());
}

#[test]
fn buildup_memo_key_differs_by_bag_epoch() {
    let left = memo_key(hold_state());
    let mut changed = hold_state();
    changed.bag_epoch += 1;
    let right = memo_key(changed);

    assert_ne!(left.stable_hash(), right.stable_hash());
}

#[test]
fn buildup_memo_key_differs_by_bag_remainder_key() {
    let left = memo_key(hold_state());
    let mut changed = hold_state();
    changed.bag_remainder_key ^= 0x40;
    let right = memo_key(changed);

    assert_ne!(left.stable_hash(), right.stable_hash());
}

#[test]
fn buildup_memo_key_differs_by_reachability_state() {
    let left = memo_key(hold_state());
    let mut right = left;
    right.reachability_relevant_state ^= 0x8000;

    assert_ne!(left.stable_hash(), right.stable_hash());
}

#[test]
fn buildup_memo_key_includes_source_kind_hold_policy_observation_and_provenance() {
    let base = hold_state();
    let base_key = memo_key(base);

    let mut source_kind = base;
    source_kind.source_kind = PieceSourceKind::ObservedWindow;
    let mut hold_policy = base;
    hold_policy.hold_policy = HoldPolicy::Required;
    let mut observation = base;
    observation.observation = SupplyObservationIdentity::new(
        QueueObservationPolicy::VisibleSeven,
        base.observation.observation_id + 1,
    );
    let mut provenance = base;
    provenance.provenance.0 += 1;

    for changed in [source_kind, hold_policy, observation, provenance] {
        let changed_key = memo_key(changed);
        assert_ne!(
            base_key.hold_automaton_state,
            changed_key.hold_automaton_state
        );
        assert_ne!(base_key.stable_hash(), changed_key.stable_hash());
    }
}
