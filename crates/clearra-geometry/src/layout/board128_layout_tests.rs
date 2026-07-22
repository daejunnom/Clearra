use super::*;

#[test]
fn accepts_standard_twelve_line_analysis_layout() {
    let layout = Board128Layout::standard_10_by_lines(12).expect("10x12 fits in u128");

    assert_eq!(layout.width(), 10);
    assert_eq!(layout.height(), 12);
    assert_eq!(layout.cell_count(), 120);
    assert_eq!(layout.backend_kind(), BoardBackendKind::Board128);
}

#[test]
fn rejects_layouts_that_do_not_fit_in_u128() {
    let size = BoardSize::standard_10x20();

    assert_eq!(
        Board128Layout::new(size),
        Err(Board128LayoutError::TooManyCells { area: 200 })
    );
}

#[test]
fn rejects_layouts_that_belong_to_board64_fast_path() {
    let size = BoardSize::new(8, 8).expect("board64-sized");

    assert_eq!(
        Board128Layout::new(size),
        Err(Board128LayoutError::TooFewCells { area: 64 })
    );
}
