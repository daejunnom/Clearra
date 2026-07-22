use super::*;

fn state(hold_piece: Option<PieceKind>) -> HoldAutomatonState {
    HoldAutomatonState::new(
        PieceSourceId::new(11),
        3,
        hold_piece,
        2,
        0xfeed,
        SupplyProvenanceId(77),
    )
}

#[test]
fn hold_automaton_uses_current_piece() {
    let step = state(None)
        .apply(HoldTransition::UseCurrent, PieceKind::I, None)
        .expect("use current");

    assert_eq!(step.used_piece, PieceKind::I);
    assert_eq!(step.next_state.cursor, 4);
    assert!(step.next_state.hold_empty);
}

#[test]
fn hold_automaton_swaps_held_piece() {
    let step = state(Some(PieceKind::T))
        .apply(HoldTransition::SwapHeld, PieceKind::I, None)
        .expect("swap held");

    assert_eq!(step.used_piece, PieceKind::T);
    assert_eq!(step.next_state.cursor, 4);
    assert_eq!(step.next_state.hold_piece, Some(PieceKind::I));
}

#[test]
fn hold_automaton_stores_current_then_uses_next() {
    let step = state(None)
        .apply(
            HoldTransition::StoreCurrentThenUseNext,
            PieceKind::I,
            Some(PieceKind::O),
        )
        .expect("store current");

    assert_eq!(step.used_piece, PieceKind::O);
    assert_eq!(step.next_state.cursor, 5);
    assert_eq!(step.next_state.hold_piece, Some(PieceKind::I));
}

#[test]
fn hold_automaton_preserves_long_carryover() {
    let before = state(Some(PieceKind::L));
    let after = before
        .apply(HoldTransition::UseCurrent, PieceKind::S, Some(PieceKind::Z))
        .expect("use current")
        .next_state;

    assert_eq!(after.hold_piece, Some(PieceKind::L));
    assert_eq!(after.bag_epoch, before.bag_epoch);
    assert_eq!(after.bag_remainder_key, before.bag_remainder_key);
}

#[test]
fn hold_automaton_long_carryover_across_bag_epoch() {
    hold_automaton_preserves_long_carryover();
}

#[test]
fn hold_automaton_state_in_buildup_memo_key() {
    let memo = state(Some(PieceKind::J)).memo_key();

    assert_eq!(memo.piece_source_id, PieceSourceId::new(11));
    assert_eq!(memo.cursor, 3);
    assert_eq!(memo.hold_piece, Some(PieceKind::J));
    assert_eq!(memo.bag_epoch, 2);
    assert_eq!(memo.bag_remainder_key, 0xfeed);
    assert_eq!(memo.provenance, SupplyProvenanceId(77));
}
