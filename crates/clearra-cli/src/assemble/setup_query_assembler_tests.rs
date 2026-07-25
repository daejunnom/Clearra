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

#[test]
fn preserves_setup_candidate_priority() {
    let args = SetupArgs::new("IOTS", false).with_candidate_priority(
        clearra_setup_search::query::SetupCandidatePriority::PcProbabilityFirst,
    );
    let query = SetupQueryAssembler::assemble(&args).expect("setup query");

    assert_eq!(
        query.candidate_priority(),
        clearra_setup_search::query::SetupCandidatePriority::PcProbabilityFirst
    );
}
