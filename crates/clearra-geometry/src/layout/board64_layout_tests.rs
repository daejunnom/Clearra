use super::*;

#[test]
fn accepts_standard_six_line_analysis_layout() {
    let layout = Board64Layout::standard_10_by_lines(6).expect("10x6 fits in u64");

    assert_eq!(layout.width(), 10);
    assert_eq!(layout.height(), 6);
    assert_eq!(layout.cell_count(), 60);
    assert_eq!(layout.backend_kind(), BoardBackendKind::Board64);
}

#[test]
fn rejects_layouts_that_do_not_fit_in_u64() {
    let size = BoardSize::standard_10x20();

    assert_eq!(
        Board64Layout::new(size),
        Err(Board64LayoutError::TooManyCells { area: 200 })
    );
}
