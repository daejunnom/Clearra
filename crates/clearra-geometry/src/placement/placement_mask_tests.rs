use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;

use super::*;

#[test]
fn creates_expected_o_piece_mask_at_origin() {
    let layout = Board64Layout::standard_10_by_lines(6).expect("10x6 fits in u64");
    let registry = standard_tetromino_registry();
    let o_piece = registry.get(PieceKind::O).expect("O piece is registered");

    let placement =
        PlacementMask::new(layout, o_piece, RotationState::Zero, 0, 0).expect("valid mask");

    assert_eq!(placement.mask(), 0b11 | (0b11 << 10));
}

#[test]
fn rejects_out_of_bounds_placement() {
    let layout = Board64Layout::standard_10_by_lines(6).expect("10x6 fits in u64");
    let registry = standard_tetromino_registry();
    let i_piece = registry.get(PieceKind::I).expect("I piece is registered");

    assert_eq!(
        PlacementMask::new(layout, i_piece, RotationState::Zero, 7, 0),
        Err(PlacementMaskError::OutOfBounds)
    );
}
