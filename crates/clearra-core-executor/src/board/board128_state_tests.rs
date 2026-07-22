use super::*;

#[test]
fn board128_backend_places_and_clears_rows() {
    let layout = Board128Layout::standard_10_by_lines(12).expect("layout");
    let full_bottom = 0x03ff_u128;
    let single_cell_on_second_row = 1_u128 << 10;
    let board =
        Board128State::new(layout, full_bottom | single_cell_on_second_row).expect("valid board");

    assert_eq!(board.backend_kind(), BoardBackendKind::Board128);
    assert_eq!(board.occupied_count(), 11);
    let (cleared, lines) = board.clear_full_rows();

    assert_eq!(lines, 1);
    assert_eq!(cleared.occupied(), 1);
}

#[test]
fn board128_backend_rejects_outside_layout_masks() {
    let layout = Board128Layout::standard_10_by_lines(12).expect("layout");
    let board = Board128State::empty(layout);

    assert_eq!(
        board.place_mask(&(1_u128 << 127)),
        Err(BoardBackendError::MaskOutsideLayout)
    );
}
