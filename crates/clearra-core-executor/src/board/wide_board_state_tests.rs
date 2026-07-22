use clearra_core_domain::board::board_size::BoardSize;

use super::*;

#[test]
fn wide_board_backend_places_and_clears_custom_width_rows() {
    let layout = WideBoardLayout::new(BoardSize::new(12, 4).expect("size"));
    let full_bottom = 0..12;
    let board = WideBoardState::new(layout, full_bottom.chain([12])).expect("board");

    assert_eq!(board.backend_kind(), BoardBackendKind::Wide);
    assert_eq!(board.occupied_count(), 13);
    let (cleared, lines) = board.clear_full_rows();

    assert_eq!(lines, 1);
    assert_eq!(
        cleared.occupied_cells().iter().copied().collect::<Vec<_>>(),
        vec![0]
    );
}

#[test]
fn wide_board_backend_rejects_outside_layout_masks() {
    let layout = WideBoardLayout::new(BoardSize::new(12, 4).expect("size"));
    let board = WideBoardState::empty(layout);

    assert_eq!(
        board.place_mask(&WideBoardMask::new([48])),
        Err(BoardBackendError::MaskOutsideLayout)
    );
}
