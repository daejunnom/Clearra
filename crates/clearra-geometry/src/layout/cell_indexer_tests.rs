use super::*;

#[test]
fn uses_row_major_bottom_left_indexing() {
    let layout = Board64Layout::standard_10_by_lines(6).expect("10x6 fits in u64");

    assert_eq!(try_cell_index(layout, 0, 0), Ok(0));
    assert_eq!(try_cell_index(layout, 9, 5), Ok(59));
    assert_eq!(
        coord_for_index(layout, 59),
        Some(CellCoord::new_unchecked(9, 5))
    );
}

#[test]
fn indexes_board128_layouts_with_same_row_major_contract() {
    let layout = Board128Layout::standard_10_by_lines(12).expect("10x12 fits in u128");

    assert_eq!(try_cell_index128(layout, 0, 0), Ok(0));
    assert_eq!(try_cell_index128(layout, 9, 11), Ok(119));
    assert_eq!(
        coord_for_index128(layout, 119),
        Some(CellCoord::new_unchecked(9, 11))
    );
}
