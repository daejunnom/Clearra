use super::*;

#[test]
fn assembles_fixed_queue_query() {
    let query = SetupQueryAssembler::assemble(&SetupArgs::new("I,T,O", true)).expect("setup query");

    assert_eq!(query.queue().len(), 3);
    assert!(query.queue().fixed_queue().is_some());
}

#[test]
fn rejects_unknown_queue_piece() {
    assert_eq!(
        SetupQueryAssembler::assemble(&SetupArgs::new("IX", false)),
        Err(SetupQueryAssemblyError::UnknownPiece { value: 'X' })
    );
}
