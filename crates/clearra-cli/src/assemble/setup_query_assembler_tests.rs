use super::*;

#[test]
fn assembles_residue_and_cycle_boundary_policy() {
    let query = SetupQueryAssembler::assemble(&SetupArgs::new("I,T,O", true)).expect("setup query");

    assert_eq!(query.residue().remaining_count(), 3);
    assert_eq!(query.residue().cycle(), Some(7));
    assert_eq!(
        query.cycle_reset_borrow_policy(),
        SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
    );
}

#[test]
fn rejects_unknown_residue_piece() {
    assert_eq!(
        SetupQueryAssembler::assemble(&SetupArgs::new("IX", false)),
        Err(SetupQueryAssemblyError::UnknownPiece { value: 'X' })
    );
}
