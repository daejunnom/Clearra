use crate::builder::cell_universe_builder::CellUniverseBuilder;

use super::*;

#[test]
fn generic_exact_cover_candidate_maps_cells_to_compact_columns() {
    let universe = CellUniverseBuilder::universe_from_cells([2, 5, 9]).expect("universe");
    let candidate =
        GenericExactCoverCandidate::from_cells(7, "custom:tri", 2, vec![5, 9], &universe)
            .expect("candidate");

    assert_eq!(candidate.candidate_id(), 7);
    assert_eq!(candidate.piece_id(), "custom:tri");
    assert_eq!(candidate.piece_area(), 2);
    assert_eq!(candidate.cells(), &[5, 9]);
    assert_eq!(candidate.columns(), &[1, 2]);
    assert_eq!(
        candidate.to_exact_cover_candidate(),
        ExactCoverCandidate::new(7, vec![1, 2])
    );
}

#[test]
fn generic_exact_cover_candidate_rejects_area_mismatch() {
    let universe = CellUniverseBuilder::universe_from_cells([0, 1, 2]).expect("universe");

    assert_eq!(
        GenericExactCoverCandidate::from_cells(3, "bad", 4, vec![0, 1], &universe),
        Err(
            GenericExactCoverCandidateError::PieceAreaDoesNotMatchCells {
                candidate_id: 3,
                piece_area: 4,
                cell_count: 2
            }
        )
    );
}
