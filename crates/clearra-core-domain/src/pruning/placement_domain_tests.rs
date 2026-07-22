use super::*;

fn key() -> PlacementDomainKey {
    PlacementDomainKey {
        component_key: ComponentKey(1),
        clear_state_key: ClearStateKey(2),
        board_profile_id: BoardProfileId(3),
        piece_set_id: PieceSetId(4),
    }
}

#[test]
fn forced_piece_family_conditional_under_clear_state() {
    let domain = PlacementDomain::new(key(), vec![PlacementId(7)], PieceFamilyMask(0b0100))
        .with_forced_piece_family(PieceFamily(2));

    assert_eq!(domain.forced_piece_family, Some(PieceFamily(2)));
    assert_eq!(domain.proof_level(), ProofLevel::ClearStateConditional);
}

#[test]
fn cell_domain_empty_is_clear_state_conditional() {
    let domain = PlacementDomain::new(key(), Vec::new(), PieceFamilyMask(0));

    assert!(domain.is_empty_under_clear_state());
}
