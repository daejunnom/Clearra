use super::prefix_pruning::selected_first_clear_stalls;
use super::GeometryCatalog;

#[path = "geometry_first_clear_feedback_fixture.rs"]
mod feedback;

#[test]
fn first_clear_preserves_recorded_positive_candidates_and_cuts_fifteen_negatives() {
    // Receipt 1795402892422711228-3-runtime predates first-clear pruning.
    // Its verdict includes source/Hold/EOF, so a negative is not assumed to
    // be a physical impossibility. Only the new sufficient negative proof
    // below can eliminate it. No BuildUp or full-family search runs here.
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(10, 6, 0, (1_u64 << 60) - 1)
            .unwrap();
    let mut positives = 0;
    let mut pruned = 0;
    let mut unknown_negatives = 0;
    for &(accepted, placements) in feedback::SAMPLES {
        let rows = placements
            .iter()
            .map(|&(piece, cells)| {
                catalog
                    .skeleton_id(piece, cells)
                    .expect("exact recorded catalog pose")
            })
            .collect::<Vec<_>>();
        let rejected = selected_first_clear_stalls(&catalog, &rows, None);
        if accepted {
            positives += 1;
            assert!(
                !rejected,
                "recorded source-valid physical solution must survive"
            );
        } else if rejected {
            pruned += 1;
        } else {
            unknown_negatives += 1;
        }
    }
    assert_eq!(positives, 16);
    assert_eq!(pruned, 15);
    assert_eq!(unknown_negatives, 1);
}

#[test]
fn first_clear_unknown_inputs_never_supply_negative_authority() {
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, (1_u64 << 16) - 1)
            .unwrap();
    assert!(!selected_first_clear_stalls(&catalog, &[], None));
    assert!(!selected_first_clear_stalls(&catalog, &[u32::MAX], None));
    let row = catalog
        .skeleton_id(clearra_core_domain::piece::piece_kind::PieceKind::O, 0x33)
        .unwrap();
    assert!(!selected_first_clear_stalls(&catalog, &[row, row], None));
    assert!(!selected_first_clear_stalls(&catalog, &[row; 16], None));
}
