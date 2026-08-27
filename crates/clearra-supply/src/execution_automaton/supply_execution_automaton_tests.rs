use std::collections::HashSet;

use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    hold::hold_policy::HoldPolicy,
    piece_source::{PieceSourceId, PieceSourceKind},
    QueueObservationPolicy,
};

use super::*;

fn state(hold_piece: Option<PieceKind>) -> SupplyExecutionState {
    SupplyExecutionState::with_contract(
        PieceSourceId::new(0x11),
        PieceSourceKind::FixedQueue,
        3,
        hold_piece,
        HoldPolicy::Allowed,
        2,
        0x765_4321,
        SupplyObservationIdentity::new(QueueObservationPolicy::VisibleSeven, 0x22),
        SupplyProvenanceId(0x33),
    )
}

#[test]
fn hold_state_distinguishes_disabled_empty_and_occupied() {
    let mut disabled = state(None);
    disabled.hold_policy = HoldPolicy::Forbidden;
    assert_eq!(disabled.hold_state(), SupplyHoldState::Disabled);
    assert_eq!(state(None).hold_state(), SupplyHoldState::Empty);
    assert_eq!(
        state(Some(PieceKind::T)).hold_state(),
        SupplyHoldState::Occupied(PieceKind::T)
    );
}

#[test]
fn transitions_emit_typed_piece_cursor_hold_and_provenance_evidence() {
    let before = state(Some(PieceKind::T));
    let step = SupplyExecutionAutomaton::sequence()
        .transition(before, SupplyBranchKind::SwapHeld, PieceKind::I, None)
        .expect("legal swap");

    assert_eq!(step.used_piece, PieceKind::T);
    assert_eq!(step.next_state.cursor, 4);
    assert_eq!(step.next_state.hold_piece, Some(PieceKind::I));
    assert_eq!(step.evidence.used_piece, PieceKind::T);
    assert_eq!(step.evidence.queue_current_piece, PieceKind::I);
    assert_eq!(step.evidence.queue_next_piece, None);
    assert_eq!(step.evidence.queue_advances, 1);
    assert_eq!(step.evidence.cursor_before, 3);
    assert_eq!(step.evidence.cursor_after, 4);
    assert_eq!(
        step.evidence.hold_before,
        SupplyHoldState::Occupied(PieceKind::T)
    );
    assert_eq!(
        step.evidence.hold_after,
        SupplyHoldState::Occupied(PieceKind::I)
    );
    assert_eq!(step.evidence.branch_kind, SupplyBranchKind::SwapHeld);
    assert_eq!(step.evidence.source_kind, PieceSourceKind::FixedQueue);
    assert_eq!(step.evidence.observation, before.observation);
    assert_eq!(step.evidence.provenance, before.provenance);
}

#[test]
fn store_transition_records_two_queue_advances() {
    let step = SupplyExecutionAutomaton::sequence()
        .transition(
            state(None),
            SupplyBranchKind::StoreCurrent,
            PieceKind::I,
            Some(PieceKind::O),
        )
        .expect("legal store");

    assert_eq!(step.used_piece, PieceKind::O);
    assert_eq!(step.evidence.used_piece, PieceKind::O);
    assert_eq!(step.evidence.queue_current_piece, PieceKind::I);
    assert_eq!(step.evidence.queue_next_piece, Some(PieceKind::O));
    assert_eq!(step.evidence.queue_advances, 2);
    assert_eq!(step.evidence.hold_before, SupplyHoldState::Empty);
    assert_eq!(
        step.evidence.hold_after,
        SupplyHoldState::Occupied(PieceKind::I)
    );
}

#[test]
fn policy_rejects_forbidden_hold_and_required_current_branch() {
    let mut forbidden = state(None);
    forbidden.hold_policy = HoldPolicy::Forbidden;
    assert_eq!(
        SupplyExecutionAutomaton::sequence().transition(
            forbidden,
            SupplyBranchKind::StoreCurrent,
            PieceKind::I,
            Some(PieceKind::O),
        ),
        Err(SupplyExecutionError::HoldForbidden)
    );

    let mut required = state(None);
    required.hold_policy = HoldPolicy::Required;
    assert_eq!(
        SupplyExecutionAutomaton::sequence().transition(
            required,
            SupplyBranchKind::Current,
            PieceKind::I,
            None,
        ),
        Err(SupplyExecutionError::HoldRequired)
    );
}

#[test]
fn missing_next_held_and_occupied_hold_fail_closed() {
    let automaton = SupplyExecutionAutomaton::sequence();
    assert_eq!(
        automaton.transition(state(None), SupplyBranchKind::SwapHeld, PieceKind::I, None,),
        Err(SupplyExecutionError::MissingHeldPiece)
    );
    assert_eq!(
        automaton.transition(
            state(None),
            SupplyBranchKind::StoreCurrent,
            PieceKind::I,
            None,
        ),
        Err(SupplyExecutionError::MissingNextPiece)
    );
    assert_eq!(
        automaton.transition(
            state(Some(PieceKind::T)),
            SupplyBranchKind::StoreCurrent,
            PieceKind::I,
            Some(PieceKind::O),
        ),
        Err(SupplyExecutionError::HoldSlotOccupied)
    );
}

#[test]
fn cursor_advance_is_checked_instead_of_saturating() {
    let automaton = SupplyExecutionAutomaton::sequence();
    let mut current_overflow = state(None);
    current_overflow.cursor = u16::MAX;
    assert_eq!(
        automaton.transition(
            current_overflow,
            SupplyBranchKind::Current,
            PieceKind::I,
            None,
        ),
        Err(SupplyExecutionError::CursorExhausted)
    );

    let mut store_overflow = state(None);
    store_overflow.cursor = u16::MAX - 1;
    assert_eq!(
        automaton.transition(
            store_overflow,
            SupplyBranchKind::StoreCurrent,
            PieceKind::I,
            Some(PieceKind::O),
        ),
        Err(SupplyExecutionError::CursorExhausted)
    );
}

#[test]
fn bag_epoch_advance_is_checked() {
    let automaton =
        SupplyExecutionAutomaton::for_bag(&PieceKind::STANDARD_TETROMINOES).expect("standard bag");
    let mut exhausted = state(None);
    exhausted.source_kind = PieceSourceKind::BagUniverse;
    exhausted.cursor = 1;
    exhausted.bag_epoch = u16::MAX;
    exhausted.bag_remainder_key = 0;
    let mut branches = Vec::new();

    assert_eq!(
        automaton.write_matching_bag_steps(exhausted, PieceKind::I, &mut branches),
        Err(SupplyExecutionError::BagEpochExhausted)
    );
}

#[test]
fn memo_identity_separates_source_policy_observation_provenance_and_bag_state() {
    let base = state(Some(PieceKind::T));
    let mut variants = vec![base];

    let mut source_kind = base;
    source_kind.source_kind = PieceSourceKind::ObservedWindow;
    variants.push(source_kind);
    let mut policy = base;
    policy.hold_policy = HoldPolicy::Required;
    variants.push(policy);
    let mut observation_policy = base;
    observation_policy.observation.policy = QueueObservationPolicy::FullQueueOracle;
    variants.push(observation_policy);
    let mut observation_id = base;
    observation_id.observation.observation_id += 1;
    variants.push(observation_id);
    let mut provenance = base;
    provenance.provenance.0 += 1;
    variants.push(provenance);
    let mut bag_epoch = base;
    bag_epoch.bag_epoch += 1;
    variants.push(bag_epoch);
    let mut bag_remainder = base;
    bag_remainder.bag_remainder_key ^= 0x10;
    variants.push(bag_remainder);

    let keys = variants
        .into_iter()
        .map(SupplyExecutionState::memo_key)
        .collect::<HashSet<_>>();
    assert_eq!(keys.len(), 8);

    let memo = base.memo_key();
    assert_eq!(memo, base.memo_key());
    assert_eq!(memo.stable_hash(), base.memo_key().stable_hash());
}

#[test]
fn fixed_pattern_and_observed_memo_identities_do_not_collide() {
    let base = state(None);
    let mut pattern = base;
    pattern.source_kind = PieceSourceKind::MaterializedPatternUniverse;
    let mut observed = base;
    observed.source_kind = PieceSourceKind::ObservedWindow;

    let keys = [base.memo_key(), pattern.memo_key(), observed.memo_key()]
        .into_iter()
        .collect::<HashSet<_>>();
    assert_eq!(keys.len(), 3);
    assert_ne!(
        base.memo_key().stable_hash(),
        pattern.memo_key().stable_hash()
    );
    assert_ne!(
        base.memo_key().stable_hash(),
        observed.memo_key().stable_hash()
    );
}
